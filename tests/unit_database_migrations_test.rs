//! Unit tests for database migrations module.

use am::database::Database;
use tempfile::tempdir;

// =============================================================================
// Migration Execution Tests
// =============================================================================

#[tokio::test]
async fn test_p0_run_migrations_creates_schema_migrations_table() {
    // GIVEN: A fresh database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Running migrations
    let result = db.run_migrations().await;

    // THEN: Should succeed and create schema_migrations table
    assert!(result.is_ok(), "Migrations should succeed");

    let stmt = db
        .prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
        )
        .expect("Failed to prepare");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");
    assert_eq!(tables.len(), 1, "schema_migrations table should exist");
}

#[tokio::test]
async fn test_p0_run_migrations_creates_projects_table() {
    // GIVEN: A fresh database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Running migrations
    db.run_migrations().await.expect("Migrations should succeed");

    // THEN: Projects table should exist
    let stmt = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='projects'")
        .expect("Failed to prepare");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");
    assert_eq!(tables.len(), 1, "projects table should exist");
}

#[tokio::test]
async fn test_p0_run_migrations_creates_templates_table() {
    // GIVEN: A fresh database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Running migrations
    db.run_migrations().await.expect("Migrations should succeed");

    // THEN: Templates table should exist
    let stmt = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='templates'")
        .expect("Failed to prepare");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");
    assert_eq!(tables.len(), 1, "templates table should exist");
}

#[tokio::test]
async fn test_p0_run_migrations_creates_configuration_table() {
    // GIVEN: A fresh database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Running migrations
    db.run_migrations().await.expect("Migrations should succeed");

    // THEN: Configuration table should exist
    let stmt = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='configuration'")
        .expect("Failed to prepare");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");
    assert_eq!(tables.len(), 1, "configuration table should exist");
}

#[tokio::test]
async fn test_p1_run_migrations_records_migration_versions() {
    // GIVEN: A fresh database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Running migrations
    db.run_migrations().await.expect("Migrations should succeed");

    // THEN: Migration versions should be recorded
    let stmt = db
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .expect("Failed to prepare");
    let versions: Vec<u32> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");

    assert!(versions.len() >= 4, "Should have at least 4 migrations recorded");
    assert_eq!(versions[0], 1, "First migration should be version 1");
    assert_eq!(versions[1], 2, "Second migration should be version 2");
    assert_eq!(versions[2], 3, "Third migration should be version 3");
    assert_eq!(versions[3], 4, "Fourth migration should be version 4");
}

#[tokio::test]
async fn test_p1_run_migrations_stores_checksums() {
    // GIVEN: A fresh database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Running migrations
    db.run_migrations().await.expect("Migrations should succeed");

    // THEN: Checksums should be stored
    let stmt = db
        .prepare("SELECT checksum FROM schema_migrations WHERE version = 1")
        .expect("Failed to prepare");
    let checksums: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");

    assert_eq!(checksums.len(), 1, "Should have checksum for migration 1");
    assert!(!checksums[0].is_empty(), "Checksum should not be empty");
}

#[tokio::test]
async fn test_p0_run_migrations_is_idempotent() {
    // GIVEN: A database that has already been migrated
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");
    db.run_migrations()
        .await
        .expect("First migration should succeed");

    // WHEN: Running migrations again
    let result = db.run_migrations().await;

    // THEN: Should succeed without errors (no duplicate migrations)
    assert!(result.is_ok(), "Re-running migrations should succeed");

    // Verify migration count hasn't doubled
    let stmt = db
        .prepare("SELECT COUNT(*) FROM schema_migrations")
        .expect("Failed to prepare");
    let count: Vec<i32> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");
    assert_eq!(count[0], 4, "Should still have exactly 4 migrations");
}

// =============================================================================
// Default Configuration Tests
// =============================================================================

#[tokio::test]
async fn test_p1_migrations_insert_default_configuration() {
    // GIVEN: A fresh database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Running migrations
    db.run_migrations().await.expect("Migrations should succeed");

    // THEN: Default configuration values should exist
    let stmt = db
        .prepare("SELECT key, value FROM configuration ORDER BY key")
        .expect("Failed to prepare");
    let configs: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("Failed to query");

    assert!(configs.len() >= 3, "Should have at least 3 default configs");

    let keys: Vec<&str> = configs.iter().map(|(k, _)| k.as_str()).collect();
    assert!(keys.contains(&"version"), "Should have version config");
    assert!(
        keys.contains(&"auto_update"),
        "Should have auto_update config"
    );
    assert!(
        keys.contains(&"telemetry_enabled"),
        "Should have telemetry_enabled config"
    );
}

// =============================================================================
// Table Schema Tests
// =============================================================================

#[tokio::test]
async fn test_p1_projects_table_has_correct_columns() {
    // GIVEN: A migrated database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");
    db.run_migrations().await.expect("Migrations should succeed");

    // WHEN: Querying table info
    let stmt = db
        .prepare("PRAGMA table_info(projects)")
        .expect("Failed to prepare");
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("Failed to query");

    // THEN: Should have expected columns
    assert!(columns.contains(&"id".to_string()), "Should have id column");
    assert!(
        columns.contains(&"name".to_string()),
        "Should have name column"
    );
    assert!(
        columns.contains(&"path".to_string()),
        "Should have path column"
    );
    assert!(
        columns.contains(&"created_at".to_string()),
        "Should have created_at column"
    );
    assert!(
        columns.contains(&"updated_at".to_string()),
        "Should have updated_at column"
    );
}

#[tokio::test]
async fn test_p2_projects_table_has_unique_name_constraint() {
    // GIVEN: A migrated database with a project
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");
    db.run_migrations().await.expect("Migrations should succeed");

    db.execute(
        "INSERT INTO projects (name, path) VALUES ('test_project', '/path/to/project')",
        [],
    )
    .expect("First insert should succeed");

    // WHEN: Inserting a duplicate project name
    let result = db.execute(
        "INSERT INTO projects (name, path) VALUES ('test_project', '/different/path')",
        [],
    );

    // THEN: Should fail with unique constraint violation
    assert!(
        result.is_err(),
        "Duplicate project name should fail unique constraint"
    );
}
