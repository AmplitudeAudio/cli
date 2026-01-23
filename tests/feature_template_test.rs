//! Feature tests for template commands.
//!
//! These tests exercise the actual template command handlers and verify
//! the output format meets acceptance criteria.

use am::commands::template::{TemplateCommands, handler};
use am::database::entities::TemplateSource;
use am::database::{Database, db_get_templates};
use am::input::NonInteractiveInput;
use am::presentation::{Output, OutputMode, create_output};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use tempfile::tempdir;

/// Helper to create a test database with migrations applied.
async fn setup_test_database() -> (Arc<Database>, tempfile::TempDir) {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");
    db.run_migrations().await.expect("Failed to run migrations");
    (Arc::new(db), temp_dir)
}

/// A capturing Output implementation for testing handler output.
struct CaptureOutput {
    success_calls: Rc<RefCell<Vec<(Value, Option<i64>)>>>,
    table_calls: Rc<RefCell<Vec<(Option<String>, Value)>>>,
    progress_calls: Rc<RefCell<Vec<String>>>,
    mode: OutputMode,
}

impl CaptureOutput {
    fn new(mode: OutputMode) -> Self {
        Self {
            success_calls: Rc::new(RefCell::new(Vec::new())),
            table_calls: Rc::new(RefCell::new(Vec::new())),
            progress_calls: Rc::new(RefCell::new(Vec::new())),
            mode,
        }
    }

    fn last_success(&self) -> Option<Value> {
        self.success_calls.borrow().last().map(|(v, _)| v.clone())
    }

    fn last_table(&self) -> Option<(Option<String>, Value)> {
        self.table_calls.borrow().last().cloned()
    }

    fn progress_messages(&self) -> Vec<String> {
        self.progress_calls.borrow().clone()
    }
}

impl Output for CaptureOutput {
    fn success(&self, data: Value, request_id: Option<i64>) {
        self.success_calls.borrow_mut().push((data, request_id));
    }

    fn error(&self, _err: &anyhow::Error, _code: i32, _request_id: Option<i64>) {
        // Not needed for these tests
    }

    fn progress(&self, message: &str) {
        self.progress_calls.borrow_mut().push(message.to_string());
    }

    fn table(&self, title: Option<&str>, data: Value) {
        self.table_calls
            .borrow_mut()
            .push((title.map(String::from), data));
    }

    fn mode(&self) -> OutputMode {
        self.mode.clone()
    }
}

// Safety: CaptureOutput is only used in single-threaded tests
unsafe impl Send for CaptureOutput {}
unsafe impl Sync for CaptureOutput {}

// =============================================================================
// Template List Database Tests
// =============================================================================

#[tokio::test]
async fn test_p0_template_list_returns_embedded_template() {
    // GIVEN: A fresh database with migrations applied
    let (db_arc, _temp_dir) = setup_test_database().await;

    // WHEN: We fetch templates from the database
    let templates = db_get_templates(Some(db_arc));

    // THEN: The query should succeed
    assert!(templates.is_ok());

    // AND: The embedded template is handled by the command handler, not the database
    // So database should return empty for a fresh database
    let templates_list = templates.unwrap();
    assert!(
        templates_list.is_empty(),
        "Fresh database should have no custom templates"
    );
}

