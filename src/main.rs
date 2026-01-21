mod app;
mod commands;
mod common;
mod database;
mod input;
mod presentation;

use crate::{
    app::{App, Commands},
    commands::{project::handler as handle_project_command, sudo::handler as handle_sudo_command},
    common::errors::{CliError, determine_exit_code, exit_codes},
    common::logger::{init_logger, setup_crash_logging, write_crash_log_on_error},
    database::{Database, setup_crash_db_cleanup},
    input::{Input, InputMode, create_input},
    presentation::{Output, OutputMode, create_output},
};
use clap::Parser;
use log::{debug, error, warn};
use std::{panic, sync::Arc};
use tokio::signal;

fn main() {
    // We manually create a runtime to be able to use `catch_unwind` on the async logic.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = panic::catch_unwind(|| rt.block_on(async_main()));

    // This block handles the result of the program execution, including panics.
    // It is responsible for setting the final exit code.
    let exit_code = match result {
        // The program executed without panicking
        Ok(Ok(())) => exit_codes::SUCCESS,
        Ok(Err(e)) => {
            // The program returned a normal error, determine exit code from it.
            // We need to re-parse CLI args to get the output mode.
            let cli = App::parse();
            let output_mode = if cli.json {
                OutputMode::Json
            } else {
                OutputMode::Interactive
            };
            let output = create_output(output_mode);
            let error_code = e.downcast_ref::<CliError>().map(|ce| ce.code).unwrap_or(-1);
            let exit_code = determine_exit_code(&e);

            output.error(&e, error_code, None);

            if output_mode == OutputMode::Interactive {
                if let Some(log_path) = write_crash_log_on_error() {
                    eprintln!("Error log written to: {}", log_path.display());
                }
            }
            exit_code
        }
        // The program panicked
        Err(panic_payload) => {
            let cli = App::parse();
            let output_mode = if cli.json {
                OutputMode::Json
            } else {
                OutputMode::Interactive
            };
            let output = create_output(output_mode);

            // Create a generic error message for the panic
            let err_msg = if let Some(s) = panic_payload.downcast_ref::<&'static str>() {
                format!("Internal panic: {}", s)
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                format!("Internal panic: {}", s)
            } else {
                "An unexpected internal error occurred".to_string()
            };

            let panic_err = anyhow::anyhow!(err_msg);
            output.error(&panic_err, -1, None);

            if let Some(log_path) = write_crash_log_on_error() {
                if output_mode == OutputMode::Interactive {
                    eprintln!("A crash log has been written to: {}", log_path.display());
                }
            }

            exit_codes::SYSTEM_ERROR
        }
    };

    std::process::exit(exit_code);
}

async fn async_main() -> anyhow::Result<()> {
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

    // Return the result to be handled by the synchronous main function
    result
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
