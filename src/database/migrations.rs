// Copyright (c) 2026-present Sparky Studios. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
                        metadata TEXT -- JSON data for additional template info
                    );

                    CREATE INDEX IF NOT EXISTS idx_templates_name ON templates(name);
                    CREATE INDEX IF NOT EXISTS idx_templates_created_at ON templates(created_at);

                    -- Trigger to update updated_at on row update
                    CREATE TRIGGER IF NOT EXISTS update_templates_updated_at
                    AFTER UPDATE ON templates
                    BEGIN
                        UPDATE templates SET updated_at = CURRENT_TIMESTAMP
                        WHERE id = NEW.id;
                    END;
                "#
                .to_string(),
                down_sql: Some("DROP TABLE IF EXISTS templates;".to_string()),
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

        migrations.insert(
            5,
            Migration {
                version: 5,
                description: "Add engine and description columns to templates table".to_string(),
                up_sql: r#"
                    ALTER TABLE templates ADD COLUMN engine TEXT DEFAULT 'generic';
                    ALTER TABLE templates ADD COLUMN description TEXT;
                "#
                .to_string(),
                down_sql: None, // SQLite doesn't support DROP COLUMN in older versions
            },
        );

        migrations.insert(
            6,
            Migration {
                version: 6,
                description: "Add is_favorite column to projects table".to_string(),
                up_sql: r#"
                    ALTER TABLE projects ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0;
                    CREATE INDEX IF NOT EXISTS idx_projects_is_favorite ON projects(is_favorite);
                "#
                .to_string(),
                down_sql: None,
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
}

impl Default for MigrationManager {
    fn default() -> Self {
        Self::new()
    }
}