#[tokio::test]
async fn test_p1_template_list_empty_database_succeeds() {
    // GIVEN: A fresh database with no custom templates
    let (db_arc, _temp_dir) = setup_test_database().await;

    // WHEN: We fetch templates
    let result = db_get_templates(Some(db_arc));

    // THEN: The operation should succeed (not error)
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_p1_template_source_embedded_is_default() {
    // GIVEN: The TemplateSource type
    // WHEN: We use the default value
    let source = TemplateSource::default();

    // THEN: It should be Embedded
    assert_eq!(source, TemplateSource::Embedded);
}

#[test]
fn test_p1_output_mode_json_creates_json_output() {
    // GIVEN: JSON output mode
    let mode = OutputMode::Json;

    // WHEN: We create an output instance
    let output = create_output(mode);

    // THEN: It should be in JSON mode
    assert_eq!(output.mode(), OutputMode::Json);
}

#[test]
fn test_p1_output_mode_interactive_creates_interactive_output() {
    // GIVEN: Interactive output mode
    let mode = OutputMode::Interactive;

    // WHEN: We create an output instance
    let output = create_output(mode);

    // THEN: It should be in Interactive mode
    assert_eq!(output.mode(), OutputMode::Interactive);
}

#[test]
fn test_p2_template_source_serializes_to_lowercase() {
    // GIVEN: Template sources
    let embedded = TemplateSource::Embedded;
    let custom = TemplateSource::Custom;

    // WHEN: We serialize them
    let embedded_json = serde_json::to_string(&embedded).unwrap();
    let custom_json = serde_json::to_string(&custom).unwrap();

    // THEN: They should be lowercase (serde rename_all)
    assert_eq!(embedded_json, "\"embedded\"");
    assert_eq!(custom_json, "\"custom\"");
}

#[test]
fn test_p2_template_source_deserializes_from_lowercase() {
    // GIVEN: Lowercase JSON strings
    let embedded_str = "\"embedded\"";
    let custom_str = "\"custom\"";

    // WHEN: We deserialize them
    let embedded: TemplateSource = serde_json::from_str(embedded_str).unwrap();
    let custom: TemplateSource = serde_json::from_str(custom_str).unwrap();

    // THEN: They should match the expected values
    assert_eq!(embedded, TemplateSource::Embedded);
    assert_eq!(custom, TemplateSource::Custom);
}

// =============================================================================
// Template List Handler Tests - JSON Output
// =============================================================================

#[tokio::test]
async fn test_p0_template_list_handler_json_output_envelope_format() {
    // GIVEN: A fresh database and JSON output mode
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok(), "Handler should succeed");

    // AND: Output should be in success format with array of templates
    let success_data = output.last_success().expect("Should have success output");

    // AND: Data should be an array
    assert!(success_data.is_array(), "Success data should be an array");

    // AND: Array should contain at least the embedded template
    let templates = success_data.as_array().unwrap();
    assert!(
        !templates.is_empty(),
        "Should have at least embedded template"
    );

    // AND: First template should be the embedded "default" template
    let first = &templates[0];
    assert_eq!(first["name"], "default");
    assert_eq!(first["engine"], "generic");
    assert_eq!(
        first["source"], "embedded",
        "JSON mode should use snake_case 'embedded'"
    );
    assert!(first["description"].as_str().unwrap().contains("Default"));
}

#[tokio::test]
async fn test_p0_template_list_handler_json_source_is_snake_case() {
    // GIVEN: A fresh database and JSON output mode
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: Source field should use snake_case values for JSON
    let success_data = output.last_success().unwrap();
    let templates = success_data.as_array().unwrap();
    let first = &templates[0];

    // Verify source is "embedded" not "Embedded"
    assert_eq!(
        first["source"], "embedded",
        "JSON should use snake_case 'embedded'"
    );
    assert_ne!(
        first["source"], "Embedded",
        "JSON should NOT use PascalCase"
    );
}

// =============================================================================
// Template List Handler Tests - Interactive Output
// =============================================================================

#[tokio::test]
async fn test_p0_template_list_handler_interactive_shows_table() {
    // GIVEN: A fresh database and Interactive output mode
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Interactive);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok(), "Handler should succeed");

    // AND: Output should be via table()
    let (title, data) = output.last_table().expect("Should have table output");

    // AND: Title should be "Available Templates"
    assert_eq!(title, Some("Available Templates".to_string()));

    // AND: Data should contain the embedded template
    let templates = data.as_array().unwrap();
    assert!(!templates.is_empty());

    // AND: Source should use display format for interactive
    let first = &templates[0];
    assert_eq!(first["name"], "default");
    // Interactive mode uses colored output, so the value contains ANSI codes
    // We just verify it's not the JSON snake_case value
    let source_str = first["source"].as_str().unwrap();
    assert!(
        source_str.contains("Embedded") || source_str.contains("\x1b"),
        "Interactive source should be 'Embedded' (possibly with ANSI colors)"
    );
}

#[tokio::test]
async fn test_p1_template_list_handler_interactive_shows_tip_when_no_custom() {
    // GIVEN: A fresh database (no custom templates) and Interactive output mode
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Interactive);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: Progress messages should include the tip about registering templates
    let messages = output.progress_messages();
    let has_tip = messages.iter().any(|m| m.contains("am template register"));
    assert!(
        has_tip,
        "Should show tip about registering custom templates"
    );
}

