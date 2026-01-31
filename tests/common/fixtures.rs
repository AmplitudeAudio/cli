//! Test fixtures and helpers for Amplitude CLI tests.
//!
//! Provides reusable test infrastructure with automatic cleanup.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::{TempDir, tempdir};

use am::assets::ProjectValidator;
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
// Asset Test Fixture
// =============================================================================

/// Test fixture for asset operations with a fully populated project directory.
///
/// Creates a temporary project directory with the complete SDK source structure
/// (`sources/sounds/`, `sources/collections/`, etc.) and provides helpers for
/// creating asset JSON files. Each fixture instance is isolated via `TempDir`,
/// so tests can run concurrently without interference.
///
/// # Example
///
/// ```ignore
/// let fixture = AssetTestFixture::new("test_project")?;
/// fixture.create_test_sound("footstep", 42)?;
/// fixture.create_data_file("footstep.wav")?;
/// let validator = fixture.create_project_validator()?;
/// assert!(validator.validate_sound_exists(42).is_ok());
/// ```
pub struct AssetTestFixture {
    _temp_dir: TempDir,
    project_root: PathBuf,
}

impl AssetTestFixture {
    /// Create a new asset test fixture with a full SDK directory structure.
    ///
    /// Creates a temp directory containing:
    /// - `{project_name}/.amproject` (valid project configuration)
    /// - `sources/{type}/` directories for all SDK asset types
    /// - `data/`, `build/`, `plugins/` directories
    pub fn new(project_name: &str) -> anyhow::Result<Self> {
        let temp_dir = tempdir()?;
        let project_root = temp_dir.path().join(project_name);

        // Create all SDK source directories
        let source_dirs = [
            "sources/sounds",
            "sources/collections",
            "sources/effects",
            "sources/switches",
            "sources/switch_containers",
            "sources/soundbanks",
            "sources/events",
            "sources/attenuators",
            "sources/pipelines",
            "sources/rtpc",
        ];

        for dir in &source_dirs {
            std::fs::create_dir_all(project_root.join(dir))?;
        }

        // Create additional project directories
        std::fs::create_dir_all(project_root.join("data"))?;
        std::fs::create_dir_all(project_root.join("build"))?;
        std::fs::create_dir_all(project_root.join("plugins"))?;

        // Write a valid .amproject file
        let amproject = serde_json::json!({
            "name": project_name,
            "default_configuration": "pc.config.amconfig",
            "sources_dir": "sources",
            "data_dir": "data",
            "build_dir": "build",
            "version": 1
        });
        std::fs::write(
            project_root.join(".amproject"),
            serde_json::to_string_pretty(&amproject)?,
        )?;

        Ok(Self {
            _temp_dir: temp_dir,
            project_root,
        })
    }

    /// Get the project root path.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the sources directory path (`project_root/sources/`).
    pub fn sources_dir(&self) -> PathBuf {
        self.project_root.join("sources")
    }

    /// Create a minimal valid Sound JSON file.
    ///
    /// Writes a sound JSON matching the SDK format (with `RtpcCompatibleValue` tagged
    /// enum for `gain`/`priority` and `SoundLoopConfig` for `loop`) to
    /// `sources/sounds/{name}.json`.
    pub fn create_test_sound(&self, name: &str, id: u64) -> anyhow::Result<PathBuf> {
        let sound_json = serde_json::json!({
            "id": id,
            "name": name,
            "path": format!("data/{}.wav", name),
            "bus": 0,
            "gain": { "kind": "Static", "value": 1.0 },
            "priority": { "kind": "Static", "value": 128.0 },
            "stream": false,
            "loop": { "enabled": false, "loop_count": 0 },
            "spatialization": "None",
            "attenuation": 0,
            "scope": "World",
            "fader": "Linear",
            "effect": 0
        });

        let path = self
            .project_root
            .join("sources/sounds")
            .join(format!("{}.json", name));
        std::fs::write(&path, serde_json::to_string_pretty(&sound_json)?)?;
        Ok(path)
    }

    /// Create a minimal Collection JSON file.
    ///
    /// Writes a collection JSON with the given sound ID references to
    /// `sources/collections/{name}.json`. Uses `serde_json::Value` directly
    /// since the Collection struct is not yet implemented.
    pub fn create_test_collection(
        &self,
        name: &str,
        id: u64,
        sound_ids: &[u64],
    ) -> anyhow::Result<PathBuf> {
        let collection_json = serde_json::json!({
            "id": id,
            "name": name,
            "sound_ids": sound_ids,
            "mode": "random",
            "scope": "World"
        });

        let path = self
            .project_root
            .join("sources/collections")
            .join(format!("{}.json", name));
        std::fs::write(&path, serde_json::to_string_pretty(&collection_json)?)?;
        Ok(path)
    }

    /// Write an arbitrary asset JSON file to the appropriate source directory.
    ///
    /// Maps `asset_type` to its directory under `sources/` and writes the JSON
    /// value to `sources/{asset_type}/{name}.json`.
    pub fn write_asset_json(
        &self,
        asset_type: &str,
        name: &str,
        json: serde_json::Value,
    ) -> anyhow::Result<PathBuf> {
        let path = self
            .project_root
            .join("sources")
            .join(asset_type)
            .join(format!("{}.json", name));
        std::fs::write(&path, serde_json::to_string_pretty(&json)?)?;
        Ok(path)
    }

    /// Create a `ProjectValidator` that scans this fixture's project directory.
    pub fn create_project_validator(&self) -> anyhow::Result<ProjectValidator> {
        ProjectValidator::new(self.project_root.clone())
    }

    /// Create an empty file in the `data/` directory.
    ///
    /// Useful for tests that validate audio file existence (e.g., `Sound::validate_rules`).
    pub fn create_data_file(&self, name: &str) -> anyhow::Result<PathBuf> {
        let path = self.project_root.join("data").join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, b"")?;
        Ok(path)
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
        self.progress_messages
            .write()
            .unwrap()
            .push(message.to_string());
    }

    fn table(&self, title: Option<&str>, data: serde_json::Value) {
        self.tables
            .write()
            .unwrap()
            .push((title.map(|s| s.to_string()), data));
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
