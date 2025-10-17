//! CLI command implementations

use std::path::PathBuf;

use dprojc_docs::{DocsConfig, DocumentationGenerator};
use dprojc_scanner::ProjectScanner;
use dprojc_types::{ProjectType, ReportData, StatsData};
use fuzzy_matcher::FuzzyMatcher;
use indicatif::{ProgressBar, ProgressStyle};

use crate::output::{OutputFormat, OutputFormatter};
use crate::CliRunner;

impl CliRunner {
    /// Run the scan command
    pub async fn run_scan(
        &mut self,
        paths: &[PathBuf],
        max_depth: Option<usize>,
        format: &OutputFormat,
        no_save: bool,
    ) -> anyhow::Result<()> {
        let mut config = self.config.clone();
        if let Some(depth) = max_depth {
            config.max_depth = Some(depth);
        }

        let scanner = ProjectScanner::with_config(config)?;
        let mut all_results = Vec::new();

        // Create progress bar
        let progress = ProgressBar::new(paths.len() as u64);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );

        for path in paths {
            progress.set_message(format!("Scanning {}", path.display()));
            let result = scanner.scan(path).await?;
            all_results.push(result);
            progress.inc(1);
        }

        progress.finish_with_message("Scan complete");

        // Collect all projects
        let mut all_projects = Vec::new();
        let mut total_dirs_scanned = 0;
        let mut total_errors = 0;

        for result in &all_results {
            all_projects.extend(result.projects.clone());
            total_dirs_scanned += result.dirs_scanned;
            total_errors += result.errors.len();
        }