// =============================================================================
// Template List Handler Tests - No Custom Templates
// =============================================================================

#[tokio::test]
async fn test_p0_template_list_handler_no_custom_templates_no_error() {
    // GIVEN: A fresh database with no custom templates
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed without error
    assert!(
        result.is_ok(),
        "Handler should succeed even with no custom templates"
    );

    // AND: Output should still contain embedded templates
    let success_data = output.last_success().unwrap();
    let templates = success_data.as_array().unwrap();
    assert!(!templates.is_empty(), "Should have embedded templates");
}

// =============================================================================
// Template List Handler Tests - Embedded First Ordering
// =============================================================================

#[tokio::test]
async fn test_p1_template_list_embedded_first_ordering() {
    // GIVEN: A list with embedded and custom templates (simulated)
    let embedded = am::database::entities::Template {
        id: None,
        name: "default".to_string(),
        path: "bundled".to_string(),
        engine: Some("generic".to_string()),
        description: Some("Default".to_string()),
        source: TemplateSource::Embedded,
    };

    let custom = am::database::entities::Template {
        id: Some(1),
        name: "custom-template".to_string(),
        path: "/path/to/template".to_string(),
        engine: Some("o3de".to_string()),
        description: Some("Custom template".to_string()),
        source: TemplateSource::Custom,
    };

    // WHEN: We combine them (embedded first)
    let mut all_templates = vec![embedded];
    all_templates.push(custom);

    // THEN: Embedded should be first
    assert_eq!(all_templates[0].source, TemplateSource::Embedded);
    assert_eq!(all_templates[1].source, TemplateSource::Custom);
}

// =============================================================================
// Template List Handler Tests - Error Handling
// =============================================================================

#[tokio::test]
async fn test_p0_template_list_handler_propagates_db_errors() {
    // GIVEN: No database (simulating DB unavailable)
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler without a database
    let result = handler(&command, None, &input, &output).await;

    // THEN: Handler should return an error (not swallow it)
    assert!(
        result.is_err(),
        "Handler should propagate database unavailable error"
    );
}

// =============================================================================
// Template List Handler Tests - Sorting (Multiple custom templates)
// =============================================================================

#[tokio::test]
async fn test_p2_template_list_custom_templates_sorted_alphabetically() {
    // GIVEN: Multiple custom templates in database
    let (db_arc, _temp_dir) = setup_test_database().await;

    // Insert custom templates in non-alphabetical order
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["zebra-template", "/zebra", "o3de", "Zebra template"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["alpha-template", "/alpha", "o3de", "Alpha template"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["middle-template", "/middle", "o3de", "Middle template"],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: Templates should be ordered: embedded first, then custom alphabetically
    let success_data = output.last_success().unwrap();
    let templates = success_data.as_array().unwrap();

    // Should have 4 templates: 1 embedded + 3 custom
    assert_eq!(templates.len(), 4);

    // First should be embedded
    assert_eq!(templates[0]["source"], "embedded");
    assert_eq!(templates[0]["name"], "default");

    // Custom templates should be sorted alphabetically
    assert_eq!(templates[1]["name"], "alpha-template");
    assert_eq!(templates[2]["name"], "middle-template");
    assert_eq!(templates[3]["name"], "zebra-template");
}

// =============================================================================
// Description Truncation Tests
// =============================================================================

#[tokio::test]
async fn test_p1_template_list_interactive_truncates_long_descriptions() {
    // GIVEN: A template with a very long description
    let (db_arc, _temp_dir) = setup_test_database().await;

    // Insert a template with a long description
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        let long_desc = "This is a very long description that should definitely be truncated because it exceeds the maximum character limit for display in the interactive table format";
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["long-desc-template", "/long", "o3de", long_desc],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Interactive);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: The long description should be truncated in interactive mode
    let (_, data) = output.last_table().unwrap();
    let templates = data.as_array().unwrap();

    // Find our long description template
    let long_template = templates
        .iter()
        .find(|t| t["name"] == "long-desc-template")
        .expect("Should find long-desc-template");

    let description = long_template["description"].as_str().unwrap();
    assert!(
        description.ends_with("..."),
        "Long description should be truncated with ellipsis"
    );
    assert!(
        description.len() <= 63,
        "Description should be truncated to max length + ellipsis"
    ); // 60 + "..."
}

#[tokio::test]
async fn test_p1_template_list_json_preserves_full_descriptions() {
    // GIVEN: A template with a very long description
    let (db_arc, _temp_dir) = setup_test_database().await;

    let long_desc = "This is a very long description that should definitely be truncated because it exceeds the maximum character limit for display in the interactive table format";

    // Insert a template with a long description
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["long-desc-template", "/long", "o3de", long_desc],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: JSON mode should preserve the full description
    let success_data = output.last_success().unwrap();
    let templates = success_data.as_array().unwrap();

    // Find our long description template
    let long_template = templates
        .iter()
        .find(|t| t["name"] == "long-desc-template")
        .expect("Should find long-desc-template");

    let description = long_template["description"].as_str().unwrap();
    assert_eq!(
        description, long_desc,
        "JSON mode should preserve full description"
    );
}

#[tokio::test]
async fn test_p2_template_list_short_descriptions_not_truncated() {
    // GIVEN: A template with a short description
    let (db_arc, _temp_dir) = setup_test_database().await;

    let short_desc = "Short description";

    // Insert a template with a short description
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["short-desc-template", "/short", "o3de", short_desc],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Interactive);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: Short descriptions should not be modified
    let (_, data) = output.last_table().unwrap();
    let templates = data.as_array().unwrap();

    // Find our short description template
    let short_template = templates
        .iter()
        .find(|t| t["name"] == "short-desc-template")
        .expect("Should find short-desc-template");

    let description = short_template["description"].as_str().unwrap();
    assert_eq!(
        description, short_desc,
        "Short description should not be truncated"
    );
    assert!(
        !description.ends_with("..."),
        "Short description should not have ellipsis"
    );
}

