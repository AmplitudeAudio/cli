# Amplitude CLI Test Suite

This directory contains the comprehensive test suite for the Amplitude CLI (`am`).

## Test Structure

```
tests/
├── common/                              # Shared test utilities
│   ├── mod.rs
│   └── fixtures.rs                      # Test fixtures, factories, assertions
│
├── unit_database_connection_test.rs     # Database, DatabaseStatement, DatabaseTransaction
├── unit_database_crud_test.rs           # db_create_*, db_get_*, db_forget_*
├── unit_database_entities_test.rs       # Project, Template, ProjectConfiguration
├── unit_database_migrations_test.rs     # MigrationManager, schema verification
├── unit_commands_project_test.rs        # validate_name, transform_name
├── unit_common_logger_test.rs           # LogEntry, Logger, formatting
├── unit_presentation_test.rs            # Output trait implementations
│
└── feature_project_lifecycle_test.rs    # Full project init/register/unregister
```

**Naming Convention:** `{type}_{module}_{component}_test.rs`
- `unit_*` - Unit tests (module-level isolation)
- `feature_*` - Feature/Integration tests (end-to-end workflows)

## Running Tests

```bash
# Run all tests
cargo test

# Run with output (see println! statements)
cargo test -- --nocapture

# Run specific test file
cargo test --test unit_presentation_test

# Run tests matching a pattern
cargo test database

# Run only P0 (critical) tests
cargo test p0

# Run only P1 (high priority) tests
cargo test p1

# Run tests in a specific module
cargo test unit_database

# Run with verbose output
cargo test -- --show-output
```

## Priority Tags

Tests are tagged with priority levels in their names:

| Priority | Description | When to Run |
|----------|-------------|-------------|
| **P0** | Critical paths, data integrity | Every commit |
| **P1** | High priority, important features | PR to main |
| **P2** | Medium priority, edge cases | Nightly builds |
| **P3** | Low priority, nice-to-have | On-demand |

### Running by Priority

```bash
# Critical tests only (fastest feedback)
cargo test p0

# Critical + High priority
cargo test "p0\|p1"

# All except low priority
cargo test "p0\|p1\|p2"
```

## Test Categories

### Unit Tests (`unit_*_test.rs`)

Test individual modules in isolation:

- **unit_database_connection_test.rs**: Database connection, transactions, WAL mode
- **unit_database_crud_test.rs**: CRUD operations for projects and templates
- **unit_database_entities_test.rs**: Data structure serialization/deserialization
- **unit_database_migrations_test.rs**: Schema migrations, checksums, idempotency
- **unit_commands_project_test.rs**: Name validation and transformation
- **unit_common_logger_test.rs**: Log entry formatting, verbose mode
- **unit_presentation_test.rs**: Output trait abstraction layer

### Feature Tests (`feature_*_test.rs`)

Test end-to-end workflows:

- **feature_project_lifecycle_test.rs**: Full project init → register → unregister flow

## Test Fixtures

Located in `tests/common/fixtures.rs`:

### TestDatabaseFixture

Creates an isolated test database in a temporary directory:

```rust
let fixture = TestDatabaseFixture::new().unwrap();
let db = Database::new(fixture.db_path()).unwrap();
// Database is cleaned up when fixture is dropped
```

### TestProjectFixture

Creates a temporary project directory structure:

```rust
let fixture = TestProjectFixture::new("my_project").unwrap();
fixture.create_amproject_file("my_project").unwrap();
// Project directory is cleaned up when fixture is dropped
```

### Factories

Create test data with sensible defaults:

```rust
use crate::common::fixtures::factories::*;

let (name, path) = create_test_project(None, None);
let output = create_test_output();
```

### Assertions

Common assertion helpers:

```rust
use crate::common::fixtures::assertions::*;

assert_file_exists(&path);
assert_dir_exists(&path);
assert_not_exists(&path);
assert_file_contains(&path, "expected content");
```

## Writing New Tests

### Naming Convention

```rust
#[test]
fn test_p{priority}_{function}_{scenario}_{expected_outcome}() {
    // GIVEN: Setup conditions

    // WHEN: Action under test

    // THEN: Assertions
}
```

Example:
```rust
#[test]
fn test_p0_database_transaction_commits_changes() {
    // GIVEN: A database with a table
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db = Database::new(temp_dir.path().join("test.db")).unwrap();

    // WHEN: Using a transaction that commits
    let tx = db.transaction().unwrap();
    tx.execute("INSERT INTO users (name) VALUES (?1)", ["Alice"]).unwrap();
    tx.commit().unwrap();

    // THEN: Data should persist
    // ... assertions ...
}
```

### Async Tests

For tests that require async operations (e.g., migrations):

```rust
#[tokio::test]
async fn test_p0_run_migrations_creates_tables() {
    let temp_dir = tempdir().unwrap();
    let mut db = Database::new(temp_dir.path().join("test.db")).unwrap();

    db.run_migrations().await.unwrap();

    // ... assertions ...
}
```

### Test Isolation

- Each test should be independent
- Use `tempdir()` for file system isolation
- Use in-memory or temporary databases for data isolation
- Clean up resources in fixtures (auto-cleanup on drop)

## Coverage Analysis

To check test coverage (requires `cargo-tarpaulin`):

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Run coverage report
cargo tarpaulin --out Html

# View report
open tarpaulin-report.html
```

## CI Integration

These tests are designed to run in CI pipelines:

```yaml
# Example GitHub Actions
- name: Run tests
  run: cargo test --all-features

# Run P0 tests on every push
- name: Critical tests
  run: cargo test p0

# Full test suite on PR
- name: Full test suite
  run: cargo test
```

## Troubleshooting

### Tests failing due to database locks

Ensure each test uses its own temporary directory:
```rust
let temp_dir = tempdir().unwrap();  // Unique per test
```

### Async tests timing out

Increase tokio test timeout:
```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_with_longer_timeout() {
    // ...
}
```

### Tests polluting each other

Check that tests don't rely on global state. Use fixtures for isolation.
