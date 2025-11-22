mod app;
mod commands;
mod common;
mod database;
mod widgets;

use crate::{
    app::{App, Commands},
    commands::{project::handler as handle_project_command, sudo::handler as handle_sudo_command},
    database::Database,
};
use clap::Parser;
use log::{Level, log};
use std::sync::Arc;
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize the database
    let database = match database::initialize().await {
        Ok(db) => {
            log!(Level::Debug, "Successfully initialized database");
            Some(Arc::<Database>::new(db))
        }
        Err(e) => {
            log!(Level::Error, "Failed to initialize database: {}", e);
            log!(
                Level::Error,
                "  The application will continue but some features may not work properly."
            );
            None
        }
    };

    let db_for_handler = database.clone();

    // Set up signal handlers for graceful shutdown
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        log!(Level::Debug, "\nReceived shutdown signal, cleaning up...");

        if let Some(db) = db_for_handler {
            if let Ok(db) = Arc::try_unwrap(db) {
                database::cleanup(Some(db));
            } else {
                log!(
                    Level::Warn,
                    "Database connections still active, forcing shutdown"
                );
            }
        }

        std::process::exit(0);
    });

    // Set up the panic hook to ensure database cleanup
    let db_for_panic = database.clone();
    std::panic::set_hook(Box::new(move |panic_info| {
        eprintln!("Application panicked: {}", panic_info);

        if let Some(db) = &db_for_panic {
            // Try to get exclusive access, but don't wait if we can't
            if let Ok(db) = Arc::try_unwrap(db.clone()) {
                database::cleanup(Some(db));
            }
        }
    }));

    let cli = App::parse();
    let result = run_command(cli, database.clone()).await;

    // Clean up database on normal exit
    if let Some(db) = database {
        if let Ok(db) = Arc::try_unwrap(db) {
            database::cleanup(Some(db));
        }
    }

    result
}

async fn run_command(cli: App, database: Option<Arc<Database>>) -> anyhow::Result<()> {
    match &cli.command {
        Commands::Project { command } => handle_project_command(command, database).await,
        Commands::Sudo { command } => handle_sudo_command(command, database).await,
    }
}