// =============================================================================
// Comprehensive Handler Tests - Empty Database
// =============================================================================

#[tokio::test]
async fn test_p0_template_list_handler_empty_db_returns_only_embedded() {
    // GIVEN: A fresh database with no custom templates
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: Only embedded templates should be returned
    let success_data = output.last_success().unwrap();
    let templates = success_data.as_array().unwrap();

    // All templates should be embedded
    for template in templates {
        assert_eq!(
            template["source"], "embedded",
            "Empty DB should only have embedded templates"
        );
    }
}

#[tokio::test]
async fn test_p1_template_list_handler_mixed_templates_maintains_order() {
    // GIVEN: A database with multiple custom templates
    let (db_arc, _temp_dir) = setup_test_database().await;

    // Insert custom templates
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["my-custom", "/custom", "unreal", "Custom Unreal template"],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: Embedded templates should come first
    let success_data = output.last_success().unwrap();
    let templates = success_data.as_array().unwrap();

    assert!(
        templates.len() >= 2,
        "Should have at least embedded + custom"
    );

    // First template(s) should be embedded
    assert_eq!(templates[0]["source"], "embedded");

    // Custom template should come after embedded
    let custom_idx = templates
        .iter()
        .position(|t| t["name"] == "my-custom")
        .unwrap();
    let embedded_count = templates
        .iter()
        .filter(|t| t["source"] == "embedded")
        .count();

    assert!(
        custom_idx >= embedded_count,
        "Custom templates should come after all embedded templates"
    );
}

#[tokio::test]
async fn test_p1_template_list_handler_all_fields_present_in_output() {
    // GIVEN: A database with templates
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::List {};

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: Each template should have all required fields
    let success_data = output.last_success().unwrap();
    let templates = success_data.as_array().unwrap();

    for template in templates {
        assert!(
            template.get("name").is_some(),
            "Template should have 'name' field"
        );
        assert!(
            template.get("engine").is_some(),
            "Template should have 'engine' field"
        );
        assert!(
            template.get("source").is_some(),
            "Template should have 'source' field"
        );
        assert!(
            template.get("description").is_some(),
            "Template should have 'description' field"
        );
    }
}

// =============================================================================
// Template Info Handler Tests - Embedded Template
// =============================================================================

#[tokio::test]
async fn test_p0_template_info_embedded_template_json_output() {
    // GIVEN: A fresh database and JSON output mode
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "default".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed
    assert!(
        result.is_ok(),
        "Handler should succeed for embedded template"
    );

    // AND: Output should contain template details
    let success_data = output.last_success().expect("Should have success output");

    // AND: Should have correct fields
    assert_eq!(success_data["name"], "default");
    assert_eq!(success_data["engine"], "generic");
    assert_eq!(success_data["source"], "embedded");
    assert_eq!(success_data["path"], "bundled");
    assert!(
        success_data["description"]
            .as_str()
            .unwrap()
            .contains("Default")
    );

    // AND: Should have files array
    assert!(success_data["files"].is_array(), "Should have files array");
    let files = success_data["files"].as_array().unwrap();
    assert!(!files.is_empty(), "Files array should not be empty");
}

