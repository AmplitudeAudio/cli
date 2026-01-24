//! Test fixtures and helpers for Amplitude CLI tests.
//!
//! Provides reusable test infrastructure with automatic cleanup.

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::{tempdir, TempDir};

use am::database::Database;

/// Test fixture that provides an in-memory database for isolated testing.
///
/// # Example
/// ```
/// let fixture = TestDatabaseFixture::new().unwrap();
/// let db = fixture.database();
/// // Use database...
/// // Automatically cleaned up when fixture is dropped
/// ```
pub struct TestDatabaseFixture {
    _temp_dir: TempDir,
    db_path: PathBuf,
}

impl TestDatabaseFixture {
    /// Create a new test database fixture with a temporary directory.
    pub fn new() -> anyhow::Result<Self> {
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("test.db");

        Ok(Self {
            _temp_dir: temp_dir,
            db_path,
        })
    }

    /// Get the path to the test database file.
    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Get the temporary directory path (for creating project files).
    pub fn temp_path(&self) -> &std::path::Path {
        self._temp_dir.path()
    }
}

/// Test fixture for database with migrations already applied.
/// Provides Arc<Database> ready for use in tests.
pub struct MigratedDatabaseFixture {
    _temp_dir: TempDir,
    database: Arc<Database>,
}

impl MigratedDatabaseFixture {
    /// Create a new fixture with a fresh migrated database.
    pub async fn new() -> anyhow::Result<Self> {
        let temp_dir = tempdir()?;
        let db_path = temp_dir.path().join("test.db");
        let mut db = Database::new(&db_path)?;
        db.run_migrations().await?;

        Ok(Self {
            _temp_dir: temp_dir,
            database: Arc::new(db),
        })
    }

    /// Get the database Arc for use in tests.
    pub fn database(&self) -> Arc<Database> {
        self.database.clone()
    }

    /// Get the temporary directory path.
    pub fn temp_path(&self) -> &std::path::Path {
        self._temp_dir.path()
    }
}

/// Test fixture for isolated home directory operations.
/// Useful for testing functions that use dirs::home_dir().
pub struct IsolatedHomeFixture {
    _temp_dir: TempDir,
    amplitude_dir: PathBuf,
}

impl IsolatedHomeFixture {
    /// Create a new fixture with a temporary .amplitude directory.
    pub fn new() -> anyhow::Result<Self> {
        let temp_dir = tempdir()?;
        let amplitude_dir = temp_dir.path().join(".amplitude");
        std::fs::create_dir_all(&amplitude_dir)?;

        Ok(Self {
            _temp_dir: temp_dir,
            amplitude_dir,
        })
    }

    /// Get the path to the .amplitude directory.
    pub fn amplitude_dir(&self) -> &PathBuf {
        &self.amplitude_dir
    }

    /// Get the temporary "home" directory path.
    pub fn home_path(&self) -> &std::path::Path {
        self._temp_dir.path()
    }
}

/// Test fixture for project directory operations.
///
/// Creates a temporary directory structure mimicking a real Amplitude project.
pub struct TestProjectFixture {
    _temp_dir: TempDir,
    project_path: PathBuf,
}

impl TestProjectFixture {
    /// Create a new project fixture with standard directory structure.
    pub fn new(project_name: &str) -> anyhow::Result<Self> {
        let temp_dir = tempdir()?;
        let project_path = temp_dir.path().join(project_name);

        Ok(Self {
            _temp_dir: temp_dir,
            project_path,
        })
    }

    /// Get the project root path.
    pub fn project_path(&self) -> &PathBuf {
        &self.project_path
    }

    /// Get the temporary directory root.
    pub fn temp_root(&self) -> &std::path::Path {
        self._temp_dir.path()
    }

    /// Create a minimal .amproject file for testing.
    pub fn create_amproject_file(&self, name: &str) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.project_path)?;

        let config = serde_json::json!({
            "name": name,
            "default_configuration": "pc.config.amconfig",
            "sources_dir": "sources",
            "data_dir": "data",
            "build_dir": "build",
            "version": 1
        });

        let amproject_path = self.project_path.join(".amproject");
        std::fs::write(&amproject_path, serde_json::to_string_pretty(&config)?)?;

        Ok(())
    }
}

// =============================================================================
// Capture Output for Testing
// =============================================================================

use am::presentation::{Output, OutputMode};
use std::sync::RwLock;

