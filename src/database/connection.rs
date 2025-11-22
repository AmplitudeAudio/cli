use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::migrations::MigrationManager;

/// Wrapper around the SQLite connection
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    path: String,
}

impl Database {
    /// Create a new database connection
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid database path"))?
            .to_string();

        let conn = Connection::open_with_flags(
            &path_str,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .context("Failed to open database connection")?;

        // Set pragmas for better performance and reliability
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -64000;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            ",
        )
        .context("Failed to set database pragmas")?;

        Ok(Self {
            connection: Arc::new(Mutex::new(conn)),
            path: path_str,
        })
    }

    /// Run all pending migrations
    pub async fn run_migrations(&mut self) -> Result<()> {
        let migration_manager = MigrationManager::new();
        migration_manager.run_migrations(self)?;
        Ok(())
    }

    /// Get a connection for executing queries
    pub fn get_connection(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.connection)
    }

    /// Execute a query that doesn't return results
    pub fn execute<P>(&self, sql: &str, params: P) -> Result<usize>
    where
        P: rusqlite::Params,
    {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        conn.execute(sql, params).context("Failed to execute query")
    }

    /// Execute a batch of SQL statements
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        conn.execute_batch(sql)
            .context("Failed to execute batch query")
    }

    /// Prepare a statement for execution
    pub fn prepare(&self, sql: &str) -> Result<DatabaseStatement> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        let stmt = conn.prepare(sql).context("Failed to prepare statement")?;

        Ok(DatabaseStatement {
            connection: Arc::clone(&self.connection),
            sql: sql.to_string(),
        })
    }

    /// Begin a transaction
    pub fn transaction(&self) -> Result<DatabaseTransaction> {
        DatabaseTransaction::new(Arc::clone(&self.connection))
    }

    /// Get the database path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Close the database connection
    pub fn close(self) {
        // Connection will be closed when dropped
        drop(self.connection);
    }
}

/// Wrapper for a prepared statement
#[allow(dead_code)]
pub struct DatabaseStatement {
    connection: Arc<Mutex<Connection>>,
    sql: String,
}

impl DatabaseStatement {
    /// Execute the prepared statement
    #[allow(dead_code)]
    pub fn execute<P>(&self, params: P) -> Result<usize>
    where
        P: rusqlite::Params,
    {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        conn.execute(&self.sql, params)
            .context("Failed to execute prepared statement")
    }

    /// Query the prepared statement
    #[allow(dead_code)]
    pub fn query_map<T, P, F>(&self, params: P, f: F) -> Result<Vec<T>>
    where
        P: rusqlite::Params,
        F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        let mut stmt = conn.prepare(&self.sql)?;
        let rows = stmt.query_map(params, f)?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }
}

/// Wrapper for a database transaction
pub struct DatabaseTransaction {
    connection: Arc<Mutex<Connection>>,
    committed: bool,
}

impl DatabaseTransaction {
    /// Create a new transaction
    fn new(connection: Arc<Mutex<Connection>>) -> Result<Self> {
        {
            let conn = connection
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

            conn.execute("BEGIN TRANSACTION", [])
                .context("Failed to begin transaction")?;
        }

        Ok(Self {
            connection,
            committed: false,
        })
    }

    /// Execute a query within the transaction
    pub fn execute<P>(&self, sql: &str, params: P) -> Result<usize>
    where
        P: rusqlite::Params,
    {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        conn.execute(sql, params)
            .context("Failed to execute query in transaction")
    }

    /// Execute a batch of SQL statements within the transaction
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        conn.execute_batch(sql)
            .context("Failed to execute batch query in transaction")
    }

    /// Commit the transaction
    pub fn commit(mut self) -> Result<()> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        conn.execute("COMMIT", [])
            .context("Failed to commit transaction")?;

        self.committed = true;
        Ok(())
    }

    /// Rollback the transaction
    #[allow(dead_code)]
    pub fn rollback(mut self) -> Result<()> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

        conn.execute("ROLLBACK", [])
            .context("Failed to rollback transaction")?;

        self.committed = true;
        Ok(())
    }
}

impl Drop for DatabaseTransaction {
    fn drop(&mut self) {
        if !self.committed {
            if let Ok(conn) = self.connection.lock() {
                let _ = conn.execute("ROLLBACK", []);
            }
        }
    }
}

/// Type alias for a shared database connection
#[allow(dead_code)]
pub type DatabaseConnection = Arc<Mutex<Connection>>;
