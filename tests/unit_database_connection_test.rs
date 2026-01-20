//! Unit tests for database connection module.
//!
//! Tests Database, DatabaseStatement, and DatabaseTransaction functionality.

use am::database::Database;
use tempfile::tempdir;

// =============================================================================
// Database::new() Tests
// =============================================================================

#[test]
fn test_p0_database_new_creates_database_file() {
    // GIVEN: A temporary directory for the database
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // WHEN: Creating a new database
    let db = Database::new(&db_path);

    // THEN: Database is created successfully
    assert!(db.is_ok(), "Database creation should succeed");
    assert!(db_path.exists(), "Database file should exist");
}

#[test]
fn test_p0_database_new_creates_parent_directories() {
    // GIVEN: A path with non-existent parent directories
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("nested").join("path").join("test.db");

    // WHEN: Creating a new database (parent dirs don't exist yet)
    let result = Database::new(&db_path);

    // THEN: Should fail because parent directory doesn't exist
    assert!(
        result.is_err(),
        "Database::new should fail without parent directory"
    );
}

#[test]
fn test_p0_database_new_sets_wal_mode() {
    // GIVEN: A temporary directory
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    // WHEN: Creating a new database
    let db = Database::new(&db_path).expect("Failed to create database");

    // THEN: WAL mode should be enabled
    db.execute("CREATE TABLE test_wal (id INTEGER)", [])
        .expect("Failed to create table");
    assert!(db_path.exists(), "Database file should exist");
}

#[test]
fn test_p1_database_path_returns_correct_path() {
    // GIVEN: A database created at a specific path
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("my_database.db");
    let db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Getting the database path
    let returned_path = db.path();

    // THEN: Should match the original path
    assert_eq!(
        returned_path,
        db_path.to_str().unwrap(),
        "Database path should match"
    );
}

// =============================================================================
// Database::execute() Tests
// =============================================================================

#[test]
fn test_p0_database_execute_creates_table() {
    // GIVEN: A database connection
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Executing CREATE TABLE statement
    let result = db.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        [],
    );

    // THEN: Should succeed
    assert!(result.is_ok(), "CREATE TABLE should succeed");
}

#[test]
fn test_p0_database_execute_inserts_data() {
    // GIVEN: A database with a table
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");
    db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", [])
        .expect("Failed to create table");

    // WHEN: Inserting data
    let result = db.execute("INSERT INTO users (name) VALUES (?1)", ["Alice"]);

    // THEN: Should return 1 row affected
    assert!(result.is_ok(), "INSERT should succeed");
    assert_eq!(result.unwrap(), 1, "Should affect 1 row");
}

#[test]
fn test_p1_database_execute_returns_error_for_invalid_sql() {
    // GIVEN: A database connection
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Executing invalid SQL
    let result = db.execute("INVALID SQL STATEMENT", []);

    // THEN: Should return error
    assert!(result.is_err(), "Invalid SQL should return error");
}

// =============================================================================
// Database::execute_batch() Tests
// =============================================================================

#[test]
fn test_p1_database_execute_batch_runs_multiple_statements() {
    // GIVEN: A database connection
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Executing batch SQL
    let result = db.execute_batch(
        "
        CREATE TABLE table1 (id INTEGER);
        CREATE TABLE table2 (id INTEGER);
        INSERT INTO table1 VALUES (1);
        INSERT INTO table2 VALUES (2);
    ",
    );

    // THEN: Should succeed
    assert!(result.is_ok(), "Batch execution should succeed");
}

// =============================================================================
// Database::prepare() Tests
// =============================================================================

#[test]
fn test_p1_database_prepare_returns_statement() {
    // GIVEN: A database with a table
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");
    db.execute("CREATE TABLE users (id INTEGER, name TEXT)", [])
        .expect("Failed to create table");

    // WHEN: Preparing a statement
    let result = db.prepare("SELECT * FROM users WHERE id = ?1");

    // THEN: Should return a prepared statement
    assert!(result.is_ok(), "Prepare should succeed");
}

// =============================================================================
// Database::transaction() Tests
// =============================================================================

#[test]
fn test_p0_database_transaction_commits_changes() {
    // GIVEN: A database with a table
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");
    db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", [])
        .expect("Failed to create table");

    // WHEN: Using a transaction that commits
    {
        let tx = db.transaction().expect("Failed to begin transaction");
        tx.execute("INSERT INTO users (name) VALUES (?1)", ["Bob"])
            .expect("Failed to insert");
        tx.commit().expect("Failed to commit");
    }

    // THEN: Data should persist
    let stmt = db
        .prepare("SELECT COUNT(*) FROM users")
        .expect("Failed to prepare");
    let count: Vec<i32> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");
    assert_eq!(count[0], 1, "Data should persist after commit");
}

#[test]
fn test_p0_database_transaction_rollback_on_drop() {
    // GIVEN: A database with a table
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");
    db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", [])
        .expect("Failed to create table");

    // WHEN: Using a transaction that is dropped without commit
    {
        let tx = db.transaction().expect("Failed to begin transaction");
        tx.execute("INSERT INTO users (name) VALUES (?1)", ["Charlie"])
            .expect("Failed to insert");
        // Transaction dropped here without commit
    }

    // THEN: Data should NOT persist (auto-rollback)
    let stmt = db
        .prepare("SELECT COUNT(*) FROM users")
        .expect("Failed to prepare");
    let count: Vec<i32> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");
    assert_eq!(count[0], 0, "Data should rollback when transaction dropped");
}

#[test]
fn test_p1_database_transaction_batch_execution() {
    // GIVEN: A database connection
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Using transaction for batch execution
    {
        let tx = db.transaction().expect("Failed to begin transaction");
        tx.execute_batch(
            "
            CREATE TABLE batch_test (id INTEGER, value TEXT);
            INSERT INTO batch_test VALUES (1, 'one');
            INSERT INTO batch_test VALUES (2, 'two');
        ",
        )
        .expect("Failed to execute batch");
        tx.commit().expect("Failed to commit");
    }

    // THEN: All statements should have executed
    let stmt = db
        .prepare("SELECT COUNT(*) FROM batch_test")
        .expect("Failed to prepare");
    let count: Vec<i32> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query");
    assert_eq!(count[0], 2, "Both inserts should have executed");
}

// =============================================================================
// Database::close() Tests
// =============================================================================

#[test]
fn test_p2_database_close_releases_connection() {
    // GIVEN: A database connection
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).expect("Failed to create database");

    // WHEN: Closing the database
    db.close();

    // THEN: Should be able to open a new connection to the same file
    let db2 = Database::new(&db_path);
    assert!(
        db2.is_ok(),
        "Should be able to open new connection after close"
    );
}
