use anyhow::Result;
use clap::Subcommand;
use inquire::Confirm;
use log::warn;
use std::sync::Arc;

use crate::{
    database::{Database, get_database_path},
    presentation::Output,
};
use serde_json::json;

#[derive(Subcommand, Debug)]
pub enum SudoCommands {
    /// Database management operations
    Database {
        #[command(subcommand)]
        command: DatabaseCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum DatabaseCommands {
    /// Reset the database (destructive operation)
    Reset {
        /// Skip confirmation prompt
        #[arg(short = 'y', long = "yes")]
        skip_confirmation: bool,
    },
}

pub async fn handler(
    command: &SudoCommands,
    database: Option<Arc<Database>>,
    output: &dyn Output,
) -> Result<()> {
    match command {
        SudoCommands::Database { command } => {
            handle_database_command(command, database, output).await
        }
    }
}

async fn handle_database_command(
    command: &DatabaseCommands,
    database: Option<Arc<Database>>,
    output: &dyn Output,
) -> Result<()> {
    match command {
        DatabaseCommands::Reset { skip_confirmation } => {
            reset_database(*skip_confirmation, database, output).await
        }
    }
}

async fn reset_database(
    skip_confirmation: bool,
    database: Option<Arc<Database>>,
    output: &dyn Output,
) -> Result<()> {
    output.progress("This operation will:");
    output.progress("  • Delete ALL projects from the database");
    output.progress("  • Clear ALL configuration settings");
    output.progress("  • Reset the database to its initial state");
    output.progress("");
    output.progress("This action cannot be undone!");
    output.progress("");

    // Check if we should ask for confirmation
    if !skip_confirmation {
        let confirmed =
            Confirm::new("Are you absolutely sure you want to reset the database?").prompt()?;

        if !confirmed {
            output.success(json!("Database reset cancelled."), None);
            return Ok(());
        }
    }

    output.progress("Resetting database...");

    // Get the database path
    let db_path = get_database_path()?;

    // Close the current database connection if it exists
    if let Some(db) = database {
        // The database will be closed when the Arc is dropped
        drop(db);
    }

    // Delete the database file
    if db_path.exists() {
        std::fs::remove_file(&db_path)
            .map_err(|e| anyhow::anyhow!("Failed to delete database file: {}", e))?;
        output.progress("Database file deleted");
    } else {
        warn!("Database file does not exist, skipping deletion");
    }

    // Also clean up any WAL or journal files that might exist
    let wal_path = db_path.with_extension("db-wal");
    let shm_path = db_path.with_extension("db-shm");
    let journal_path = db_path.with_extension("db-journal");

    if wal_path.exists() {
        std::fs::remove_file(&wal_path).ok();
    }

    if shm_path.exists() {
        std::fs::remove_file(&shm_path).ok();
    }

    if journal_path.exists() {
        std::fs::remove_file(&journal_path).ok();
    }

    // Recreate and initialize a fresh database
    output.progress("Creating fresh database...");
    let new_db = crate::database::initialize().await?;

    output.success(json!("Database has been reset successfully"), None);

    // Clean up the new database connection
    drop(new_db);

    Ok(())
}
