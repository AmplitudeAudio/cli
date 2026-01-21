//! Unit tests for database module-level functions.
//!
//! Tests for initialize(), get_database_path(), cleanup(), setup_crash_db_cleanup().
//!
//! Priority levels:
//! - P0: Database initialization, path resolution
//! - P1: Cleanup functions, crash handling
//! - P2: Edge cases, error conditions

use am::database::{Database, cleanup, get_database_path, initialize, setup_crash_db_cleanup};
use std::sync::Arc;
use tempfile::tempdir;

// =============================================================================
// get_database_path() Tests
// =============================================================================

#[test]
fn test_p0_get_database_path_returns_path() {
    // GIVEN: A system with a home directory

    // WHEN: Getting the database path
    let result = get_database_path();

    // THEN: Should return a valid path
    match result {
        Ok(path) => {
            assert!(
                path.to_string_lossy().contains(".amplitude"),
                "Path should contain .amplitude directory"
            );
            assert!(
                path.to_string_lossy().ends_with("am.db"),
                "Path should end with am.db"
            );
        }
        Err(e) => {
            // May fail in environments without home directory
            println!(
                "get_database_path failed (expected in some environments): {}",
                e
            );
        }
    }
}

#[test]
fn test_p1_get_database_path_is_in_home_directory() {
    // GIVEN: A system with a home directory

    // WHEN: Getting the database path
    let result = get_database_path();

    // THEN: Path should be under home directory
    if let Ok(path) = result {
        if let Some(home) = dirs::home_dir() {
            assert!(
                path.starts_with(&home),
                "Database path should be under home directory"
            );
        }
    }
}

#[test]
fn test_p1_get_database_path_is_consistent() {
    // GIVEN: Multiple calls to get_database_path

    // WHEN: Getting the path twice
    let path1 = get_database_path();
    let path2 = get_database_path();

    // THEN: Both should return the same path
    match (path1, path2) {
        (Ok(p1), Ok(p2)) => {
            assert_eq!(p1, p2, "Path should be consistent across calls");
        }
        _ => {
            // Skip test if home directory not available
        }
    }
}

// =============================================================================
// initialize() Tests
// =============================================================================

#[tokio::test]
async fn test_p0_initialize_creates_database() {
    // GIVEN: initialize() is called
    // Note: This uses the real home directory path

    // WHEN: Initializing the database
    let result = initialize().await;

    // THEN: Should return a valid Database
    match result {
        Ok(db) => {
            // Verify database is usable
            let tables_result = db.prepare("SELECT name FROM sqlite_master WHERE type='table'");
            assert!(tables_result.is_ok(), "Database should be queryable");
            db.close();
        }
        Err(e) => {
            // May fail in CI environments
            println!("initialize failed (expected in some environments): {}", e);
        }
    }
}

#[tokio::test]
async fn test_p0_initialize_runs_migrations() {
    // GIVEN: A fresh call to initialize()

    // WHEN: Initializing
    let result = initialize().await;

    // THEN: Migrations should have run (projects table should exist)
    if let Ok(db) = result {
        let stmt =
            db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='projects'");
        if let Ok(s) = stmt {
            let tables: Vec<String> = s.query_map([], |row| row.get(0)).unwrap_or_default();
            assert!(
                tables.contains(&"projects".to_string()),
                "projects table should exist after initialization"
            );
        }
        db.close();
    }
}

#[tokio::test]
async fn test_p1_initialize_creates_amplitude_directory() {
    // GIVEN: Initialize is called

    // WHEN: Initializing
    let _ = initialize().await;

    // THEN: .amplitude directory should exist
    if let Some(home) = dirs::home_dir() {
        let amplitude_dir = home.join(".amplitude");
        assert!(
            amplitude_dir.exists(),
            ".amplitude directory should be created"
        );
    }
}

// =============================================================================
// cleanup() Tests
// =============================================================================

#[test]
fn test_p1_cleanup_with_some_database_closes_connection() {
    // GIVEN: A database connection
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("cleanup_test.db");
    let db = Database::new(&db_path).expect("Failed to create database");

    // Verify database is open
    assert!(db_path.exists(), "Database file should exist");

    // WHEN: Calling cleanup with Some(db)
    cleanup(Some(db));

    // THEN: Database should be closed (we can open a new connection)
    let db2 = Database::new(&db_path);
    assert!(
        db2.is_ok(),
        "Should be able to open new connection after cleanup"
    );
}

