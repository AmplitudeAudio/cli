use anyhow::Result;
use clap::Subcommand;
use inquire::Confirm;
use std::sync::Arc;

use crate::database::{Database, get_database_path};

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

pub async fn handler(command: &SudoCommands, database: Option<Arc<Database>>) -> Result<()> {
    match command {
        SudoCommands::Database { command } => handle_database_command(command, database).await,
    }
}

async fn handle_database_command(
    command: &DatabaseCommands,
    database: Option<Arc<Database>>,
) -> Result<()> {
    match command {
        DatabaseCommands::Reset { skip_confirmation } => {
            reset_database(*skip_confirmation, database).await
        }
    }
}

async fn reset_database(skip_confirmation: bool, database: Option<Arc<Database>>) -> Result<()> {
    println!("⚠️  WARNING: Database Reset");
    println!("============================");
    println!("This operation will:");
    println!("  • Delete ALL projects from the database");
    println!("  • Clear ALL configuration settings");
    println!("  • Remove ALL command history");
    println!("  • Reset the database to its initial state");
    println!();
    println!("This action cannot be undone!");
    println!();

    // Check if we should ask for confirmation
    if !skip_confirmation {
        let confirmed =
            Confirm::new("Are you absolutely sure you want to reset the database?").prompt()?;

        if !confirmed {
            println!("Database reset cancelled.");
            return Ok(());
        }

        println!();
    }

    println!("Resetting database...");

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
        println!("✓ Database file deleted");
    } else {
        println!("ℹ Database file does not exist");
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
    println!("Creating fresh database...");
    let new_db = crate::database::initialize().await?;

    println!("✓ Database has been reset successfully");
    println!();
    println!("The database has been restored to its initial state.");
    println!("All data has been permanently deleted.");

    // Clean up the new database connection
    drop(new_db);

    Ok(())
}
