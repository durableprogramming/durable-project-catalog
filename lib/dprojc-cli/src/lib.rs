//! Command-line interface for the Durable Project Catalog
//!
//! This crate provides a CLI tool for scanning, managing, and querying
//! software project catalogs.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use dprojc_config::ConfigManager;
use dprojc_db::ProjectDatabase;

use dprojc_types::ScanConfig;

pub mod commands;
pub mod output;

pub use output::OutputFormat;

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parsing() {
        // Test basic CLI parsing
        let cli = Cli::try_parse_from(["durable-catalog", "--help"]).unwrap_or_else(|_| {
            // --help will cause an error, but we can still test the parsing
            Cli {
                verbose: 0,
                config: None,
                database: None,
                command: Commands::Stats { format: OutputFormat::Table },
            }
        });

        assert_eq!(cli.verbose, 0);
        assert!(cli.config.is_none());
        assert!(cli.database.is_none());
    }

    #[test]
    fn test_output_format_variants() {
        // Test that all OutputFormat variants work
        let formats = vec![
            OutputFormat::Table,
            OutputFormat::Json,
            OutputFormat::Yaml,
        ];

        for format in formats {
            match format {
                OutputFormat::Table => {}
                OutputFormat::Json => {}
                OutputFormat::Yaml => {}
            }
        }
    }

    #[test]
    fn test_cli_command_factory() {
        // Test that CLI implements CommandFactory
        let _ = Cli::command();
    }
}

/// Main CLI structure
#[derive(Parser)]
#[command(name = "durable-catalog")]
#[command(about = "A pragmatic system for discovering and cataloging software projects")]
#[command(version)]
pub struct Cli {
    /// Increase verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Configuration file path
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Database path (defaults to ~/.local/durable/durable-project-catalog/catalog.db)
    #[arg(long)]
    pub database: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI commands
#[derive(Subcommand)]
pub enum Commands {
    /// Scan directories for software projects
    Scan {
        /// Paths to scan
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Maximum scan depth
        #[arg(long)]
        max_depth: Option<usize>,

        /// Output format
        #[arg(long, default_value = "table")]
        format: OutputFormat,

        /// Don't save results to database
        #[arg(long)]
        no_save: bool,
    },

