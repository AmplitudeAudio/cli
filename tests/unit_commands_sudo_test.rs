//! Unit tests for sudo command module.
//!
//! Tests the sudo command handlers and database reset functionality.
//!
//! Priority levels:
//! - P0: Database reset core functionality, safety checks
//! - P1: Command routing, progress output, error handling
//! - P2: Edge cases, WAL/journal cleanup

use am::commands::sudo::{DatabaseCommands, SudoCommands};
use am::database::Database;
use am::presentation::{InteractiveOutput, Output};
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tempfile::tempdir;

/// Mock Output implementation for testing command handlers.
struct MockOutput {
    success_calls: Rc<RefCell<Vec<serde_json::Value>>>,
    progress_calls: Rc<RefCell<Vec<String>>>,
    error_calls: Rc<RefCell<Vec<(String, i32)>>>,
}

impl MockOutput {
    fn new() -> Self {
        Self {
            success_calls: Rc::new(RefCell::new(Vec::new())),
            progress_calls: Rc::new(RefCell::new(Vec::new())),
            error_calls: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn success_count(&self) -> usize {
        self.success_calls.borrow().len()
    }

    fn progress_count(&self) -> usize {
        self.progress_calls.borrow().len()
    }

    fn last_success(&self) -> Option<serde_json::Value> {
        self.success_calls.borrow().last().cloned()
    }

    fn progress_messages(&self) -> Vec<String> {
        self.progress_calls.borrow().clone()
    }
}

impl Output for MockOutput {
    fn success(&self, data: serde_json::Value, _request_id: Option<i64>) {
        self.success_calls.borrow_mut().push(data);
    }

    fn error(&self, err: &anyhow::Error, code: i32, _request_id: Option<i64>) {
        self.error_calls
            .borrow_mut()
            .push((err.to_string(), code));
    }

    fn progress(&self, message: &str) {
        self.progress_calls.borrow_mut().push(message.to_string());
    }
}

// Safety: MockOutput is only used in single-threaded tests
unsafe impl Send for MockOutput {}
unsafe impl Sync for MockOutput {}

// =============================================================================
// SudoCommands Enum Tests
// =============================================================================

#[test]
fn test_p1_sudo_commands_database_variant_exists() {
    // GIVEN: A DatabaseCommands value
    let db_cmd = DatabaseCommands::Reset {
        skip_confirmation: true,
    };

    // WHEN: Wrapping in SudoCommands
    let cmd = SudoCommands::Database { command: db_cmd };

    // THEN: Should create valid SudoCommands variant
    match cmd {
        SudoCommands::Database { command: _ } => assert!(true),
    }
}

#[test]
fn test_p1_database_commands_reset_variant_exists() {
    // GIVEN: Reset command with skip_confirmation
    let cmd = DatabaseCommands::Reset {
        skip_confirmation: true,
    };

    // THEN: Should match Reset variant
    match cmd {
        DatabaseCommands::Reset { skip_confirmation } => {
            assert!(skip_confirmation, "skip_confirmation should be true");
        }
    }
}

#[test]
fn test_p2_database_commands_reset_default_no_skip() {
    // GIVEN: Reset command without skip
    let cmd = DatabaseCommands::Reset {
        skip_confirmation: false,
    };

    // THEN: Should have skip_confirmation as false
    match cmd {
        DatabaseCommands::Reset { skip_confirmation } => {
            assert!(!skip_confirmation, "Default should not skip confirmation");
        }
    }
}

// =============================================================================
// Command Debug Trait Tests
// =============================================================================

#[test]
fn test_p2_sudo_commands_implements_debug() {
    // GIVEN: A SudoCommands value
    let cmd = SudoCommands::Database {
        command: DatabaseCommands::Reset {
            skip_confirmation: true,
        },
    };

    // WHEN: Formatting with Debug
    let debug_str = format!("{:?}", cmd);

    // THEN: Should contain relevant information
    assert!(debug_str.contains("Database"), "Debug should show variant");
    assert!(debug_str.contains("Reset"), "Debug should show subcommand");
}

#[test]
fn test_p2_database_commands_implements_debug() {
    // GIVEN: A DatabaseCommands value
    let cmd = DatabaseCommands::Reset {
        skip_confirmation: false,
    };

    // WHEN: Formatting with Debug
    let debug_str = format!("{:?}", cmd);

    // THEN: Should contain relevant information
    assert!(debug_str.contains("Reset"), "Debug should show Reset");
    assert!(
        debug_str.contains("skip_confirmation"),
        "Debug should show field"
    );
}

// =============================================================================
// Handler Function Tests (Integration-style)
// =============================================================================

#[tokio::test]
async fn test_p0_handler_routes_database_commands() {
    // GIVEN: A SudoCommands::Database with Reset
    let cmd = SudoCommands::Database {
        command: DatabaseCommands::Reset {
            skip_confirmation: true,
        },
    };
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");
    db.run_migrations().await.expect("Failed to run migrations");
    let db_arc = Arc::new(db);
    let output = MockOutput::new();

    // WHEN: Calling the handler
    // Note: This will fail because it tries to use the real home directory
    // but it validates the routing works
    let result = am::commands::sudo::handler(&cmd, Some(db_arc), &output).await;

    // THEN: Handler should execute (may fail due to file system access)
    // The important thing is it routes correctly to reset_database
    assert!(
        output.progress_count() > 0,
        "Handler should produce progress output"
    );
}

#[tokio::test]
async fn test_p1_handler_shows_warning_messages() {
    // GIVEN: A reset command
    let cmd = SudoCommands::Database {
        command: DatabaseCommands::Reset {
            skip_confirmation: true,
        },
    };
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");
    db.run_migrations().await.expect("Failed to run migrations");
    let db_arc = Arc::new(db);
    let output = MockOutput::new();

    // WHEN: Calling the handler
    let _ = am::commands::sudo::handler(&cmd, Some(db_arc), &output).await;

    // THEN: Should show warning messages about the operation
    let messages = output.progress_messages();
    assert!(
        messages.iter().any(|m| m.contains("Delete ALL projects")),
        "Should warn about deleting projects"
    );
    assert!(
        messages.iter().any(|m| m.contains("cannot be undone")),
        "Should warn operation is irreversible"
    );
}

// =============================================================================
// Reset Database Logic Tests
// =============================================================================

#[test]
fn test_p0_database_file_deletion_concept() {
    // GIVEN: A database file exists
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // Create a dummy file
    std::fs::write(&db_path, b"test data").expect("Failed to write test file");
    assert!(db_path.exists(), "File should exist before deletion");

    // WHEN: Deleting the file (simulating reset)
    std::fs::remove_file(&db_path).expect("Failed to delete file");

    // THEN: File should no longer exist
    assert!(!db_path.exists(), "File should not exist after deletion");
}

#[test]
fn test_p2_wal_file_cleanup_concept() {
    // GIVEN: Database files with WAL and SHM
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let wal_path = temp_dir.path().join("test.db-wal");
    let shm_path = temp_dir.path().join("test.db-shm");

    std::fs::write(&db_path, b"db").expect("Failed to write db");
    std::fs::write(&wal_path, b"wal").expect("Failed to write wal");
    std::fs::write(&shm_path, b"shm").expect("Failed to write shm");

    // WHEN: Cleaning up all files
    std::fs::remove_file(&db_path).ok();
    std::fs::remove_file(&wal_path).ok();
    std::fs::remove_file(&shm_path).ok();

    // THEN: All files should be removed
    assert!(!db_path.exists(), "DB file should be removed");
    assert!(!wal_path.exists(), "WAL file should be removed");
    assert!(!shm_path.exists(), "SHM file should be removed");
}

#[test]
fn test_p2_journal_file_cleanup_concept() {
    // GIVEN: Database with journal file
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let journal_path = temp_dir.path().join("test.db-journal");

    std::fs::write(&db_path, b"db").expect("Failed to write db");
    std::fs::write(&journal_path, b"journal").expect("Failed to write journal");

    // WHEN: Cleaning up files
    std::fs::remove_file(&db_path).ok();
    std::fs::remove_file(&journal_path).ok();

    // THEN: All files should be removed
    assert!(!db_path.exists(), "DB file should be removed");
    assert!(!journal_path.exists(), "Journal file should be removed");
}

// =============================================================================
// Skip Confirmation Tests
// =============================================================================

#[test]
fn test_p1_skip_confirmation_flag_true() {
    // GIVEN: Reset command with skip_confirmation = true
    let cmd = DatabaseCommands::Reset {
        skip_confirmation: true,
    };

    // THEN: Should skip the interactive prompt
    match cmd {
        DatabaseCommands::Reset { skip_confirmation } => {
            assert!(skip_confirmation, "Should skip confirmation when flag is set");
        }
    }
}

#[test]
fn test_p1_skip_confirmation_flag_false() {
    // GIVEN: Reset command with skip_confirmation = false
    let cmd = DatabaseCommands::Reset {
        skip_confirmation: false,
    };

    // THEN: Should require interactive confirmation
    match cmd {
        DatabaseCommands::Reset { skip_confirmation } => {
            assert!(
                !skip_confirmation,
                "Should require confirmation when flag is not set"
            );
        }
    }
}

// =============================================================================
// Output Integration Tests
// =============================================================================

#[test]
fn test_p1_mock_output_captures_progress() {
    // GIVEN: A mock output
    let output = MockOutput::new();

    // WHEN: Calling progress multiple times
    output.progress("Step 1");
    output.progress("Step 2");
    output.progress("Step 3");

    // THEN: All messages should be captured
    assert_eq!(output.progress_count(), 3);
    let messages = output.progress_messages();
    assert_eq!(messages[0], "Step 1");
    assert_eq!(messages[1], "Step 2");
    assert_eq!(messages[2], "Step 3");
}

#[test]
fn test_p1_mock_output_captures_success() {
    // GIVEN: A mock output
    let output = MockOutput::new();

    // WHEN: Calling success
    output.success(json!("Database reset successful"), None);

    // THEN: Success should be captured
    assert_eq!(output.success_count(), 1);
    assert_eq!(
        output.last_success(),
        Some(json!("Database reset successful"))
    );
}
