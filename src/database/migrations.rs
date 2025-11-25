use super::Database;
use anyhow::{Context, Result};
use log::debug;
use std::collections::BTreeMap;

/// Represents a single database migration
pub struct Migration {
    /// Version number of the migration
    pub version: u32,
    /// Description of what this migration does
    pub description: String,
    /// SQL to execute for the migration
    pub up_sql: String,
    /// Optional SQL to rollback the migration
    pub down_sql: Option<String>,
}

/// Manages database migrations
pub struct MigrationManager {
    migrations: BTreeMap<u32, Migration>,
}

impl MigrationManager {
    /// Create a new migration manager with all migrations
    pub fn new() -> Self {
        let mut migrations = BTreeMap::new();

        // Add all migrations here
        migrations.insert(
            1,
            Migration {
                version: 1,
                description: "Initial schema - create migrations table".to_string(),
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS schema_migrations (
                        version INTEGER PRIMARY KEY NOT NULL,
                        description TEXT NOT NULL,
                        applied_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                        checksum TEXT NOT NULL
                    );

                    CREATE INDEX IF NOT EXISTS idx_migrations_applied_at
                    ON schema_migrations(applied_at);
                "#
                .to_string(),
                down_sql: Some("DROP TABLE IF EXISTS schema_migrations;".to_string()),
            },
        );

        migrations.insert(
            2,
            Migration {
                version: 2,
                description: "Create projects table".to_string(),
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS projects (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        name TEXT NOT NULL UNIQUE,
                        path TEXT NOT NULL,
                        template TEXT,
                        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                        metadata TEXT -- JSON data for additional project info
                    );

                    CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name);
                    CREATE INDEX IF NOT EXISTS idx_projects_created_at ON projects(created_at);

                    -- Trigger to update updated_at on row update
                    CREATE TRIGGER IF NOT EXISTS update_projects_updated_at
                    AFTER UPDATE ON projects
                    BEGIN
                        UPDATE projects SET updated_at = CURRENT_TIMESTAMP
                        WHERE id = NEW.id;
                    END;
                "#
                .to_string(),
                down_sql: Some("DROP TABLE IF EXISTS projects;".to_string()),
            },
        );

