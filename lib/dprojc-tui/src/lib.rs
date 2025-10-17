use std::io;
use std::path::PathBuf;
use tokio::sync::mpsc;
use serde::{Deserialize, Serialize};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};

use dprojc_types::{Project, ScanResult};
use dprojc_scanner::SharedScanner;

use dprojc_utils::get_project_type_priority;
use fuzzy_matcher::FuzzyMatcher;

/// TUI-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    /// Paths to scan for projects
    pub scan_paths: Vec<std::path::PathBuf>,
    /// Maximum number of projects to display
    pub max_display_projects: Option<usize>,
    /// Whether to show project details by default
    pub show_details: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        let mut scan_paths = Vec::new();
        if let Ok(current_dir) = std::env::current_dir() {
            scan_paths.push(current_dir);
        }
        if let Some(home_dir) = dirs::home_dir() {
            scan_paths.push(home_dir);
        }

        Self {
            scan_paths,
            max_display_projects: None,
            show_details: false,
        }
    }
}

impl TuiConfig {
    /// Load configuration from file and environment variables
    pub fn load() -> anyhow::Result<Self> {
        Self::load_with_overrides(None, None)
    }

    /// Load configuration with optional overrides
    pub fn load_with_overrides(max_display: Option<usize>, show_details: Option<bool>) -> anyhow::Result<Self> {
        let mut config = Self::default();

        // Try to load from config file
        if let Some(file_config) = Self::load_from_file()? {
            config.merge(file_config);
        }

        // Load from environment variables
        Self::load_from_env(&mut config)?;

        // Apply overrides
        if let Some(max_display) = max_display {
            config.max_display_projects = Some(max_display);
        }
        if let Some(show_details) = show_details {
            config.show_details = show_details;
        }

        Ok(config)
    }

    /// Load configuration from a TOML file
    fn load_from_file() -> anyhow::Result<Option<Self>> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        let config_file = config_dir.join("durable-project-catalog").join("tui.toml");

        if !config_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(config_file)?;
        let config: Self = toml::from_str(&content)?;
        Ok(Some(config))
    }

    /// Load configuration from environment variables
    fn load_from_env(config: &mut Self) -> anyhow::Result<()> {
        if let Ok(paths_str) = std::env::var("DURABLE_SCAN_PATHS") {
            config.scan_paths = paths_str
                .split(',')
                .map(|s| s.trim().into())
                .filter(|p: &std::path::PathBuf| p.exists())
                .collect();
        }

        if let Ok(max_display) = std::env::var("DURABLE_MAX_DISPLAY_PROJECTS") {
            if let Ok(num) = max_display.parse::<usize>() {
                config.max_display_projects = Some(num);
            }
        }

        if let Ok(show_details) = std::env::var("DURABLE_SHOW_DETAILS") {
            config.show_details = show_details.parse::<bool>().unwrap_or(false);
        }

        Ok(())
    }

    /// Merge another config into this one
    fn merge(&mut self, other: Self) {
        if !other.scan_paths.is_empty() {
            self.scan_paths = other.scan_paths;
        }
        if other.max_display_projects.is_some() {
            self.max_display_projects = other.max_display_projects;
        }
        self.show_details = other.show_details;
    }

    /// Save configuration to file
    pub fn save(&self) -> anyhow::Result<()> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
            .join("durable-project-catalog");
        std::fs::create_dir_all(&config_dir)?;

        let config_file = config_dir.join("tui.toml");
        let content = toml::to_string_pretty(self)?;
        std::fs::write(config_file, content)?;
        Ok(())
    }
}

/// Messages sent from scan tasks to the main app
#[derive(Debug, Clone)]
enum ScanResultMessage {
    /// Scan progress update (current, total)
    Progress(usize, usize),
    /// Scan completed successfully
    Success(Vec<ScanResult>),
    /// Scan failed with error
    Error(String),
}

/// Commands sent to scan tasks
#[derive(Debug)]
enum ScanCommand {
    /// Start scanning the configured paths
    Scan,
}

/// Main TUI application
pub struct App {
    /// Current state of the application
    state: AppState,
    /// Current projects
    projects: Vec<Project>,
    /// Filtered projects for display
    filtered_projects: Vec<Project>,
    /// Current search query
    search_query: String,
    /// Selected project index
    selected_index: usize,
    /// Current view
    current_view: View,
    /// Should quit
    should_quit: bool,
    /// Channel for receiving scan results
    scan_result_rx: mpsc::UnboundedReceiver<ScanResultMessage>,
    /// Channel for sending scan commands
    scan_command_tx: mpsc::UnboundedSender<ScanCommand>,
    /// Last scan errors
    scan_errors: Vec<String>,
    /// TUI configuration
    config: TuiConfig,
    /// Scanning progress (current/total paths)
    scan_progress: Option<(usize, usize)>,
    /// Current sort mode
    sort_mode: SortMode,
    /// Selected path to output (when user presses Enter to select)
    selected_path: Option<PathBuf>,
}

/// Application state
#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    /// Loading/initializing
    Loading,
    /// Displaying projects
    Browsing,
    /// Searching projects
    Searching,
    /// Viewing project details
    Details,
    /// Scanning directories
    Scanning,
}

/// Current view in the application
#[derive(Debug, Clone, PartialEq)]
pub enum View {
    /// Main project list
    ProjectList,
    /// Project details
    ProjectDetails,
    /// Help screen
    Help,
    /// Error screen
    Errors,
}

/// Sorting options for projects
#[derive(Debug, Clone, PartialEq)]
pub enum SortMode {
    /// Sort by path (default)
    Path,
    /// Sort by project type
    Type,
    /// Sort by last scanned date
    Date,
}