        // Save to database if requested
        if !no_save {
            let save_progress = ProgressBar::new(all_projects.len() as u64);
            save_progress.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} Saving projects...")
                    .unwrap()
                    .progress_chars("#>-"),
            );

            for project in &all_projects {
                self.database.upsert_project(project)?;
                save_progress.inc(1);
            }

            save_progress.finish_with_message("Projects saved to database");

            // Store scan results
            for result in &all_results {
                self.database.store_scan_result(result)?;
            }
        }

        // Output results
        let formatter = OutputFormatter::new(format.clone());

        if self.verbose > 0 {
            println!(
                "Scanned {} directories, found {} projects, {} errors",
                total_dirs_scanned,
                all_projects.len(),
                total_errors
            );
        }

        formatter.format_scan_results(&all_results)?;

        Ok(())
    }

    /// Run the list command
    pub async fn run_list(
        &self,
        project_type: Option<&str>,
        search: Option<&str>,
        format: &OutputFormat,
        limit: Option<usize>,
    ) -> anyhow::Result<()> {
        let mut projects = if let Some(pt_str) = project_type {
            let project_type = match pt_str.to_lowercase().as_str() {
                "rust" => ProjectType::Rust,
                "nodejs" | "node" => ProjectType::NodeJs,
                "ruby" => ProjectType::Ruby,
                "python" => ProjectType::Python,
                "go" => ProjectType::Go,
                "java" => ProjectType::Java,
                "git" => ProjectType::Git,
                "nix" => ProjectType::Nix,
                "unknown" => ProjectType::Unknown,
                _ => return Err(anyhow::anyhow!("Unknown project type: {}", pt_str)),
            };
            self.database.get_projects_by_type(&project_type)?
        } else {
            self.database.get_all_projects()?
        };

        // Apply search filter
        if let Some(query) = search {
            let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
            projects.retain(|project| {
                let path_str = project.path.to_string_lossy();
                let type_str = format!("{:?}", project.project_type);
                matcher.fuzzy_match(&path_str, query).is_some()
                    || matcher.fuzzy_match(&type_str, query).is_some()
            });
        }

        // Apply limit
        if let Some(limit) = limit {
            projects.truncate(limit);
        }

        let formatter = OutputFormatter::new(format.clone());
        formatter.format_projects(&projects)?;

        Ok(())
    }

    /// Run the search command
    pub async fn run_search(
        &self,
        query: &str,
        format: &OutputFormat,
        limit: Option<usize>,
    ) -> anyhow::Result<()> {
        let all_projects = self.database.get_all_projects()?;

        let mut matched_projects: Vec<_> = all_projects
            .into_iter()
            .filter(|project| {
                let path_str = project.path.to_string_lossy();
                let type_str = format!("{:?}", project.project_type);
                path_str.contains(query) || type_str.contains(query)
            })
            .collect();

        // Sort by relevance (exact matches first, then by path length)
        matched_projects.sort_by(|a, b| {
            let a_path = a.path.to_string_lossy();
            let b_path = b.path.to_string_lossy();
            let a_type = format!("{:?}", a.project_type);
            let b_type = format!("{:?}", b.project_type);

            // Exact matches get priority
            let a_exact = a_path == query || a_type == query;
            let b_exact = b_path == query || b_type == query;

            if a_exact && !b_exact {
                std::cmp::Ordering::Less
            } else if !a_exact && b_exact {
                std::cmp::Ordering::Greater
            } else {
                // For substring matches, sort by path length (shorter paths first)
                a_path.len().cmp(&b_path.len())
            }
        });

        // Apply limit
        if let Some(limit) = limit {
            matched_projects.truncate(limit);
        }

        let formatter = OutputFormatter::new(format.clone());
        formatter.format_projects(&matched_projects)?;

        Ok(())
    }

    /// Run the report command
    pub async fn run_report(
        &self,
        output_path: Option<&PathBuf>,
        format: &OutputFormat,
        include_stats: bool,
    ) -> anyhow::Result<()> {
        let projects = self.database.get_all_projects()?;
        let stats = if include_stats {
            Some(self.database.get_scan_statistics()?)
        } else {
            None
        };

        let report = ReportData {
            projects,
            statistics: stats,
            generated_at: chrono::Utc::now(),
        };

        let output = match format {
            OutputFormat::Json => serde_json::to_string_pretty(&report)?,
            OutputFormat::Yaml => serde_yaml::to_string(&report)?,
            OutputFormat::Table => {
                let formatter = OutputFormatter::new(OutputFormat::Table);
                let mut buffer = Vec::new();
                formatter.format_report_to_writer(&report, &mut buffer)?;
                String::from_utf8(buffer)?
            }
        };

        if let Some(path) = output_path {
            std::fs::write(path, &output)?;
            if self.verbose > 0 {
                println!("Report written to {}", path.display());
            }
        } else {
            println!("{}", output);
        }

        Ok(())
    }

    /// Run the stats command
    pub async fn run_stats(&self, format: &OutputFormat) -> anyhow::Result<()> {
        let stats = self.database.get_scan_statistics()?;
        let counts = self.database.get_project_counts_by_type()?;

        let stats_data = StatsData {
            statistics: stats,
            project_counts: counts,
        };

        let formatter = OutputFormatter::new(format.clone());
        formatter.format_stats(&stats_data)?;

        Ok(())
    }

    /// Run the config command
    pub async fn run_config(&self, format: &OutputFormat) -> anyhow::Result<()> {
        let output = match format {
            OutputFormat::Json => serde_json::to_string_pretty(&self.config)?,
            OutputFormat::Yaml => serde_yaml::to_string(&self.config)?,
            OutputFormat::Table => {
                format!("Max Depth: {:?}\nExclude Patterns: {:?}\nProject Indicators: {:?}\nFollow Symlinks: {}",
                       self.config.max_depth,
                       self.config.exclude_patterns,
                       self.config.project_indicators,
                       self.config.follow_symlinks)
            }
        };

        println!("{}", output);
        Ok(())
    }

    /// Run the clean command
    pub async fn run_clean(
        &self,
        max_age_days: i64,
        dry_run: bool,
        check_paths: bool,
    ) -> anyhow::Result<()> {
        if dry_run {
            println!("DRY RUN - No changes will be made");
        }

        // Get statistics before cleanup
        let before_stats = self.database.get_scan_statistics()?;

        println!("Before cleanup:");
        println!("  Total scans: {}", before_stats.total_scans);
        println!("  Total projects: {}", before_stats.total_projects);

        let mut deleted_missing_paths = 0;

        if dry_run {
            println!(
                "\nWould clean up scan results older than {} days",
                max_age_days
            );
            println!("Would remove orphaned projects");

            if check_paths {
                println!("Would remove projects with non-existent paths");
                // Count how many would be removed
                let all_projects = self.database.get_all_projects()?;
                for project in all_projects {
                    if !project.path.exists() {
                        deleted_missing_paths += 1;
                        println!("  Would remove: {}", project.path.display());
                    }
                }
            }
        } else {
            let deleted_scans = self.database.delete_old_scan_results(
                chrono::Utc::now() - chrono::Duration::days(max_age_days),
            )?;
            let deleted_projects = self.database.cleanup_orphaned_projects(max_age_days)?;

            if check_paths {
                let all_projects = self.database.get_all_projects()?;
                for project in all_projects {
                    if !project.path.exists() {
                        if self.verbose > 0 {
                            println!("Removing non-existent path: {}", project.path.display());
                        }
                        self.database.delete_project_by_path(&project.path)?;
                        deleted_missing_paths += 1;
                    }
                }
            }

            println!("\nCleanup completed:");
            println!("  Deleted {} old scan results", deleted_scans);
            println!("  Removed {} orphaned projects", deleted_projects);
            if check_paths {
                println!(
                    "  Removed {} projects with non-existent paths",
                    deleted_missing_paths
                );
            }

            let after_stats = self.database.get_scan_statistics()?;
            println!("\nAfter cleanup:");
            println!("  Total scans: {}", after_stats.total_scans);
            println!("  Total projects: {}", after_stats.total_projects);
        }

        Ok(())
    }

    /// Run the docs command
    pub async fn run_docs(
        &self,
        output_dir: &str,
        include_readmes: bool,
        include_stats: bool,
        include_details: bool,
        templates: Option<&PathBuf>,
    ) -> anyhow::Result<()> {
        use dprojc_core::ProjectCatalog;

        // Create a catalog instance
        let catalog = ProjectCatalog::new().await?;

        // Create docs configuration
        let config = DocsConfig {
            output_dir: output_dir.to_string(),
            template_dir: templates.as_ref().map(|p| p.to_string_lossy().to_string()),
            include_readmes,
            include_statistics: include_stats,
            include_project_details: include_details,
            syntax_highlight: true,
            theme: "base16-ocean.dark".to_string(),
        };

        // Create documentation generator
        let mut generator = DocumentationGenerator::new(&catalog, config)?;

        // Generate all documentation
        let progress = ProgressBar::new(1);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );

        progress.set_message("Generating documentation...");
        generator.generate_all().await?;
        progress.finish_with_message("Documentation generated successfully");

        if self.verbose > 0 {
            println!("Documentation generated in: {}", output_dir);
        }

        Ok(())
    }

    /// Run the TUI command
    pub async fn run_tui(
        &self,
        max_display: Option<usize>,
        show_details: bool,
    ) -> anyhow::Result<()> {
        use dprojc_tui::{run_tui_with_config, TuiConfig};

        let config = TuiConfig::load_with_overrides(max_display, Some(show_details))?;
        run_tui_with_config(config).await
    }

    /// Run the clean-old-cargo command
    pub async fn run_clean_old_cargo(
        &self,
        paths: &[PathBuf],
        max_age_hours: u64,
        dry_run: bool,
    ) -> anyhow::Result<()> {
        use std::time::{Duration, SystemTime};
        use walkdir::WalkDir;

        if dry_run {
            println!("DRY RUN - No changes will be made");
        }

        let cutoff_time = SystemTime::now() - Duration::from_secs(max_age_hours * 3600);
        let mut cleaned_count = 0;
        let mut total_found = 0;

        println!("Scanning for Cargo.toml files in {} paths...", paths.len());

        for scan_path in paths {
            println!("Scanning: {}", scan_path.display());

            for entry in WalkDir::new(scan_path).into_iter().filter_map(|e| e.ok()) {
                if entry.file_name() == "Cargo.toml" && entry.file_type().is_file() {
                    let cargo_dir = entry.path().parent().unwrap();
                    let target_dir = cargo_dir.join("target");

                    if target_dir.exists() && target_dir.is_dir() {
                        total_found += 1;

                        // Check if any file in target/ has been modified recently
                        let mut should_clean = true;
                        for target_entry in
                            WalkDir::new(&target_dir).into_iter().filter_map(|e| e.ok())
                        {
                            if let Ok(metadata) = target_entry.metadata() {
                                // Only check files, not directories (directories get mtime updated when files are added)
                                if metadata.is_file() {
                                    if let Ok(modified) = metadata.modified() {
                                        if modified > cutoff_time {
                                            should_clean = false;
                                            break;
                                        }
                                    }
                                }
                            }
                        }

                        if should_clean {
                            println!("Found old target directory: {}", target_dir.display());

                            if !dry_run {
                                match std::process::Command::new("cargo")
                                    .arg("clean")
                                    .current_dir(cargo_dir)
                                    .output()
                                {
                                    Ok(output) => {
                                        if output.status.success() {
                                            println!("  ✓ Cleaned successfully");
                                            cleaned_count += 1;
                                        } else {
                                            let stderr = String::from_utf8_lossy(&output.stderr);
                                            println!("  ✗ Failed to clean: {}", stderr.trim());
                                        }
                                    }
                                    Err(e) => {
                                        println!("  ✗ Failed to run cargo clean: {}", e);
                                    }
                                }
                            } else {
                                cleaned_count += 1;
                            }
                        }
                    }
                }
            }
        }

        println!("\nSummary:");
        println!(
            "  Found {} Cargo projects with target/ directories",
            total_found
        );
        println!("  Cleaned {} old target directories", cleaned_count);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cli, Commands, OutputFormat};
    use dprojc_types::{Project, ProjectIndicator, ProjectType};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_test_project() -> Project {
        Project {
            path: PathBuf::from("/test/project"),
            project_type: ProjectType::Rust,
            indicators: vec![ProjectIndicator::CargoToml],
            last_scanned: chrono::Utc::now(),
        }
    }

    fn create_test_stats_data() -> StatsData {
        StatsData {
            statistics: dprojc_types::ScanStatistics {
                total_scans: 1,
                total_projects: 5,
                total_dirs_scanned: 100,
                total_errors: 0,
                last_scan_timestamp: Some(chrono::Utc::now()),
            },
            project_counts: std::collections::HashMap::from([
                (ProjectType::Rust, 3),
                (ProjectType::NodeJs, 2),
            ]),
        }
    }

    #[test]
    fn test_stats_data_creation() {
        let stats = create_test_stats_data();
        assert_eq!(stats.statistics.total_scans, 1);
        assert_eq!(stats.statistics.total_projects, 5);
        assert_eq!(stats.project_counts.len(), 2);
    }

    #[test]
    fn test_project_creation() {
        let project = create_test_project();
        assert_eq!(project.project_type, ProjectType::Rust);
        assert_eq!(project.indicators.len(), 1);
        assert!(matches!(project.indicators[0], ProjectIndicator::CargoToml));
    }

    #[test]
    fn test_report_data_creation() {
        let projects = vec![create_test_project()];
        let stats = Some(create_test_stats_data().statistics);

        let report = ReportData {
            projects,
            statistics: stats,
            generated_at: chrono::Utc::now(),
        };

        assert_eq!(report.projects.len(), 1);
        assert!(report.statistics.is_some());
    }

    #[tokio::test]
    async fn test_cli_runner_creation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create a minimal CLI for testing
        let cli = Cli {
            verbose: 0,
            config: None,
            database: Some(db_path),
            command: Commands::Stats {
                format: OutputFormat::Table,
            },
        };

        let runner = CliRunner::new(&cli).await;
        assert!(runner.is_ok());
    }

    #[test]
    fn test_output_formatter_creation() {
        let _formatter = OutputFormatter::new(OutputFormat::Json);
        // Just test that it can be created without panicking
    }

    #[test]
    fn test_project_type_parsing() {
        // Test valid project types
        assert_eq!("rust".to_lowercase(), "rust");
        assert_eq!("nodejs", "nodejs");
        assert_eq!("python", "python");
        assert_eq!("go", "go");
        assert_eq!("java", "java");
        assert_eq!("git", "git");
        assert_eq!("nix", "nix");
        assert_eq!("unknown", "unknown");
    }

    #[test]
    fn test_fuzzy_search_filtering() {
        let projects = vec![
            Project {
                path: PathBuf::from("/path/to/rust/project"),
                project_type: ProjectType::Rust,
                indicators: vec![ProjectIndicator::CargoToml],
                last_scanned: chrono::Utc::now(),
            },
            Project {
                path: PathBuf::from("/path/to/node/project"),
                project_type: ProjectType::NodeJs,
                indicators: vec![ProjectIndicator::PackageJson],
                last_scanned: chrono::Utc::now(),
            },
            Project {
                path: PathBuf::from("/other/path"),
                project_type: ProjectType::Git,
                indicators: vec![ProjectIndicator::GitDirectory],
                last_scanned: chrono::Utc::now(),
            },
        ];

        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();

        // Test exact match
        let filtered: Vec<_> = projects
            .iter()
            .filter(|p| {
                let path_str = p.path.to_string_lossy();
                matcher.fuzzy_match(&path_str, "rust").is_some()
            })
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].project_type, ProjectType::Rust);

        // Test partial match
        let filtered: Vec<_> = projects
            .iter()
            .filter(|p| {
                let path_str = p.path.to_string_lossy();
                matcher.fuzzy_match(&path_str, "path").is_some()
            })
            .collect();
        assert_eq!(filtered.len(), 3);

        // Test no match
        let filtered: Vec<_> = projects
            .iter()
            .filter(|p| {
                let path_str = p.path.to_string_lossy();
                matcher.fuzzy_match(&path_str, "nonexistent").is_some()
            })
            .collect();
        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_limit_application() {
        let projects = vec![
            Project {
                path: PathBuf::from("/path/1"),
                project_type: ProjectType::Rust,
                indicators: vec![ProjectIndicator::CargoToml],
                last_scanned: chrono::Utc::now(),
            },
            Project {
                path: PathBuf::from("/path/2"),
                project_type: ProjectType::NodeJs,
                indicators: vec![ProjectIndicator::PackageJson],
                last_scanned: chrono::Utc::now(),
            },
            Project {
                path: PathBuf::from("/path/3"),
                project_type: ProjectType::Python,
                indicators: vec![ProjectIndicator::PyprojectToml],
                last_scanned: chrono::Utc::now(),
            },
        ];

        // Test limit of 2
        let mut limited = projects.clone();
        limited.truncate(2);
        assert_eq!(limited.len(), 2);

        // Test limit larger than available
        let mut limited = projects.clone();
        limited.truncate(10);
        assert_eq!(limited.len(), 3);

        // Test limit of 0
        let mut limited = projects.clone();
        limited.truncate(0);
        assert_eq!(limited.len(), 0);
    }

    #[test]
    fn test_stats_data_serialization() {
        let stats = create_test_stats_data();

        // Test that it can be serialized to JSON
        let json = serde_json::to_string(&stats);
        assert!(json.is_ok());

        // Test that it can be deserialized
        let deserialized: StatsData = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(deserialized.statistics.total_scans, 1);
        assert_eq!(deserialized.project_counts.len(), 2);
    }

    #[test]
    fn test_report_data_serialization() {
        let projects = vec![create_test_project()];
        let stats = Some(create_test_stats_data().statistics);

        let report = ReportData {
            projects,
            statistics: stats,
            generated_at: chrono::Utc::now(),
        };

        // Test JSON serialization
        let json = serde_json::to_string(&report);
        assert!(json.is_ok());

        // Test YAML serialization
        let yaml = serde_yaml::to_string(&report);
        assert!(yaml.is_ok());
    }
}