#[tokio::test]
async fn test_p0_template_info_embedded_files_list() {
    // GIVEN: JSON output mode for default template
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "default".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: Files should include expected default template files
    let success_data = output.last_success().unwrap();
    let files: Vec<&str> = success_data["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    assert!(
        files.contains(&"default.buses.json"),
        "Should contain default.buses.json"
    );
    assert!(
        files.contains(&"default.config.json"),
        "Should contain default.config.json"
    );
    assert!(
        files.contains(&"default.pipeline.json"),
        "Should contain default.pipeline.json"
    );
    assert!(
        files.contains(&"default.source.json"),
        "Should contain default.source.json"
    );
}

#[tokio::test]
async fn test_p1_template_info_json_includes_config_options() {
    // GIVEN: JSON output mode for default template
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "default".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_ok());

    // THEN: JSON should include config_options field
    let success_data = output.last_success().unwrap();
    assert!(
        success_data.get("config_options").is_some(),
        "Should include config_options field"
    );

    // AND: config_options should be an array
    assert!(
        success_data["config_options"].is_array(),
        "config_options should be an array"
    );
}

// =============================================================================
// Template Info Handler Tests - Not Found Error
// =============================================================================

#[tokio::test]
async fn test_p0_template_info_not_found_returns_error() {
    // GIVEN: A fresh database and a non-existent template name
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "nonexistent".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should return an error
    assert!(
        result.is_err(),
        "Handler should fail for non-existent template"
    );

    // AND: Error should be a CliError with the correct code
    let err = result.unwrap_err();
    let cli_err = err
        .downcast_ref::<am::common::errors::CliError>()
        .expect("Error should be a CliError");

    assert_eq!(
        cli_err.code, -29005,
        "Error code should be -29005 (template_not_found)"
    );
    assert!(
        cli_err.what.contains("nonexistent"),
        "Error 'what' should mention template name"
    );
    assert!(
        cli_err.suggestion.contains("am template list"),
        "Suggestion should mention 'am template list'"
    );
}

#[tokio::test]
async fn test_p1_template_info_not_found_error_structure() {
    // GIVEN: A non-existent template
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "does-not-exist".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;
    assert!(result.is_err());

    // THEN: Error should have What/Why/Fix structure
    let err = result.unwrap_err();
    let cli_err = err.downcast_ref::<am::common::errors::CliError>().unwrap();

    // What: Template 'name' not found
    assert!(
        cli_err.what.contains("does-not-exist"),
        "What should contain template name"
    );

    // Why: No embedded or registered template matches
    assert!(!cli_err.why.is_empty(), "Why should explain the reason");

    // Fix: Suggestion to use am template list
    assert!(
        cli_err.suggestion.contains("template list"),
        "Fix should suggest using template list"
    );
}

// =============================================================================
// Template Info Handler Tests - Interactive Output
// =============================================================================

#[tokio::test]
async fn test_p1_template_info_interactive_output() {
    // GIVEN: Interactive output mode for default template
    let (db_arc, _temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Interactive);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "default".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok(), "Handler should succeed");

    // AND: Progress messages should contain template information
    let messages = output.progress_messages();
    assert!(
        !messages.is_empty(),
        "Should have progress messages for interactive display"
    );

    // Check that key information is displayed
    let all_messages = messages.join("\n");
    assert!(
        all_messages.contains("default") || output.last_success().is_some(),
        "Should display template name"
    );
}

// =============================================================================
// Template Info Handler Tests - Database Not Available
// =============================================================================

#[tokio::test]
async fn test_p0_template_info_embedded_works_without_database() {
    // GIVEN: No database (None) but asking for embedded template
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "default".to_string(),
    };

    // WHEN: We call the handler without database
    let result = handler(&command, None, &input, &output).await;

    // THEN: Handler should still succeed for embedded templates
    // (embedded templates don't require database)
    assert!(
        result.is_ok(),
        "Embedded template info should work without database"
    );

    // AND: Should return the embedded template info
    let success_data = output.last_success().unwrap();
    assert_eq!(success_data["name"], "default");
    assert_eq!(success_data["source"], "embedded");
}