        migrations.insert(
            3,
            Migration {
                version: 3,
                description: "Create templates table".to_string(),
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS templates (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        name TEXT NOT NULL UNIQUE,
                        path TEXT NOT NULL,
                        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                        metadata TEXT -- JSON data for additional project info
                    );

                    CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name);
                    CREATE INDEX IF NOT EXISTS idx_projects_created_at ON projects(created_at);

                    -- Trigger to update updated_at on row update
                    CREATE TRIGGER IF NOT EXISTS update_projects_updated_at
                    AFTER UPDATE ON projects
                    BEGIN
                        UPDATE projects SET updated_at = CURRENT_TIMESTAMP
                        WHERE id = NEW.id;
                    END;
                "#
                .to_string(),
                down_sql: Some("DROP TABLE IF EXISTS projects;".to_string()),
            },
        );

        migrations.insert(
            4,
            Migration {
                version: 4,
                description: "Create configuration table".to_string(),
                up_sql: r#"
                    CREATE TABLE IF NOT EXISTS configuration (
                        key TEXT PRIMARY KEY NOT NULL,
                        value TEXT NOT NULL,
                        type TEXT NOT NULL CHECK(type IN ('string', 'number', 'boolean', 'json')),
                        description TEXT,
                        created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                        updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
                    );

                    CREATE INDEX IF NOT EXISTS idx_configuration_updated_at
                    ON configuration(updated_at);

                    -- Trigger to update updated_at on row update
                    CREATE TRIGGER IF NOT EXISTS update_configuration_updated_at
                    AFTER UPDATE ON configuration
                    BEGIN
                        UPDATE configuration SET updated_at = CURRENT_TIMESTAMP
                        WHERE key = NEW.key;
                    END;

                    -- Insert default configuration values
                    INSERT OR IGNORE INTO configuration (key, value, type, description)
                    VALUES
                        ('version', '0.1.0', 'string', 'CLI tool version'),
                        ('auto_update', 'true', 'boolean', 'Enable automatic updates check'),
                        ('telemetry_enabled', 'false', 'boolean', 'Enable anonymous usage telemetry');
                "#.to_string(),
                down_sql: Some("DROP TABLE IF EXISTS configuration;".to_string()),
            },
        );

        Self { migrations }
    }

    /// Get the current schema version from the database
    pub fn get_current_version(&self, db: &Database) -> Result<u32> {
        // First check if the migrations table exists
        let table_exists: bool = {
            let conn = db.get_connection();
            let conn = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to acquire lock: {}", e))?;

            let mut stmt = conn.prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            )?;

            stmt.exists([]).unwrap_or(false)
        };

        if !table_exists {
            return Ok(0);
        }

        // Get the maximum version from the migrations table
        let version: u32 = {
            let conn = db.get_connection();
            let conn = conn
                .lock()
                .map_err(|e| anyhow::anyhow!("Failed to acquire lock: {}", e))?;

            conn.query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
        };

        Ok(version)
    }

    /// Run all pending migrations
    pub fn run_migrations(&self, db: &Database) -> Result<()> {
        let current_version = self.get_current_version(db)?;

        debug!("Current database version: {}", current_version);

        let pending_migrations: Vec<_> = self
            .migrations
            .iter()
            .filter(|(version, _)| **version > current_version)
            .collect();

        if pending_migrations.is_empty() {
            debug!("Database is up to date");
            return Ok(());
        }

        debug!("Found {} pending migration(s)", pending_migrations.len());

        for (&version, migration) in pending_migrations {
            self.apply_migration(db, migration)
                .with_context(|| format!("Failed to apply migration {}", version))?;
        }

        debug!("All migrations completed successfully");
        Ok(())
    }

    /// Apply a single migration
    fn apply_migration(&self, db: &Database, migration: &Migration) -> Result<()> {
        debug!(
            "Applying migration {}: {}",
            migration.version, migration.description
        );

        let transaction = db.transaction()?;

        // Execute the migration SQL
        transaction
            .execute_batch(&migration.up_sql)
            .with_context(|| {
                format!(
                    "Failed to execute migration SQL for version {}",
                    migration.version
                )
            })?;

        // Calculate checksum for the migration
        let checksum = self.calculate_checksum(migration);

        // Record the migration in the migrations table
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, description, checksum) VALUES (?1, ?2, ?3)",
                rusqlite::params![migration.version, migration.description, checksum],
            )
            .with_context(|| format!("Failed to record migration {}", migration.version))?;

        transaction.commit()?;

        debug!("Migration {} applied successfully", migration.version);
        Ok(())
    }

    /// Rollback a migration (if down_sql is provided)
    pub fn rollback_migration(&self, db: &Database, version: u32) -> Result<()> {
        let migration = self
            .migrations
            .get(&version)
            .ok_or_else(|| anyhow::anyhow!("Migration {} not found", version))?;

        if let Some(down_sql) = &migration.down_sql {
            debug!(
                "Rolling back migration {}: {}",
                migration.version, migration.description
            );

            let transaction = db.transaction()?;

            // Execute the rollback SQL
            transaction
                .execute_batch(down_sql)
                .with_context(|| format!("Failed to rollback migration {}", version))?;

            // Remove the migration record
            transaction
                .execute(
                    "DELETE FROM schema_migrations WHERE version = ?1",
                    rusqlite::params![version],
                )
                .with_context(|| {
                    format!("Failed to remove migration record for version {}", version)
                })?;

            transaction.commit()?;

            debug!("Migration {} rolled back successfully", migration.version);
        } else {
            return Err(anyhow::anyhow!(
                "Migration {} does not support rollback",
                version
            ));
        }

        Ok(())
    }

    /// Calculate a checksum for a migration to detect changes
    fn calculate_checksum(&self, migration: &Migration) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        migration.version.hash(&mut hasher);
        migration.description.hash(&mut hasher);
        migration.up_sql.hash(&mut hasher);
        if let Some(down_sql) = &migration.down_sql {
            down_sql.hash(&mut hasher);
        }

        format!("{:x}", hasher.finish())
    }

    /// Verify all applied migrations match their expected checksums
    pub fn verify_migrations(&self, db: &Database) -> Result<()> {
        let conn = db.get_connection();
        let conn = conn
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire lock: {}", e))?;

        let mut stmt =
            conn.prepare("SELECT version, checksum FROM schema_migrations ORDER BY version")?;

        let applied_migrations = stmt
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        for (version, stored_checksum) in applied_migrations {
            if let Some(migration) = self.migrations.get(&version) {
                let expected_checksum = self.calculate_checksum(migration);
                if stored_checksum != expected_checksum {
                    return Err(anyhow::anyhow!(
                        "Migration {} has been modified! Expected checksum: {}, found: {}",
                        version,
                        expected_checksum,
                        stored_checksum
                    ));
                }
            } else {
                return Err(anyhow::anyhow!(
                    "Unknown migration {} found in database",
                    version
                ));
            }
        }

        Ok(())
    }

    /// Get list of all available migrations
    pub fn get_migrations(&self) -> Vec<&Migration> {
        self.migrations.values().collect()
    }

    /// Get list of pending migrations
    pub fn get_pending_migrations(&self, db: &Database) -> Result<Vec<&Migration>> {
        let current_version = self.get_current_version(db)?;

        Ok(self
            .migrations
            .iter()
            .filter(|(version, _)| **version > current_version)
            .map(|(_, migration)| migration)
            .collect())
    }
}

impl Default for MigrationManager {
    fn default() -> Self {
        Self::new()
    }
}