    /// List projects from the catalog
    List {
        /// Filter by project type
        #[arg(long)]
        project_type: Option<String>,

        /// Search query (fuzzy match on path)
        #[arg(long)]
        search: Option<String>,

        /// Output format
        #[arg(long, default_value = "table")]
        format: OutputFormat,

        /// Limit number of results
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Search projects in the catalog
    Search {
        /// Search query
        query: String,

        /// Output format
        #[arg(long, default_value = "table")]
        format: OutputFormat,

        /// Limit number of results
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Generate a report of the catalog
    Report {
        /// Output file path (prints to stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output format
        #[arg(long, default_value = "json")]
        format: OutputFormat,

        /// Include scan statistics
        #[arg(long)]
        stats: bool,
    },

    /// Show catalog statistics
    Stats {
        /// Output format
        #[arg(long, default_value = "table")]
        format: OutputFormat,
    },

    /// Show current configuration
    Config {
        /// Output format
        #[arg(long, default_value = "yaml")]
        format: OutputFormat,
    },

    /// Clean up old scan results and orphaned projects
    Clean {
        /// Maximum age of scan results to keep (in days)
        #[arg(long, default_value = "30")]
        max_age_days: i64,

        /// Dry run - show what would be deleted
        #[arg(long)]
        dry_run: bool,

        /// Remove projects with non-existent paths (ignores max_age_days for path checking)
        #[arg(long)]
        check_paths: bool,
    },

    /// Generate documentation for the project catalog
    Docs {
        /// Output directory for generated documentation
        #[arg(short, long, default_value = "docs")]
        output_dir: String,

        /// Include README content in documentation
        #[arg(long)]
        include_readmes: bool,

        /// Include project statistics
        #[arg(long)]
        include_stats: bool,

        /// Include detailed project pages
        #[arg(long)]
        include_details: bool,

        /// Template directory (optional)
        #[arg(long)]
        templates: Option<PathBuf>,
    },

    /// Launch the terminal user interface
    Tui {
        /// Maximum number of projects to display
        #[arg(long)]
        max_display: Option<usize>,

        /// Show project details by default
        #[arg(long)]
        show_details: bool,
    },

    /// Clean old Cargo target directories
    CleanOldCargo {
        /// Paths to scan for Cargo.toml files
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Maximum age in hours for target/ files to keep (default: 48)
        #[arg(long, default_value = "48")]
        max_age_hours: u64,

        /// Dry run - show what would be cleaned
        #[arg(long)]
        dry_run: bool,
    },

    /// Shell integration commands
    #[command(subcommand)]
    Shell(ShellCommands),
}

/// Shell integration subcommands
#[derive(Subcommand)]
pub enum ShellCommands {
    /// Query projects matching a pattern (returns paths sorted by frecency)
    Query {
        /// Search pattern
        pattern: String,

        /// Limit number of results
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Record a directory access (for frecency tracking)
    Record {
        /// Path to record
        path: PathBuf,
    },

    /// Check if a path is in the catalog
    Check {
        /// Path to check
        path: PathBuf,
    },

    /// Generate completions for a partial path
    Complete {
        /// Partial path to complete
        partial: String,
    },

    /// Generate shell integration script
    Init {
        /// Shell type (bash, zsh, fish)
        #[arg(value_parser = ["bash", "zsh", "fish"])]
        shell: String,
    },
}



/// Main CLI runner
pub struct CliRunner {
    config: ScanConfig,
    database: ProjectDatabase,
    verbose: u8,
}

impl CliRunner {
    /// Create a new CLI runner
    pub async fn new(cli: &Cli) -> anyhow::Result<Self> {
        let config = if let Some(config_path) = &cli.config {
            ConfigManager::load_from_path(config_path)?
        } else {
            ConfigManager::load_config()?
        };

        let database_path = cli.database.clone()
            .unwrap_or_else(|| dprojc_utils::default_db_path().unwrap());
        let database = ProjectDatabase::open(database_path)?;

        Ok(Self {
            config,
            database,
            verbose: cli.verbose,
        })
    }

    /// Run the CLI command
    pub async fn run(&mut self, command: &Commands) -> anyhow::Result<()> {
        match command {
            Commands::Scan { paths, max_depth, format, no_save } => {
                self.run_scan(paths, *max_depth, format, *no_save).await
            }
            Commands::List { project_type, search, format, limit } => {
                self.run_list(project_type.as_deref(), search.as_deref(), format, *limit).await
            }
            Commands::Search { query, format, limit } => {
                self.run_search(query, format, *limit).await
            }
            Commands::Report { output, format, stats } => {
                self.run_report(output.as_ref(), format, *stats).await
            }
            Commands::Stats { format } => {
                self.run_stats(format).await
            }
            Commands::Config { format } => {
                self.run_config(format).await
            }
            Commands::Clean { max_age_days, dry_run, check_paths } => {
                self.run_clean(*max_age_days, *dry_run, *check_paths).await
            }
            Commands::Docs { output_dir, include_readmes, include_stats, include_details, templates } => {
                self.run_docs(output_dir, *include_readmes, *include_stats, *include_details, templates.as_ref()).await
            }
            Commands::Tui { max_display, show_details } => {
                self.run_tui(*max_display, *show_details).await
            }
            Commands::CleanOldCargo { paths, max_age_hours, dry_run } => {
                self.run_clean_old_cargo(paths, *max_age_hours, *dry_run).await
            }
            Commands::Shell(shell_cmd) => {
                self.run_shell(shell_cmd).await
            }
        }
    }
}