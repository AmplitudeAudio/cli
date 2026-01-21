mod app;
mod commands;
mod common;
mod database;
mod input;
mod presentation;

use crate::{
    app::{App, Commands},
    commands::{project::handler as handle_project_command, sudo::handler as handle_sudo_command},
    common::logger::{init_logger, setup_crash_logging, write_crash_log_on_error},
    database::{Database, setup_crash_db_cleanup},
    input::{Input, InputMode, create_input},
    presentation::{Output, OutputMode, create_output},
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

    // Create output handler based on --json flag
    let output_mode = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Interactive
    };
    let output = create_output(output_mode);

    // Create input handler based on flags.
    // Rule: --json implies non-interactive input.
    let input_mode = if cli.json || cli.non_interactive {
        InputMode::NonInteractive
    } else {
        InputMode::Interactive
    };
    let input = create_input(input_mode);

    let result = run_command(&cli, database.clone(), input.as_ref(), output.as_ref()).await;

    // Clean up database on normal exit
    if let Some(db) = database {
        if let Ok(db) = Arc::try_unwrap(db) {
            database::cleanup(Some(db));
        }
    }

    // Handle result and exit appropriately
    match result {
        Ok(()) => std::process::exit(0),
        Err(ref e) => {
            // Determine exit code: 1 for user errors (default), 2 for system errors
            // Story 1.5 will refine this with proper error code mapping
            let exit_code = 1;

            if cli.json {
                // In JSON mode, output error as JSON to stdout
                // Use a generic error code for now (-1), Story 1.4 will add proper codes
                output.error(e, -1, None);
            } else {
                // In interactive mode, log the error
                error!("{}", e);
                if let Some(log_path) = write_crash_log_on_error() {
                    eprintln!("Error log written to: {}", log_path.display());
                }
            }

            std::process::exit(exit_code);
        }
    }
}

async fn run_command(
    cli: &App,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> anyhow::Result<()> {
    match &cli.command {
        Commands::Project { command } => {
            handle_project_command(command, database, input, output).await
        }
        Commands::Sudo { command } => handle_sudo_command(command, database, input, output).await,
    }
}