impl Default for App {
    fn default() -> Self {
        let (scan_command_tx, _scan_command_rx) = mpsc::unbounded_channel();
        let (_scan_result_tx, scan_result_rx) = mpsc::unbounded_channel();

        let config = TuiConfig::default();

        // Start the scan task (only in non-test mode)
        #[cfg(not(test))]
        {
            let scan_paths_clone = config.scan_paths.clone();
            let scan_command_rx = _scan_command_rx;
            let scan_result_tx = _scan_result_tx;
            tokio::spawn(async move {
                scan_task(scan_command_rx, scan_result_tx, scan_paths_clone).await;
            });
        }

        Self {
            state: AppState::Loading,
            projects: Vec::new(),
            filtered_projects: Vec::new(),
            search_query: String::new(),
            selected_index: 0,
            current_view: View::ProjectList,
            should_quit: false,
            scan_result_rx,
            scan_command_tx,
            scan_errors: Vec::new(),
            config,
            scan_progress: None,
            sort_mode: SortMode::Path,
            selected_path: None,
        }
    }
}

impl App {
    /// Create a new application instance
    pub fn new() -> Self {
        let mut app = Self::default();

        // Override config with loaded config
        if let Ok(config) = TuiConfig::load() {
            app.config = config;
        }

        // Note: Scan configuration is loaded in the scan_task

        app
    }

    /// Create a new application instance with custom TUI configuration
    pub fn with_tui_config(config: TuiConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Run the TUI application
    pub async fn run(&mut self) -> anyhow::Result<()> {
        // Setup terminal - use /dev/tty to work with shell command substitution
        enable_raw_mode()?;

        // Try to open /dev/tty for input when stdout is redirected
        // This allows the TUI to work in command substitution: result=$(dpc tui)
        if let Ok(mut tty) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
        {
            execute!(tty, EnterAlternateScreen, EnableMouseCapture)?;
            let backend = CrosstermBackend::new(tty);
            let mut terminal = Terminal::new(backend)?;
            return self.run_loop(&mut terminal).await;
        }

        // Fallback to stdout if /dev/tty unavailable
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        self.run_loop(&mut terminal).await
    }

    /// Main event loop (extracted to work with different backend types)
    async fn run_loop<B: Backend + std::io::Write>(&mut self, terminal: &mut Terminal<B>) -> anyhow::Result<()> {
        // Load initial data from database (instant)
        self.load_projects().await?;

        // Main event loop
        loop {
            // Draw the UI
            terminal.draw(|f| self.draw(f))?;

            // Handle incoming scan results
            while let Ok(message) = self.scan_result_rx.try_recv() {
                self.handle_scan_result(message);
            }

            // Handle events
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key.code);
                }
            }

            // Check if we should quit
            if self.should_quit {
                break;
            }
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        // Output selected path for shell integration
        if let Some(path) = &self.selected_path {
            println!("{}", path.display());
        }

