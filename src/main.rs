mod app;
mod commands;
mod common;
mod database;

use crate::{
    app::{App, Commands},
    commands::{project::handler as handle_project_command, sudo::handler as handle_sudo_command},
    common::logger::{init_logger, setup_crash_logging, write_crash_log_on_error},
    database::{Database, setup_crash_db_cleanup},
};
use clap::Parser;
use log::{debug, error, warn};
use std::sync::Arc;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI arguments first to get verbose flag
    let cli = App::parse();

    // Initialize logging system
    if let Err(e) = init_logger(cli.verbose) {
        eprintln!("Failed to initialize logger: {}", e);
        std::process::exit(1);
    }

    // Setup crash logging
    setup_crash_logging();

    // Initialize the database
    let database = match database::initialize().await {
        Ok(db) => {
            debug!("Successfully initialized database");
            Some(Arc::<Database>::new(db))
        }
        Err(e) => {
            error!("Failed to initialize database: {}", e);
            error!("  The application will continue but some features may not work properly.");
            None
        }
    };

    setup_crash_db_cleanup(database.clone());
    let db_for_handler = database.clone();

    // Set up signal handlers for graceful shutdown
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        debug!("\nReceived shutdown signal, cleaning up...");

        if let Some(db) = db_for_handler {
            if let Ok(db) = Arc::try_unwrap(db) {
                database::cleanup(Some(db));
            } else {
                warn!("Database connections still active, forcing shutdown");
            }
        }

        std::process::exit(0);
    });

    let result = run_command(&cli, database.clone()).await;

    // Clean up database on normal exit
    if let Some(db) = database {
        if let Ok(db) = Arc::try_unwrap(db) {
            database::cleanup(Some(db));
        }
    }

    // Handle errors by writing crash log
    if let Err(ref e) = result {
        error!("{}", e);
        if let Some(log_path) = write_crash_log_on_error() {
            eprintln!("Error log written to: {}", log_path.display());
        }
    }

    Ok(())
}

async fn run_command(cli: &App, database: Option<Arc<Database>>) -> anyhow::Result<()> {
    match &cli.command {
        Commands::Project { command } => handle_project_command(command, database).await,
        Commands::Sudo { command } => handle_sudo_command(command, database).await,
    }
}