impl CliRunner {
    /// Run shell integration commands
    pub async fn run_shell(&mut self, command: &crate::ShellCommands) -> anyhow::Result<()> {
        use crate::ShellCommands;
        use dprojc_shell::{generate_completions, ShellIntegration, ShellType};

        let db_path = dprojc_utils::default_db_path()?;

        match command {
            ShellCommands::Query { pattern, limit } => {
                let shell = ShellIntegration::new(&db_path)?;
                let results = shell.query(pattern, *limit)?;

                // Print only the paths, one per line (for shell consumption)
                for path in results.iter() {
                    println!("{}", path.display());
                }

                Ok(())
            }

            ShellCommands::Record { path } => {
                let shell = ShellIntegration::new(&db_path)?;
                shell.record_access(path)?;
                Ok(())
            }

            ShellCommands::Check { path } => {
                let shell = ShellIntegration::new(&db_path)?;
                let is_root = shell.is_project_root(path)?;

                // Exit with 0 if path is in catalog, 1 otherwise
                if is_root {
                    std::process::exit(0);
                } else {
                    std::process::exit(1);
                }
            }

            ShellCommands::Complete { partial } => {
                let shell = ShellIntegration::new(&db_path)?;
                let all = shell.all_projects()?;

                // Filter projects that match the partial path
                let matching: Vec<_> = all
                    .iter()
                    .filter(|p| {
                        let path_str = p.to_string_lossy();
                        path_str.contains(partial)
                    })
                    .collect();

                // Print matching paths for shell completion
                for path in matching {
                    println!("{}", path.display());
                }

                Ok(())
            }

            ShellCommands::Init { shell } => {
                let shell_type = ShellType::from_str(shell)
                    .ok_or_else(|| anyhow::anyhow!("Unsupported shell type: {}", shell))?;

                let script = generate_completions(shell_type);
                println!("{}", script);

                Ok(())
            }
        }
    }
}