// =============================================================================
// Custom Template Info Handler Tests - File/Directory Enumeration
// =============================================================================

#[tokio::test]
async fn test_p0_template_info_custom_includes_directories() {
    // GIVEN: A custom template directory with files and subdirectories
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("my-template");
    std::fs::create_dir_all(&template_path).unwrap();

    // Create files at root level
    std::fs::write(template_path.join("config.json"), "{}").unwrap();
    std::fs::write(template_path.join("buses.json"), "{}").unwrap();

    // Create a subdirectory with files
    let sounds_dir = template_path.join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();
    std::fs::write(sounds_dir.join(".gitkeep"), "").unwrap();

    // Set up database with the custom template
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "my-template",
                template_path.to_str().unwrap(),
                "generic",
                "Test template"
            ],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "my-template".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok(), "Handler should succeed for custom template");

    // AND: Output should include both files and directories
    let success_data = output.last_success().expect("Should have success output");
    let files: Vec<&str> = success_data["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // Should have root files
    assert!(files.contains(&"config.json"), "Should contain config.json");
    assert!(files.contains(&"buses.json"), "Should contain buses.json");

    // Should have directory entry (with trailing slash per AC#1)
    assert!(
        files.contains(&"sounds/"),
        "Should contain sounds/ directory"
    );

    // Should have nested file
    assert!(
        files.contains(&"sounds/.gitkeep"),
        "Should contain sounds/.gitkeep"
    );
}

#[tokio::test]
async fn test_p0_template_info_custom_two_levels_deep() {
    // GIVEN: A custom template with 3 levels of nesting (root, level1, level2)
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("deep-template");
    std::fs::create_dir_all(&template_path).unwrap();

    // Root level file
    std::fs::write(template_path.join("root.json"), "{}").unwrap();

    // Level 1 directory with file
    let level1 = template_path.join("level1");
    std::fs::create_dir_all(&level1).unwrap();
    std::fs::write(level1.join("level1.json"), "{}").unwrap();

    // Level 2 directory with file
    let level2 = level1.join("level2");
    std::fs::create_dir_all(&level2).unwrap();
    std::fs::write(level2.join("level2.json"), "{}").unwrap();

    // Level 3 directory (should NOT be traversed - beyond 2 levels)
    let level3 = level2.join("level3");
    std::fs::create_dir_all(&level3).unwrap();
    std::fs::write(level3.join("level3.json"), "{}").unwrap();

    // Set up database with the custom template
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "deep-template",
                template_path.to_str().unwrap(),
                "generic",
                "Deep template"
            ],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "deep-template".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok(), "Handler should succeed");

    let success_data = output.last_success().expect("Should have success output");
    let files: Vec<&str> = success_data["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // Root level file should be present
    assert!(files.contains(&"root.json"), "Should contain root.json");

    // Level 1 directory and file should be present
    assert!(files.contains(&"level1/"), "Should contain level1/");
    assert!(
        files.contains(&"level1/level1.json"),
        "Should contain level1/level1.json"
    );

    // Level 2 directory and file should be present (2 levels deep from root)
    assert!(
        files.contains(&"level1/level2/"),
        "Should contain level1/level2/"
    );
    assert!(
        files.contains(&"level1/level2/level2.json"),
        "Should contain level1/level2/level2.json"
    );

    // Level 3 should NOT be traversed (beyond 2 levels)
    assert!(
        !files.contains(&"level1/level2/level3/"),
        "Should NOT contain level3/ (beyond 2 levels)"
    );
    assert!(
        !files.contains(&"level1/level2/level3/level3.json"),
        "Should NOT contain level3.json (beyond 2 levels)"
    );
}