        Ok(())
    }

    /// Handle scan result messages from background tasks
    fn handle_scan_result(&mut self, message: ScanResultMessage) {
        match message {
            ScanResultMessage::Success(results) => {
                self.projects.clear();
                self.scan_errors.clear();
                self.scan_progress = None;

                for result in results {
                    self.projects.extend(result.projects);
                    // Collect any errors from the scan
                    for error in result.errors {
                        self.scan_errors.push(format!("{}: {}", error.path.display(), error.message));
                    }
                }

                // Sort projects according to current sort mode
                self.sort_projects();

                // Initialize filtered projects
                self.filtered_projects = self.projects.clone();
                self.selected_index = 0;
                self.state = AppState::Browsing;
            }
            ScanResultMessage::Error(err) => {
                self.scan_errors.push(err);
                self.scan_progress = None;
                self.state = AppState::Browsing; // Still allow browsing even with errors
            }
            ScanResultMessage::Progress(current, total) => {
                self.scan_progress = Some((current + 1, total));
            }
        }
    }

    /// Load projects from database first, then optionally scan in background
    async fn load_projects(&mut self) -> anyhow::Result<()> {
        // Load from database first for instant results
        let db_path = dprojc_utils::default_db_path()?;

        if let Ok(db) = dprojc_db::ProjectDatabase::open(&db_path) {
            if let Ok(projects) = db.get_all_projects() {
                if !projects.is_empty() {
                    self.projects = projects;
                    self.sort_projects();
                    self.filtered_projects = self.projects.clone();
                    self.state = AppState::Browsing;
                    return Ok(());
                }
            }
        }

        // Fallback to scanning if database is empty or unavailable
        self.state = AppState::Scanning;
        let _ = self.scan_command_tx.send(ScanCommand::Scan);
        Ok(())
    }

    /// Handle keyboard input
    fn handle_key(&mut self, key: KeyCode) {
        match self.current_view {
            View::ProjectList => self.handle_project_list_key(key),
            View::ProjectDetails => self.handle_details_key(key),
            View::Help => self.handle_help_key(key),
            View::Errors => self.handle_errors_key(key),
        }
    }

    /// Handle keys in project list view
    fn handle_project_list_key(&mut self, key: KeyCode) {
        // Handle search input first when in searching state
        if self.state == AppState::Searching {
            match key {
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.update_filtered_projects();
                    return;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.update_filtered_projects();
                    return;
                }
                KeyCode::Enter | KeyCode::Esc => {
                    self.state = AppState::Browsing;
                    self.search_query.clear();
                    self.update_filtered_projects();
                    return;
                }
                _ => {}
            }
        }

        match key {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected_index < self.filtered_projects.len().saturating_sub(1) {
                    self.selected_index += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Enter => {
                if !self.filtered_projects.is_empty() {
                    // Save the selected path and quit (for shell integration)
                    self.selected_path = Some(self.filtered_projects[self.selected_index].path.clone());
                    self.should_quit = true;
                }
            }
            KeyCode::Char('/') => {
                self.state = AppState::Searching;
            }
            KeyCode::Char('?') => {
                self.current_view = View::Help;
            }
            KeyCode::Char('e') => {
                if !self.scan_errors.is_empty() {
                    self.current_view = View::Errors;
                }
            }
            KeyCode::Char('r') => {
                // Refresh - reload projects
                self.state = AppState::Scanning;
                self.scan_errors.clear();
                let _ = self.scan_command_tx.send(ScanCommand::Scan);
            }
            KeyCode::Char('o') => {
                if !self.filtered_projects.is_empty() {
                    self.open_in_editor();
                }
            }
            KeyCode::Char('t') => {
                if !self.filtered_projects.is_empty() {
                    self.open_in_terminal();
                }
            }
            KeyCode::Char('s') => {
                self.cycle_sort_mode();
                self.sort_projects();
                self.filtered_projects = self.projects.clone();
                self.selected_index = 0;
            }
            _ => {}
        }


    }

    /// Handle keys in project details view
    fn handle_details_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.current_view = View::ProjectList;
            }
            _ => {}
        }
    }

    /// Handle keys in help view
    fn handle_help_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
                self.current_view = View::ProjectList;
            }
            _ => {}
        }
    }

    /// Handle keys in errors view
    fn handle_errors_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('e') => {
                self.current_view = View::ProjectList;
            }
            _ => {}
        }
    }

    /// Sort projects according to the current sort mode
    fn sort_projects(&mut self) {
        match self.sort_mode {
            SortMode::Path => {
                self.projects.sort_by(|a, b| a.path.cmp(&b.path));
            }
            SortMode::Type => {
                self.projects.sort_by(|a, b| {
                    let priority_a = get_project_type_priority(&a.project_type);
                    let priority_b = get_project_type_priority(&b.project_type);
                    let priority_cmp = priority_b.cmp(&priority_a); // Higher priority first
                    if priority_cmp == std::cmp::Ordering::Equal {
                        let type_cmp = a.project_type.cmp(&b.project_type);
                        if type_cmp == std::cmp::Ordering::Equal {
                            a.path.cmp(&b.path)
                        } else {
                            type_cmp
                        }
                    } else {
                        priority_cmp
                    }
                });
            }
            SortMode::Date => {
                self.projects.sort_by(|a, b| {
                    let date_cmp = b.last_scanned.cmp(&a.last_scanned); // Newest first
                    if date_cmp == std::cmp::Ordering::Equal {
                        a.path.cmp(&b.path)
                    } else {
                        date_cmp
                    }
                });
            }
        }
    }

    /// Update filtered projects based on search query
    fn update_filtered_projects(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_projects = self.projects.clone();
        } else {
            let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
            self.filtered_projects = self
                .projects
                .iter()
                .filter(|project| {
                    let path_str = project.path.to_string_lossy();
                    let type_str = format!("{:?}", project.project_type);
                    matcher.fuzzy_match(&path_str, &self.search_query).is_some()
                        || matcher.fuzzy_match(&type_str, &self.search_query).is_some()
                })
                .cloned()
                .collect();

            // Reset selection if out of bounds
            if self.selected_index >= self.filtered_projects.len() {
                self.selected_index = self.filtered_projects.len().saturating_sub(1);
            }
        }
    }

    /// Draw the UI
    fn draw<B: Backend>(&self, f: &mut Frame<B>) {
        let size = f.size();

        match self.current_view {
            View::ProjectList => self.draw_project_list(f, size),
            View::ProjectDetails => self.draw_project_details(f, size),
            View::Help => self.draw_help(f, size),
            View::Errors => self.draw_errors(f, size),
        }
    }

    /// Draw the project list view
    fn draw_project_list<B: Backend>(&self, f: &mut Frame<B>, size: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(1),    // Project list
                Constraint::Length(1), // Status bar
            ])
            .split(size);

        // Project list
        let max_items = self.config.max_display_projects.unwrap_or(usize::MAX);
        let display_projects = &self.filtered_projects[..self.filtered_projects.len().min(max_items)];

        // Header
        let shown_count = display_projects.len();
        let header_text = format!("Durable Project Catalog\nFound {} projects | Showing {} projects",
            self.projects.len(), shown_count);
        let header = Paragraph::new(header_text)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).title("Header"));
        f.render_widget(header, chunks[0]);

        let items: Vec<ListItem> = display_projects
            .iter()
            .enumerate()
            .map(|(i, project)| {
                let style = if i == self.selected_index {
                    Style::default().bg(Color::Blue).fg(Color::White)
                } else {
                    Style::default()
                };

                let path_display = dprojc_utils::format_path_display(&project.path);
                let type_display = format!("{:?}", project.project_type);
                let content = format!("{} {}\nLast scanned: {}",
                    path_display,
                    type_display,
                    project.last_scanned.format("%Y-%m-%d %H:%M"));

                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Projects"))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        f.render_widget(list, chunks[1]);

        // Status bar
        let status_text = match self.state {
            AppState::Loading => "Loading...",
            AppState::Browsing => {
                let sort_indicator = match self.sort_mode {
                    SortMode::Path => "Sort: Path",
                    SortMode::Type => "Sort: Type",
                    SortMode::Date => "Sort: Date",
                };
                if self.scan_errors.is_empty() {
                    &format!("Browsing projects | {} | Press / to search, s to change sort, ? for help, q to quit", sort_indicator)
                } else {
                    &format!("Browsing projects | {} | {} errors during scan | Press / to search, s to change sort, ? for help, q to quit", sort_indicator, self.scan_errors.len())
                }
            }
            AppState::Searching => &format!("Search: {}", self.search_query),
            AppState::Details => "Viewing project details | Press Esc to go back",
            AppState::Scanning => {
                if let Some((current, total)) = self.scan_progress {
                    &format!("Scanning directories... ({}/{})", current, total)
                } else {
                    "Scanning directories..."
                }
            }
        };

        let status_color = if self.scan_errors.is_empty() { Color::Blue } else { Color::Red };
        let status = Paragraph::new(status_text)
            .style(Style::default().bg(status_color).fg(Color::White))
            .alignment(Alignment::Center);
        f.render_widget(status, chunks[2]);
    }

    /// Draw the project details view
    fn draw_project_details<B: Backend>(&self, f: &mut Frame<B>, size: Rect) {
        if self.filtered_projects.is_empty() {
            return;
        }

        let project = &self.filtered_projects[self.selected_index];

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(1),    // Details
                Constraint::Length(1), // Footer
            ])
            .split(size);

        // Title
        let title_text = format!("Project Details\n{}", dprojc_utils::format_path_display(&project.path));
        let title = Paragraph::new(title_text)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);

        // Details
        let details_text = format!("Type: {:?}\nPath: {}\nLast Scanned: {}\nIndicators: {:?}",
            project.project_type,
            project.path.display(),
            project.last_scanned,
            project.indicators);

        let details_paragraph = Paragraph::new(details_text)
            .block(Block::default().borders(Borders::ALL).title("Details"));
        f.render_widget(details_paragraph, chunks[1]);

        // Footer
        let footer = Paragraph::new("Press Esc to go back")
            .style(Style::default().bg(Color::Blue).fg(Color::White))
            .alignment(Alignment::Center);
        f.render_widget(footer, chunks[2]);
    }

    /// Draw the help view
    fn draw_help<B: Backend>(&self, f: &mut Frame<B>, size: Rect) {
        let help_text = "Keyboard Shortcuts:\n\nNavigation:\n  ↑/k - Move up\n  ↓/j - Move down\n  Enter - View project details\n\nActions:\n  / - Search projects\n  s - Cycle sort mode (Path/Type/Date)\n  r - Refresh/scan again\n  o - Open in editor\n  t - Open in terminal\n  e - Show scan errors (if any)\n  ? - Show this help\n  q/Esc - Quit or go back\n\nSearch:\n  Type to search by path or project type\n  Fuzzy matching is supported\n  Press Enter or Esc to exit search";

        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help, size);
    }

    /// Open the selected project in the default editor
    fn open_in_editor(&self) {
        if let Some(project) = self.filtered_projects.get(self.selected_index) {
            // Try common editors in order of preference
            let editors = ["code", "cursor", "vim", "nano", "emacs"];
            for editor in &editors {
                let output = std::process::Command::new(editor)
                    .arg(&project.path)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();

                if output.is_ok() {
                    break;
                }
            }
        }
    }

    /// Open the selected project in a terminal
    fn open_in_terminal(&self) {
        if let Some(project) = self.filtered_projects.get(self.selected_index) {
            // Try to open terminal in the project directory
            let terminals = ["gnome-terminal", "konsole", "xterm", "alacritty", "kitty"];
            for terminal in &terminals {
                let output = std::process::Command::new(terminal)
                    .arg("--working-directory")
                    .arg(&project.path)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();

                if output.is_ok() {
                    break;
                }
            }
        }
    }

    /// Cycle through sort modes
    fn cycle_sort_mode(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Path => SortMode::Type,
            SortMode::Type => SortMode::Date,
            SortMode::Date => SortMode::Path,
        };
    }

    /// Draw the errors view
    fn draw_errors<B: Backend>(&self, f: &mut Frame<B>, size: Rect) {
        let error_text: String = self.scan_errors.iter().enumerate()
            .map(|(i, error)| format!("{}. {}", i + 1, error))
            .collect::<Vec<_>>()
            .join("\n");

        let errors = Paragraph::new(error_text)
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title("Scan Errors"));
        f.render_widget(errors, size);
    }
}