/// Test output implementation that captures output for verification.
///
/// Unlike `JsonOutput` or `InteractiveOutput`, this captures what was passed
/// to each method, allowing tests to verify actual output content.
///
/// Uses `RwLock` instead of `RefCell` to satisfy the `Sync` requirement of `Output` trait.
///
/// # Example
/// ```ignore
/// let output = CaptureOutput::new(OutputMode::Json);
/// // Call handler that uses output...
/// assert!(output.last_success().is_some());
/// let data = output.last_success().unwrap();
/// assert!(data["count"].as_u64().unwrap() > 0);
/// ```
pub struct CaptureOutput {
    mode: OutputMode,
    successes: RwLock<Vec<serde_json::Value>>,
    errors: RwLock<Vec<(String, i32)>>,
    progress_messages: RwLock<Vec<String>>,
    tables: RwLock<Vec<(Option<String>, serde_json::Value)>>,
}

impl CaptureOutput {
    /// Create a new capture output with the specified mode.
    pub fn new(mode: OutputMode) -> Self {
        Self {
            mode,
            successes: RwLock::new(Vec::new()),
            errors: RwLock::new(Vec::new()),
            progress_messages: RwLock::new(Vec::new()),
            tables: RwLock::new(Vec::new()),
        }
    }

    /// Create a new capture output in JSON mode.
    pub fn json() -> Self {
        Self::new(OutputMode::Json)
    }

    /// Create a new capture output in Interactive mode.
    pub fn interactive() -> Self {
        Self::new(OutputMode::Interactive)
    }

    /// Get the last success data, if any.
    pub fn last_success(&self) -> Option<serde_json::Value> {
        self.successes.read().unwrap().last().cloned()
    }

    /// Get all success calls.
    pub fn all_successes(&self) -> Vec<serde_json::Value> {
        self.successes.read().unwrap().clone()
    }

    /// Get all progress messages.
    pub fn all_progress(&self) -> Vec<String> {
        self.progress_messages.read().unwrap().clone()
    }

    /// Get the last table data, if any.
    pub fn last_table(&self) -> Option<(Option<String>, serde_json::Value)> {
        self.tables.read().unwrap().last().cloned()
    }

    /// Get all table calls.
    pub fn all_tables(&self) -> Vec<(Option<String>, serde_json::Value)> {
        self.tables.read().unwrap().clone()
    }

    /// Get all errors.
    pub fn all_errors(&self) -> Vec<(String, i32)> {
        self.errors.read().unwrap().clone()
    }
}

impl Output for CaptureOutput {
    fn success(&self, data: serde_json::Value, _request_id: Option<i64>) {
        self.successes.write().unwrap().push(data);
    }

    fn error(&self, err: &anyhow::Error, code: i32, _request_id: Option<i64>) {
        self.errors.write().unwrap().push((err.to_string(), code));
    }

    fn progress(&self, message: &str) {
        self.progress_messages.write().unwrap().push(message.to_string());
    }

    fn table(&self, title: Option<&str>, data: serde_json::Value) {
        self.tables.write().unwrap().push((title.map(|s| s.to_string()), data));
    }

    fn mode(&self) -> OutputMode {
        self.mode
    }
}

/// Factory for creating test data with sensible defaults.
pub mod factories {
    use am::presentation::InteractiveOutput;

    /// Create a test project entity with optional overrides.
    pub fn create_test_project(name: Option<&str>, path: Option<&str>) -> (String, String) {
        let name = name.unwrap_or("test_project").to_string();
        let path = path.unwrap_or("/tmp/test_project").to_string();
        (name, path)
    }

    /// Create a test template entity with optional overrides.
    pub fn create_test_template(name: Option<&str>, path: Option<&str>) -> (String, String) {
        let name = name.unwrap_or("test_template").to_string();
        let path = path.unwrap_or("/tmp/templates/test").to_string();
        (name, path)
    }

    /// Create a test output handler (InteractiveOutput for testing).
    pub fn create_test_output() -> InteractiveOutput {
        InteractiveOutput::new()
    }
}

/// Assertion helpers for common test patterns.
pub mod assertions {
    use std::path::Path;

    /// Assert that a file exists at the given path.
    pub fn assert_file_exists(path: &Path) {
        assert!(
            path.exists(),
            "Expected file to exist at: {}",
            path.display()
        );
    }

    /// Assert that a directory exists at the given path.
    pub fn assert_dir_exists(path: &Path) {
        assert!(
            path.is_dir(),
            "Expected directory to exist at: {}",
            path.display()
        );
    }

    /// Assert that a path does not exist.
    pub fn assert_not_exists(path: &Path) {
        assert!(
            !path.exists(),
            "Expected path to not exist: {}",
            path.display()
        );
    }

    /// Assert that a file contains a specific string.
    pub fn assert_file_contains(path: &Path, expected: &str) {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|_| panic!("Failed to read file: {}", path.display()));
        assert!(
            content.contains(expected),
            "Expected file {} to contain '{}', but got:\n{}",
            path.display(),
            expected,
            content
        );
    }
}