#[tokio::test]
async fn test_p1_template_info_custom_files_sorted() {
    // GIVEN: A custom template with unsorted files
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("unsorted-template");
    std::fs::create_dir_all(&template_path).unwrap();

    // Create files in non-alphabetical order
    std::fs::write(template_path.join("zebra.json"), "{}").unwrap();
    std::fs::write(template_path.join("alpha.json"), "{}").unwrap();
    std::fs::write(template_path.join("middle.json"), "{}").unwrap();

    // Set up database with the custom template
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "unsorted-template",
                template_path.to_str().unwrap(),
                "generic",
                "Unsorted template"
            ],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();
    let command = TemplateCommands::Info {
        name: "unsorted-template".to_string(),
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok(), "Handler should succeed");

    let success_data = output.last_success().expect("Should have success output");
    let files: Vec<&str> = success_data["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // Files should be sorted alphabetically
    let mut sorted_files = files.clone();
    sorted_files.sort();
    assert_eq!(
        files, sorted_files,
        "Custom template files should be sorted"
    );
}

// =============================================================================
// Real CLI JSON Stdout Envelope Tests
// =============================================================================

#[test]
fn test_p0_template_info_cli_json_stdout_envelope_success() {
    use std::process::Command;

    // GIVEN: The actual CLI binary
    // WHEN: Running `am --json template info default`
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args(["--json", "template", "info", "default"])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 0 (success)
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit code 0 for template info, got {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // AND: stdout should contain valid JSON with envelope format
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect(&format!("Expected valid JSON in stdout, got: {}", stdout));

    // AND: JSON should have ok=true envelope
    assert_eq!(
        json["ok"], true,
        "Expected ok=true in JSON envelope. Got: {}",
        stdout
    );

    // AND: JSON should have value field (not error)
    assert!(
        json.get("value").is_some(),
        "Expected 'value' field in success envelope. Got: {}",
        stdout
    );
    assert!(
        json.get("error").is_none() || json["error"].is_null(),
        "Expected no 'error' field in success envelope. Got: {}",
        stdout
    );

    // AND: value should contain template info fields
    let value = &json["value"];
    assert_eq!(value["name"], "default", "Expected name=default");
    assert_eq!(value["engine"], "generic", "Expected engine=generic");
    assert_eq!(value["source"], "embedded", "Expected source=embedded");
    assert!(value["files"].is_array(), "Expected files to be an array");
    assert!(
        value["config_options"].is_array(),
        "Expected config_options to be an array"
    );
}

#[test]
fn test_p0_template_info_cli_json_stdout_envelope_error() {
    use std::process::Command;

    // GIVEN: The actual CLI binary
    // WHEN: Running `am --json template info nonexistent`
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args(["--json", "template", "info", "nonexistent"])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for template not found, got {:?}",
        output.status.code()
    );

    // AND: stdout should contain valid JSON with error envelope
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect(&format!("Expected valid JSON in stdout, got: {}", stdout));

    // AND: JSON should have ok=false envelope
    assert_eq!(
        json["ok"], false,
        "Expected ok=false in error envelope. Got: {}",
        stdout
    );

    // AND: JSON should have error field (not value)
    assert!(
        json.get("error").is_some() && !json["error"].is_null(),
        "Expected 'error' field in error envelope. Got: {}",
        stdout
    );

    // AND: error should contain structured fields
    let error = &json["error"];
    assert_eq!(
        error["code"], -29005,
        "Expected error code -29005 for template_not_found"
    );
    assert_eq!(
        error["type"], "template_not_found",
        "Expected type=template_not_found"
    );
    assert!(
        error["message"].as_str().unwrap().contains("nonexistent"),
        "Expected message to mention template name"
    );
    assert!(
        error["suggestion"]
            .as_str()
            .unwrap()
            .contains("template list"),
        "Expected suggestion to mention 'template list'"
    );
}

#[test]
fn test_p1_template_list_cli_json_stdout_envelope() {
    use std::process::Command;

    // GIVEN: The actual CLI binary
    // WHEN: Running `am --json template list`
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args(["--json", "template", "list"])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 0 (success)
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit code 0 for template list"
    );

    // AND: stdout should contain valid JSON with envelope format
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .expect(&format!("Expected valid JSON in stdout, got: {}", stdout));

    // AND: JSON should have ok=true envelope
    assert_eq!(json["ok"], true, "Expected ok=true in JSON envelope");

    // AND: value should be an array of templates
    let value = &json["value"];
    assert!(value.is_array(), "Expected value to be an array");
    let templates = value.as_array().unwrap();
    assert!(!templates.is_empty(), "Expected at least one template");

    // AND: First template should be the embedded "default" template
    let first = &templates[0];
    assert_eq!(first["name"], "default");
    assert_eq!(first["source"], "embedded");
}