/// Background task that handles scanning operations
async fn scan_task(
    mut command_rx: mpsc::UnboundedReceiver<ScanCommand>,
    result_tx: mpsc::UnboundedSender<ScanResultMessage>,
    scan_paths: Vec<std::path::PathBuf>,
) {
    let scanner = if let Ok(scan_config) = dprojc_config::ConfigManager::load_config() {
        SharedScanner::with_config(scan_config)
    } else {
        SharedScanner::new()
    };

    while let Some(command) = command_rx.recv().await {
        match command {
            ScanCommand::Scan => {
                let total_paths = scan_paths.len();
                let mut results = Vec::new();

                for (i, path) in scan_paths.iter().enumerate() {
                    let _ = result_tx.send(ScanResultMessage::Progress(i, total_paths));

                    match scanner.scan(path).await {
                        Ok(result) => results.push(result),
                        Err(err) => {
                            let _ = result_tx.send(ScanResultMessage::Error(format!("Failed to scan {}: {}", path.display(), err)));
                            continue;
                        }
                    }
                }

                let _ = result_tx.send(ScanResultMessage::Success(results));
            }
        }
    }
}

/// Run the TUI application with default configuration
pub async fn run_tui() -> anyhow::Result<()> {
    let mut app = App::new();
    app.run().await
}

