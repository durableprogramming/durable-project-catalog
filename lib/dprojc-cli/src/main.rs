//! Main entry point for the Durable Project Catalog CLI

use clap::Parser;
use dprojc_cli::{Cli, CliRunner};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    std::env::set_var("RUST_LOG", log_level);
    env_logger::init();

    // Create and run the CLI
    match CliRunner::new(&cli).await {
        Ok(mut runner) => {
            if let Err(e) = runner.run(&cli.command).await {
                log::error!("CLI command failed: {}", e);
                return Err(e);
            }
        }
        Err(e) => {
            log::error!("Failed to initialize CLI runner: {}", e);
            return Err(e);
        }
    }

    Ok(())
}