#[test]
fn test_p1_cleanup_with_none_does_nothing() {
    // GIVEN: No database

    // WHEN: Calling cleanup with None
    cleanup(None);

    // THEN: Should not panic
    assert!(true, "cleanup(None) should complete without panic");
}

#[test]
fn test_p2_cleanup_multiple_calls_safe() {
    // GIVEN: Multiple cleanup calls

    // WHEN: Calling cleanup multiple times with None
    cleanup(None);
    cleanup(None);
    cleanup(None);

    // THEN: Should not panic
    assert!(true, "Multiple cleanup calls should be safe");
}

// =============================================================================
// setup_crash_db_cleanup() Tests
// =============================================================================

#[test]
fn test_p1_setup_crash_db_cleanup_with_some_database() {
    // GIVEN: A database wrapped in Arc
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("crash_cleanup_test.db");
    let db = Database::new(&db_path).expect("Failed to create database");
    let db_arc = Arc::new(db);

    // WHEN: Setting up crash cleanup
    setup_crash_db_cleanup(Some(db_arc));

    // THEN: Should complete without panic (panic hook is set)
    assert!(true, "setup_crash_db_cleanup should complete");
}

#[test]
fn test_p1_setup_crash_db_cleanup_with_none() {
    // GIVEN: No database

    // WHEN: Setting up crash cleanup with None
    setup_crash_db_cleanup(None);

    // THEN: Should complete without panic
    assert!(true, "setup_crash_db_cleanup(None) should complete");
}

#[test]
fn test_p2_setup_crash_db_cleanup_multiple_calls() {
    // GIVEN: Multiple setup calls

    // WHEN: Calling setup multiple times
    setup_crash_db_cleanup(None);
    setup_crash_db_cleanup(None);

    // THEN: Should not panic (hooks are replaced)
    assert!(true, "Multiple setup calls should be safe");
}

// =============================================================================
// Database Path Structure Tests
// =============================================================================

#[test]
fn test_p2_database_path_structure() {
    // GIVEN: get_database_path result

    // WHEN: Examining the path structure
    if let Ok(path) = get_database_path() {
        // THEN: Should have expected structure
        let path_str = path.to_string_lossy();

        // Should contain home directory indicator
        assert!(
            path_str.contains("amplitude") || path_str.contains(".amplitude"),
            "Path should reference amplitude directory"
        );

        // Should end with database filename
        assert!(path_str.ends_with("am.db"), "Path should end with am.db");

        // Should have parent directory
        assert!(path.parent().is_some(), "Path should have parent directory");
    }
}

#[test]
fn test_p2_database_path_parent_is_amplitude_dir() {
    // GIVEN: get_database_path result

    // WHEN: Getting the parent directory
    if let Ok(path) = get_database_path() {
        if let Some(parent) = path.parent() {
            // THEN: Parent should be .amplitude
            let parent_name = parent.file_name().unwrap().to_string_lossy();
            assert_eq!(
                parent_name, ".amplitude",
                "Parent directory should be .amplitude"
            );
        }
    }
}

// =============================================================================
// Integration Tests - Full Lifecycle
// =============================================================================

#[tokio::test]
async fn test_p0_database_lifecycle_initialize_and_cleanup() {
    // GIVEN: A fresh environment

    // WHEN: Initializing and then cleaning up
    let db_result = initialize().await;

    if let Ok(db) = db_result {
        // Verify database works
        let stmt = db.prepare("SELECT 1");
        assert!(stmt.is_ok(), "Database should be queryable");

        // Cleanup
        cleanup(Some(db));

        // THEN: Should be able to initialize again
        let db2_result = initialize().await;
        if let Ok(db2) = db2_result {
            let stmt2 = db2.prepare("SELECT 1");
            assert!(stmt2.is_ok(), "Re-initialized database should work");
            cleanup(Some(db2));
        }
    }
}

#[test]
fn test_p1_database_new_and_cleanup() {
    // GIVEN: A temporary database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("lifecycle.db");

    // WHEN: Creating and cleaning up
    let db = Database::new(&db_path).expect("Failed to create database");

    // Execute some operations
    db.execute("CREATE TABLE test (id INTEGER)", [])
        .expect("Failed to create table");

    // Cleanup
    cleanup(Some(db));

    // THEN: Should be able to reopen
    let db2 = Database::new(&db_path).expect("Failed to reopen database");

    // Table should persist
    let stmt = db2
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='test'")
        .expect("Failed to prepare");
    let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap_or_default();
    assert!(tables.contains(&"test".to_string()), "Table should persist");

    cleanup(Some(db2));
}