/// Run the TUI application with custom configuration
pub async fn run_tui_with_config(config: TuiConfig) -> anyhow::Result<()> {
    let mut app = App::with_tui_config(config);
    app.run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use dprojc_types::{Project, ProjectType, ProjectIndicator, ScanError, ScanErrorType};
    use std::path::PathBuf;
    use chrono::Utc;

    fn create_test_project(path: &str, project_type: ProjectType) -> Project {
        Project {
            path: PathBuf::from(path),
            project_type,
            indicators: vec![ProjectIndicator::GitDirectory],
            last_scanned: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_app_initialization() {
        let app = App::new();
        assert_eq!(app.state, AppState::Loading);
        assert!(app.projects.is_empty());
        assert!(app.filtered_projects.is_empty());
        assert!(app.search_query.is_empty());
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.current_view, View::ProjectList);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_update_filtered_projects_no_search() {
        let (scan_command_tx, _scan_command_rx) = mpsc::unbounded_channel::<ScanCommand>();
        let (_scan_result_tx, scan_result_rx) = mpsc::unbounded_channel::<ScanResultMessage>();
        let mut app = App {
            state: AppState::Loading,
            projects: vec![
                create_test_project("/path/to/rust", ProjectType::Rust),
                create_test_project("/path/to/node", ProjectType::NodeJs),
            ],
            filtered_projects: Vec::new(),
            search_query: String::new(),
            selected_index: 0,
            current_view: View::ProjectList,
            should_quit: false,
            scan_result_rx,
            scan_command_tx,
            scan_errors: Vec::new(),
            config: TuiConfig::default(),
            scan_progress: None,
            sort_mode: SortMode::Path,
        };
        app.update_filtered_projects();

        assert_eq!(app.filtered_projects.len(), 2);
        assert_eq!(app.filtered_projects[0].project_type, ProjectType::Rust);
        assert_eq!(app.filtered_projects[1].project_type, ProjectType::NodeJs);
    }

    #[test]
    fn test_update_filtered_projects_with_search() {
        let (scan_command_tx, _scan_command_rx) = mpsc::unbounded_channel::<ScanCommand>();
        let (_scan_result_tx, scan_result_rx) = mpsc::unbounded_channel::<ScanResultMessage>();
        let mut app = App {
            state: AppState::Loading,
            projects: vec![
                create_test_project("/path/to/rust", ProjectType::Rust),
                create_test_project("/path/to/node", ProjectType::NodeJs),
            ],
            filtered_projects: Vec::new(),
            search_query: "rust".to_string(),
            selected_index: 0,
            current_view: View::ProjectList,
            should_quit: false,
            scan_result_rx,
            scan_command_tx,
            scan_errors: Vec::new(),
            config: TuiConfig::default(),
            scan_progress: None,
            sort_mode: SortMode::Path,
        };
        app.update_filtered_projects();

        assert_eq!(app.filtered_projects.len(), 1);
        assert_eq!(app.filtered_projects[0].project_type, ProjectType::Rust);
    }

    #[test]
    fn test_update_filtered_projects_empty_search() {
        let (scan_command_tx, _scan_command_rx) = mpsc::unbounded_channel::<ScanCommand>();
        let (_scan_result_tx, scan_result_rx) = mpsc::unbounded_channel::<ScanResultMessage>();
        let mut app = App {
            state: AppState::Loading,
            projects: vec![
                create_test_project("/path/to/rust", ProjectType::Rust),
                create_test_project("/path/to/node", ProjectType::NodeJs),
            ],
            filtered_projects: Vec::new(),
            search_query: "".to_string(),
            selected_index: 0,
            current_view: View::ProjectList,
            should_quit: false,
            scan_result_rx,
            scan_command_tx,
            scan_errors: Vec::new(),
            config: TuiConfig::default(),
            scan_progress: None,
            sort_mode: SortMode::Path,
        };
        app.update_filtered_projects();

        assert_eq!(app.filtered_projects.len(), 2);
    }

    #[test]
    fn test_handle_project_list_key_navigation() {
        let mut app = App::new();
        app.projects = vec![
            create_test_project("/path/to/rust", ProjectType::Rust),
            create_test_project("/path/to/node", ProjectType::NodeJs),
        ];
        app.filtered_projects = app.projects.clone();
        app.state = AppState::Browsing;

        // Test down navigation
        app.handle_project_list_key(crossterm::event::KeyCode::Down);
        assert_eq!(app.selected_index, 1);

        // Test down at end
        app.handle_project_list_key(crossterm::event::KeyCode::Down);
        assert_eq!(app.selected_index, 1); // Should not go beyond

        // Test up navigation
        app.handle_project_list_key(crossterm::event::KeyCode::Up);
        assert_eq!(app.selected_index, 0);

        // Test up at start
        app.handle_project_list_key(crossterm::event::KeyCode::Up);
        assert_eq!(app.selected_index, 0); // Should not go below 0
    }

    #[test]
    fn test_handle_project_list_key_enter() {
        let mut app = App::new();
        app.projects = vec![create_test_project("/path/to/rust", ProjectType::Rust)];
        app.filtered_projects = app.projects.clone();
        app.state = AppState::Browsing;

        app.handle_project_list_key(crossterm::event::KeyCode::Enter);
        assert_eq!(app.current_view, View::ProjectDetails);
    }

    #[test]
    fn test_handle_project_list_key_search() {
        let mut app = App::new();
        app.state = AppState::Browsing;

        app.handle_project_list_key(crossterm::event::KeyCode::Char('/'));
        assert_eq!(app.state, AppState::Searching);
    }

    #[test]
    fn test_handle_project_list_key_quit() {
        let mut app = App::new();

        app.handle_project_list_key(crossterm::event::KeyCode::Char('q'));
        assert!(app.should_quit);

        let mut app2 = App::new();
        app2.handle_project_list_key(crossterm::event::KeyCode::Esc);
        assert!(app2.should_quit);
    }

    #[test]
    fn test_handle_details_key() {
        let mut app = App::new();
        app.current_view = View::ProjectDetails;

        app.handle_details_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app.current_view, View::ProjectList);
    }

    #[test]
    fn test_handle_help_key() {
        let mut app = App::new();
        app.current_view = View::Help;

        app.handle_help_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app.current_view, View::ProjectList);

        let mut app2 = App::new();
        app2.current_view = View::Help;
        app2.handle_help_key(crossterm::event::KeyCode::Char('q'));
        assert_eq!(app2.current_view, View::ProjectList);

        let mut app3 = App::new();
        app3.current_view = View::Help;
        app3.handle_help_key(crossterm::event::KeyCode::Char('?'));
        assert_eq!(app3.current_view, View::ProjectList);
    }

    #[tokio::test]
    async fn test_search_input_handling() {
        let mut app = App::new();
        app.state = AppState::Searching;

        // Test adding characters
        app.handle_project_list_key(crossterm::event::KeyCode::Char('r'));
        app.handle_project_list_key(crossterm::event::KeyCode::Char('u'));
        app.handle_project_list_key(crossterm::event::KeyCode::Char('s'));
        assert_eq!(app.search_query, "rus");

        // Test backspace
        app.handle_project_list_key(crossterm::event::KeyCode::Backspace);
        assert_eq!(app.search_query, "ru");

        // Test enter to exit search
        app.handle_project_list_key(crossterm::event::KeyCode::Enter);
        assert_eq!(app.state, AppState::Browsing);

        // Test esc to exit search
        let mut app2 = App::new();
        app2.state = AppState::Searching;
        app2.handle_project_list_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app2.state, AppState::Browsing);
    }

    #[test]
    fn test_scan_result_handling_success() {
        let mut app = App::new();
        let project = create_test_project("/test/path", ProjectType::Rust);
        let scan_result = ScanResult {
            root_path: PathBuf::from("/test"),
            projects: vec![project.clone()],
            excluded_dirs: vec![],
            errors: vec![],
            dirs_scanned: 10,
            scan_duration_ms: 100,
        };

        app.handle_scan_result(ScanResultMessage::Success(vec![scan_result]));

        assert_eq!(app.state, AppState::Browsing);
        assert_eq!(app.projects.len(), 1);
        assert_eq!(app.projects[0].path, project.path);
        assert!(app.scan_errors.is_empty());
    }

    #[test]
    fn test_scan_result_handling_error() {
        let mut app = App::new();
        let error_msg = "Test error".to_string();

        app.handle_scan_result(ScanResultMessage::Error(error_msg.clone()));

        assert_eq!(app.state, AppState::Browsing);
        assert_eq!(app.scan_errors.len(), 1);
        assert_eq!(app.scan_errors[0], error_msg);
    }

    #[test]
    fn test_scan_result_handling_with_scan_errors() {
        let mut app = App::new();
        let project = create_test_project("/test/path", ProjectType::Rust);
        let scan_error = ScanError {
            path: PathBuf::from("/test/error"),
            error_type: ScanErrorType::PermissionDenied,
            message: "Permission denied".to_string(),
        };
        let scan_result = ScanResult {
            root_path: PathBuf::from("/test"),
            projects: vec![project],
            excluded_dirs: vec![],
            errors: vec![scan_error],
            dirs_scanned: 10,
            scan_duration_ms: 100,
        };

        app.handle_scan_result(ScanResultMessage::Success(vec![scan_result]));

        assert_eq!(app.state, AppState::Browsing);
        assert_eq!(app.projects.len(), 1);
        assert_eq!(app.scan_errors.len(), 1);
        assert!(app.scan_errors[0].contains("Permission denied"));
    }

    #[test]
    fn test_view_transitions() {
        let mut app = App::new();

        // Start in project list
        assert_eq!(app.current_view, View::ProjectList);

        // Go to help
        app.handle_project_list_key(crossterm::event::KeyCode::Char('?'));
        assert_eq!(app.current_view, View::Help);

        // Go back to project list
        app.handle_help_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app.current_view, View::ProjectList);

        // Go to details (need projects for this)
        app.projects = vec![create_test_project("/test", ProjectType::Rust)];
        app.filtered_projects = app.projects.clone();
        app.handle_project_list_key(crossterm::event::KeyCode::Enter);
        assert_eq!(app.current_view, View::ProjectDetails);

        // Go back to project list
        app.handle_details_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app.current_view, View::ProjectList);
    }

    #[test]
    fn test_scan_result_message_progress() {
        let progress_msg = ScanResultMessage::Progress(1, 5);
        match progress_msg {
            ScanResultMessage::Progress(current, total) => {
                assert_eq!(current, 1);
                assert_eq!(total, 5);
            }
            _ => panic!("Expected Progress message"),
        }
    }

    #[test]
    fn test_tui_config_default() {
        let config = TuiConfig::default();
        assert!(!config.scan_paths.is_empty()); // Should have current dir and home
        assert!(config.max_display_projects.is_none());
        assert!(!config.show_details);
    }

    #[test]
    fn test_tui_config_merge() {
        let mut config1 = TuiConfig {
            scan_paths: vec![std::path::PathBuf::from("/path1")],
            max_display_projects: Some(10),
            show_details: false,
        };

        let config2 = TuiConfig {
            scan_paths: vec![std::path::PathBuf::from("/path2")],
            max_display_projects: Some(20),
            show_details: true,
        };

        config1.merge(config2);
        assert_eq!(config1.scan_paths, vec![std::path::PathBuf::from("/path2")]);
        assert_eq!(config1.max_display_projects, Some(20));
        assert!(config1.show_details);
    }

    #[test]
    fn test_sort_projects_by_path() {
        let mut app = App::new();
        app.projects = vec![
            create_test_project("/z/path", ProjectType::Rust),
            create_test_project("/a/path", ProjectType::NodeJs),
        ];

        app.sort_mode = SortMode::Path;
        app.sort_projects();

        assert_eq!(app.projects[0].path, std::path::PathBuf::from("/a/path"));
        assert_eq!(app.projects[1].path, std::path::PathBuf::from("/z/path"));
    }

    #[test]
    fn test_sort_projects_by_type() {
        let mut app = App::new();
        app.projects = vec![
            create_test_project("/path/node", ProjectType::NodeJs),
            create_test_project("/path/rust", ProjectType::Rust),
        ];

        app.sort_mode = SortMode::Type;
        app.sort_projects();

        // Rust has higher priority than NodeJs
        assert_eq!(app.projects[0].project_type, ProjectType::Rust);
        assert_eq!(app.projects[1].project_type, ProjectType::NodeJs);
    }

    #[test]
    fn test_sort_projects_by_date() {
        let mut app = App::new();
        let old_date = chrono::Utc::now() - chrono::Duration::days(1);
        let new_date = chrono::Utc::now();

        let mut project1 = create_test_project("/path/old", ProjectType::Rust);
        project1.last_scanned = old_date;

        let mut project2 = create_test_project("/path/new", ProjectType::NodeJs);
        project2.last_scanned = new_date;

        app.projects = vec![project1, project2];

        app.sort_mode = SortMode::Date;
        app.sort_projects();

        // Newest first
        assert_eq!(app.projects[0].project_type, ProjectType::NodeJs);
        assert_eq!(app.projects[1].project_type, ProjectType::Rust);
    }

    #[test]
    fn test_cycle_sort_mode() {
        let mut app = App::new();

        assert_eq!(app.sort_mode, SortMode::Path);
        app.cycle_sort_mode();
        assert_eq!(app.sort_mode, SortMode::Type);
        app.cycle_sort_mode();
        assert_eq!(app.sort_mode, SortMode::Date);
        app.cycle_sort_mode();
        assert_eq!(app.sort_mode, SortMode::Path);
    }

    #[test]
    fn test_update_filtered_projects_special_characters() {
        let (scan_command_tx, _scan_command_rx) = mpsc::unbounded_channel::<ScanCommand>();
        let (_scan_result_tx, scan_result_rx) = mpsc::unbounded_channel::<ScanResultMessage>();
        let mut app = App {
            state: AppState::Loading,
            projects: vec![
                create_test_project("/path/to/rust-project", ProjectType::Rust),
                create_test_project("/path/to/node_project", ProjectType::NodeJs),
            ],
            filtered_projects: Vec::new(),
            search_query: "rust-project".to_string(),
            selected_index: 0,
            current_view: View::ProjectList,
            should_quit: false,
            scan_result_rx,
            scan_command_tx,
            scan_errors: Vec::new(),
            config: TuiConfig::default(),
            scan_progress: None,
            sort_mode: SortMode::Path,
        };
        app.update_filtered_projects();

        assert_eq!(app.filtered_projects.len(), 1);
        assert!(app.filtered_projects[0].path.to_string_lossy().contains("rust-project"));
    }

    #[test]
    fn test_update_filtered_projects_no_matches() {
        let (scan_command_tx, _scan_command_rx) = mpsc::unbounded_channel::<ScanCommand>();
        let (_scan_result_tx, scan_result_rx) = mpsc::unbounded_channel::<ScanResultMessage>();
        let mut app = App {
            state: AppState::Loading,
            projects: vec![
                create_test_project("/path/to/rust", ProjectType::Rust),
            ],
            filtered_projects: Vec::new(),
            search_query: "nonexistent".to_string(),
            selected_index: 0,
            current_view: View::ProjectList,
            should_quit: false,
            scan_result_rx,
            scan_command_tx,
            scan_errors: Vec::new(),
            config: TuiConfig::default(),
            scan_progress: None,
            sort_mode: SortMode::Path,
        };
        app.update_filtered_projects();

        assert!(app.filtered_projects.is_empty());
        assert_eq!(app.selected_index, 0); // Should reset to 0 when no matches
    }

    #[test]
    fn test_handle_project_list_key_sort_cycle() {
        let mut app = App::new();
        app.projects = vec![create_test_project("/test", ProjectType::Rust)];
        app.filtered_projects = app.projects.clone();
        app.state = AppState::Browsing;

        assert_eq!(app.sort_mode, SortMode::Path);
        app.handle_project_list_key(crossterm::event::KeyCode::Char('s'));
        assert_eq!(app.sort_mode, SortMode::Type);
        assert_eq!(app.selected_index, 0); // Should reset selection
    }

    #[test]
    fn test_handle_project_list_key_refresh() {
        let mut app = App::new();
        app.state = AppState::Browsing;

        app.handle_project_list_key(crossterm::event::KeyCode::Char('r'));
        assert_eq!(app.state, AppState::Scanning);
        assert!(app.scan_errors.is_empty()); // Should clear errors
    }

    #[test]
    fn test_handle_project_list_key_errors_view() {
        let mut app = App::new();
        app.scan_errors = vec!["error1".to_string(), "error2".to_string()];

        app.handle_project_list_key(crossterm::event::KeyCode::Char('e'));
        assert_eq!(app.current_view, View::Errors);
    }

    #[test]
    fn test_handle_project_list_key_errors_view_no_errors() {
        let mut app = App::new();
        app.scan_errors = vec![]; // No errors

        app.handle_project_list_key(crossterm::event::KeyCode::Char('e'));
        assert_eq!(app.current_view, View::ProjectList); // Should not switch
    }

    #[test]
    fn test_handle_errors_key() {
        let mut app = App::new();
        app.current_view = View::Errors;

        app.handle_errors_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app.current_view, View::ProjectList);
    }

    #[test]
    fn test_scan_result_progress() {
        let mut app = App::new();

        app.handle_scan_result(ScanResultMessage::Progress(2, 10));
        assert_eq!(app.scan_progress, Some((3, 10))); // Note: +1 in implementation
    }

    #[test]
    fn test_open_in_editor_no_projects() {
        let app = App::new();
        // Should not panic
        app.open_in_editor();
    }

    #[test]
    fn test_open_in_terminal_no_projects() {
        let app = App::new();
        // Should not panic
        app.open_in_terminal();
    }

    #[test]
    fn test_draw_project_details_empty_list() {
        let app = App::new();
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        // Should not panic
        terminal.draw(|f| app.draw_project_details(f, f.size())).unwrap();
    }

    #[test]
    fn test_draw_errors_empty() {
        let app = App::new();
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        // Should not panic even with no errors
        terminal.draw(|f| app.draw_errors(f, f.size())).unwrap();
    }

    #[test]
    fn test_config_save_and_load() {
        use tempfile::tempdir;
        use std::fs;

        let temp_dir = tempdir().unwrap();
        let config_dir = temp_dir.path().join("durable-project-catalog");
        fs::create_dir_all(&config_dir).unwrap();

        // Temporarily override the config dir for testing
        std::env::set_var("XDG_CONFIG_HOME", temp_dir.path());

        let config = TuiConfig {
            scan_paths: vec![temp_dir.path().join("test/path")], // Use existing temp dir
            max_display_projects: Some(50),
            show_details: true,
        };

        config.save().unwrap();

        let loaded_config = TuiConfig::load().unwrap();
        assert_eq!(loaded_config.scan_paths, config.scan_paths);
        assert_eq!(loaded_config.max_display_projects, config.max_display_projects);
        assert_eq!(loaded_config.show_details, config.show_details);

        // Clean up
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn test_config_load_from_env() {
        use tempfile::tempdir;

        // Ensure clean environment
        std::env::remove_var("DURABLE_SCAN_PATHS");
        std::env::remove_var("DURABLE_MAX_DISPLAY_PROJECTS");
        std::env::remove_var("DURABLE_SHOW_DETAILS");

        let temp_dir = tempdir().unwrap();
        let path1 = temp_dir.path().join("path1");
        let path2 = temp_dir.path().join("path2");
        std::fs::create_dir_all(&path1).unwrap();
        std::fs::create_dir_all(&path2).unwrap();

        std::env::set_var("DURABLE_SCAN_PATHS", format!("{},{}", path1.display(), path2.display()));
        std::env::set_var("DURABLE_MAX_DISPLAY_PROJECTS", "25");
        std::env::set_var("DURABLE_SHOW_DETAILS", "true");

        let mut config = TuiConfig::default();
        TuiConfig::load_from_env(&mut config).unwrap();

        assert_eq!(config.scan_paths.len(), 2);
        assert!(config.scan_paths.contains(&path1));
        assert!(config.scan_paths.contains(&path2));
        assert_eq!(config.max_display_projects, Some(25));
        assert!(config.show_details);

        // Clean up
        std::env::remove_var("DURABLE_SCAN_PATHS");
        std::env::remove_var("DURABLE_MAX_DISPLAY_PROJECTS");
        std::env::remove_var("DURABLE_SHOW_DETAILS");
    }

    #[tokio::test]
    async fn test_scan_task_integration() {
        use std::time::Duration;
        use tokio::time::timeout;

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (result_tx, mut result_rx) = mpsc::unbounded_channel();

        let temp_dir = tempfile::tempdir().unwrap();
        let scan_paths = vec![temp_dir.path().to_path_buf()];

        // Create a test project directory
        let project_dir = temp_dir.path().join("test_project");
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(project_dir.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();

        // Spawn the scan task
        tokio::spawn(async move {
            scan_task(command_rx, result_tx, scan_paths).await;
        });

        // Send scan command
        command_tx.send(ScanCommand::Scan).unwrap();

        // Wait for results with timeout
        let result = timeout(Duration::from_secs(5), async {
            let mut messages = Vec::new();
            while let Some(msg) = result_rx.recv().await {
                let msg_clone = msg.clone();
                messages.push(msg);
                // Break when we get a success message
                if matches!(msg_clone, ScanResultMessage::Success(_)) {
                    break;
                }
            }
            messages
        }).await;

        assert!(result.is_ok(), "Scan task timed out");
        let messages = result.unwrap();

        // Should have received progress and success messages
        assert!(!messages.is_empty());
        assert!(messages.iter().any(|msg| matches!(msg, ScanResultMessage::Progress(_, _))));
        assert!(messages.iter().any(|msg| matches!(msg, ScanResultMessage::Success(_))));

        // Check that we found the project
        if let Some(ScanResultMessage::Success(results)) = messages.iter().find(|msg| matches!(msg, ScanResultMessage::Success(_))) {
            assert!(!results.is_empty());
            assert!(!results[0].projects.is_empty());
            assert_eq!(results[0].projects[0].project_type, dprojc_types::ProjectType::Rust);
        }
    }

    #[test]
    fn test_full_app_workflow_simulation() {
        // Create a test app with some projects
        let (scan_command_tx, _scan_command_rx) = mpsc::unbounded_channel::<ScanCommand>();
        let (_scan_result_tx, scan_result_rx) = mpsc::unbounded_channel::<ScanResultMessage>();

        let mut app = App {
            state: AppState::Browsing,
            projects: vec![
                create_test_project("/path/to/rust", ProjectType::Rust),
                create_test_project("/path/to/node", ProjectType::NodeJs),
                create_test_project("/path/to/go", ProjectType::Go),
            ],
            filtered_projects: Vec::new(),
            search_query: String::new(),
            selected_index: 0,
            current_view: View::ProjectList,
            should_quit: false,
            scan_result_rx,
            scan_command_tx,
            scan_errors: Vec::new(),
            config: TuiConfig::default(),
            scan_progress: None,
            sort_mode: SortMode::Path,
        };

        app.filtered_projects = app.projects.clone();

        // Test navigation
        assert_eq!(app.selected_index, 0);
        app.handle_project_list_key(crossterm::event::KeyCode::Down);
        assert_eq!(app.selected_index, 1);
        app.handle_project_list_key(crossterm::event::KeyCode::Down);
        assert_eq!(app.selected_index, 2);
        app.handle_project_list_key(crossterm::event::KeyCode::Down); // Should not go beyond
        assert_eq!(app.selected_index, 2);

        // Test search
        app.handle_project_list_key(crossterm::event::KeyCode::Char('/'));
        assert_eq!(app.state, AppState::Searching);
        app.handle_project_list_key(crossterm::event::KeyCode::Char('r'));
        app.handle_project_list_key(crossterm::event::KeyCode::Char('u'));
        app.handle_project_list_key(crossterm::event::KeyCode::Char('s'));
        assert_eq!(app.search_query, "rus");
        assert_eq!(app.filtered_projects.len(), 1);
        assert_eq!(app.filtered_projects[0].project_type, ProjectType::Rust);

        // Exit search
        app.handle_project_list_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app.state, AppState::Browsing);
        assert_eq!(app.filtered_projects.len(), 3); // Back to all projects

        // Test view transitions
        app.handle_project_list_key(crossterm::event::KeyCode::Enter);
        assert_eq!(app.current_view, View::ProjectDetails);
        app.handle_details_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app.current_view, View::ProjectList);

        // Test help view
        app.handle_project_list_key(crossterm::event::KeyCode::Char('?'));
        assert_eq!(app.current_view, View::Help);
        app.handle_help_key(crossterm::event::KeyCode::Esc);
        assert_eq!(app.current_view, View::ProjectList);

        // Test sort cycling
        assert_eq!(app.sort_mode, SortMode::Path);
        app.handle_project_list_key(crossterm::event::KeyCode::Char('s'));
        assert_eq!(app.sort_mode, SortMode::Type);
        app.handle_project_list_key(crossterm::event::KeyCode::Char('s'));
        assert_eq!(app.sort_mode, SortMode::Date);
        app.handle_project_list_key(crossterm::event::KeyCode::Char('s'));
        assert_eq!(app.sort_mode, SortMode::Path);

        // Test quit
        assert!(!app.should_quit);
        app.handle_project_list_key(crossterm::event::KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn test_config_load_with_overrides() {
        use tempfile::tempdir;

        // Ensure clean environment
        std::env::remove_var("DURABLE_SCAN_PATHS");
        std::env::remove_var("DURABLE_MAX_DISPLAY_PROJECTS");
        std::env::remove_var("DURABLE_SHOW_DETAILS");

        let temp_dir = tempdir().unwrap();
        let env_path = temp_dir.path().join("env_path");
        std::fs::create_dir_all(&env_path).unwrap();

        std::env::set_var("DURABLE_SCAN_PATHS", env_path.to_string_lossy().to_string());
        std::env::set_var("DURABLE_MAX_DISPLAY_PROJECTS", "30");

        let config = TuiConfig::load_with_overrides(Some(40), Some(true)).unwrap();

        // Overrides should take precedence
        assert_eq!(config.max_display_projects, Some(40));
        assert!(config.show_details);

        // Env vars should still be loaded
        assert!(config.scan_paths.contains(&env_path));

        // Clean up
        std::env::remove_var("DURABLE_SCAN_PATHS");
        std::env::remove_var("DURABLE_MAX_DISPLAY_PROJECTS");
        std::env::remove_var("DURABLE_SHOW_DETAILS");
    }
}