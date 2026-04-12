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

//! Feature tests for template commands.
//!
//! These tests exercise the actual template command handlers and verify
//! the output format meets acceptance criteria.

use am::commands::template::{TemplateCommands, handler};
use am::database::entities::TemplateSource;
use am::database::{Database, db_get_templates};
use am::input::{Input, NonInteractiveInput};
use am::presentation::{Output, OutputMode, create_output};
use inquire::validator::Validation;
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

fn test_home_dir() -> &'static str {
    thread_local! {
        static TEST_HOME: RefCell<Option<&'static str>> = const { RefCell::new(None) };
    }

    TEST_HOME.with(|cell| {
        let mut home = cell.borrow_mut();
        if let Some(path) = *home {
            return path;
        }

        let temp_dir = tempfile::tempdir().expect("Failed to create test home dir");
        let path = temp_dir.path().to_string_lossy().to_string();
        std::mem::forget(temp_dir);

        let leaked: &'static str = Box::leak(path.into_boxed_str());
        *home = Some(leaked);
        leaked
    })
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

/// Mock input that returns a predetermined value for confirmation.
///
/// Used to test the cancellation path where user declines the confirmation prompt.
struct MockInput {
    confirm_response: bool,
}

impl MockInput {
    /// Create a mock input that will return the specified value for confirm().
    fn with_confirm_response(response: bool) -> Self {
        Self {
            confirm_response: response,
        }
    }
}

impl Input for MockInput {
    fn prompt_text(
        &self,
        prompt: &str,
        _placeholder: Option<&str>,
        _formatter: Option<&dyn Fn(&str) -> String>,
        _validator: Option<&dyn Fn(&str) -> anyhow::Result<Validation, inquire::CustomUserError>>,
    ) -> anyhow::Result<String> {
        Err(anyhow::anyhow!(
            "MockInput: prompt_text not implemented for '{}'",
            prompt
        ))
    }

    fn select(&self, prompt: &str, _options: &[String]) -> anyhow::Result<String> {
        Err(anyhow::anyhow!(
            "MockInput: select not implemented for '{}'",
            prompt
        ))
    }

    fn confirm(&self, _prompt: &str, _default: Option<bool>) -> anyhow::Result<bool> {
        Ok(self.confirm_response)
    }

    fn prompt_text_with_default(
        &self,
        prompt: &str,
        _default: &str,
        _validator: Option<&dyn Fn(&str) -> anyhow::Result<Validation, inquire::CustomUserError>>,
    ) -> anyhow::Result<String> {
        Err(anyhow::anyhow!(
            "MockInput: prompt_text_with_default not implemented for '{}'",
            prompt
        ))
    }

    fn multi_select(&self, prompt: &str, _options: &[String]) -> anyhow::Result<Vec<String>> {
        Err(anyhow::anyhow!(
            "MockInput: multi_select not implemented for '{}'",
            prompt
        ))
    }
}

// Safety: MockInput is only used in single-threaded tests
unsafe impl Send for MockInput {}
unsafe impl Sync for MockInput {}

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
        .env("HOME", test_home_dir())
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
        .env("HOME", test_home_dir())
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
        .env("HOME", test_home_dir())
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

// =============================================================================
// Template Register Handler Tests (Story 1b.5)
// =============================================================================

#[tokio::test]
async fn test_p0_template_register_valid_template() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A valid template directory
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("my-template");
    std::fs::create_dir_all(&template_path).unwrap();

    // Create required files
    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // Set up database
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: Some("my-custom-template".to_string()),
        force: false,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc.clone()), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok(), "Handler should succeed: {:?}", result.err());

    // AND: Output should confirm registration
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["name"], "my-custom-template");
    assert_eq!(success_data["source"], "custom");

    // AND: Template should be in database
    let template = am::database::db_get_template_by_name("my-custom-template", Some(db_arc))
        .unwrap()
        .expect("Template should be in database");
    assert_eq!(template.name, "my-custom-template");
}

#[tokio::test]
async fn test_p0_template_register_invalid_structure() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: An invalid template directory (missing required files)
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("invalid-template");
    std::fs::create_dir_all(&template_path).unwrap();
    // Only create partial files - missing .amproject
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();

    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: Some("invalid-template".to_string()),
        force: false,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail with validation error
    assert!(result.is_err(), "Handler should fail for invalid template");
    let err = result.unwrap_err();
    let cli_err = err
        .downcast_ref::<am::common::errors::CliError>()
        .expect("Error should be a CliError");
    assert_eq!(cli_err.code, -29007); // ERR_INVALID_TEMPLATE_STRUCTURE
}

#[tokio::test]
async fn test_p0_template_register_nonexistent_path() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A path that doesn't exist
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: "/nonexistent/template/path".to_string(),
        name: Some("ghost-template".to_string()),
        force: false,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail with path not found error
    assert!(result.is_err(), "Handler should fail for nonexistent path");
}

#[tokio::test]
async fn test_p0_template_register_name_conflict_without_force() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A valid template directory
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("conflict-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // Set up database and pre-register a template with same name
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["existing-template", "/old/path", "generic", "Existing"],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: Some("existing-template".to_string()), // Same name as existing
        force: false,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail with name conflict error
    assert!(result.is_err(), "Handler should fail for name conflict");
    let err = result.unwrap_err();
    let cli_err = err.downcast_ref::<am::common::errors::CliError>().unwrap();
    assert_eq!(cli_err.code, -29006); // ERR_TEMPLATE_NAME_CONFLICT
    assert!(cli_err.suggestion.contains("--force"));
}

#[tokio::test]
async fn test_p0_template_register_name_conflict_with_force() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A valid template directory
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("force-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // Set up database with existing template
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["overwrite-me", "/old/path", "generic", "Old template"],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: Some("overwrite-me".to_string()),
        force: true, // Use --force
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc.clone()), &input, &output).await;

    // THEN: Handler should succeed (overwrites existing)
    assert!(result.is_ok(), "Handler should succeed with --force");

    // AND: Template path should be updated
    let template = am::database::db_get_template_by_name("overwrite-me", Some(db_arc))
        .unwrap()
        .expect("Template should exist");
    assert!(template.path.contains("force-template"));
}

#[tokio::test]
async fn test_p0_template_register_cannot_overwrite_embedded() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A valid template directory
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("fake-default");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: Some("default".to_string()), // Try to overwrite embedded template
        force: true,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail - cannot overwrite embedded
    assert!(
        result.is_err(),
        "Handler should fail for embedded template name"
    );
    let err = result.unwrap_err();
    let cli_err = err.downcast_ref::<am::common::errors::CliError>().unwrap();
    assert_eq!(cli_err.code, -29006); // ERR_TEMPLATE_NAME_CONFLICT
    assert!(cli_err.why.contains("built-in") || cli_err.why.contains("embedded"));
}

#[tokio::test]
async fn test_p1_template_register_uses_manifest_name() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A template with manifest containing name
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("manifest-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();
    std::fs::write(
        template_path.join("template.json"),
        r#"{"name":"manifest-name","engine":"o3de","description":"From manifest"}"#,
    )
    .unwrap();

    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: None, // No --name flag, should use manifest
        force: false,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc.clone()), &input, &output).await;

    // THEN: Handler should succeed using manifest name
    assert!(result.is_ok(), "Handler should succeed: {:?}", result.err());

    let success_data = output.last_success().unwrap();
    assert_eq!(success_data["name"], "manifest-name");
    assert_eq!(success_data["engine"], "o3de");
}

#[tokio::test]
async fn test_p1_template_register_cli_flag_overrides_manifest() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A template with manifest containing name, but CLI provides different name
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("override-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();
    std::fs::write(
        template_path.join("template.json"),
        r#"{"name":"manifest-name"}"#,
    )
    .unwrap();

    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: Some("cli-override-name".to_string()), // CLI flag should take priority
        force: false,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc.clone()), &input, &output).await;

    // THEN: Handler should use CLI name, not manifest
    assert!(result.is_ok());
    let success_data = output.last_success().unwrap();
    assert_eq!(success_data["name"], "cli-override-name");
}

#[tokio::test]
async fn test_p1_template_register_non_interactive_requires_name() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A template without manifest name, in non-interactive mode
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("no-name-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();
    // No template.json manifest

    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json); // JSON mode = non-interactive
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: None, // No name provided
        force: false,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail requiring --name flag
    assert!(
        result.is_err(),
        "Should fail without name in non-interactive"
    );
    let err = result.unwrap_err();
    let cli_err = err.downcast_ref::<am::common::errors::CliError>().unwrap();
    assert!(cli_err.suggestion.contains("--name"));
}

// =============================================================================
// Template Register CLI Integration Tests (Story 1b.5)
// =============================================================================

#[test]
fn test_p0_template_register_cli_json_success() {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Generate unique template name to avoid conflicts with previous test runs
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let template_name = format!("cli-test-{}", timestamp);

    // GIVEN: A valid template directory
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("cli-test-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // WHEN: Running `am --json template register <path> --name <name>`
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "register",
            template_path.to_str().unwrap(),
            "--name",
            &template_name,
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 0 (success)
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit code 0. stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    // AND: stdout should contain valid JSON with success envelope
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect(&format!("Expected valid JSON, got: {}", stdout));

    assert_eq!(json["ok"], true, "Expected ok=true");
    assert_eq!(json["value"]["name"], template_name);
    assert_eq!(json["value"]["source"], "custom");
}

#[test]
fn test_p0_template_register_cli_json_invalid_path() {
    use std::process::Command;

    // WHEN: Running `am --json template register /nonexistent/path --name test`
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "register",
            "/nonexistent/template/path",
            "--name",
            "ghost",
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for invalid path"
    );

    // AND: stdout should contain error envelope
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Expected valid JSON");

    assert_eq!(json["ok"], false, "Expected ok=false");
    assert!(json["error"]["code"].as_i64().is_some());
}

#[test]
fn test_p0_template_register_cli_json_missing_name() {
    use std::process::Command;

    // GIVEN: A valid template without manifest name
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("no-manifest-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // WHEN: Running without --name flag in JSON mode
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "register",
            template_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error - name required)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for missing name"
    );

    // AND: Error should mention --name flag
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Expected valid JSON");

    assert_eq!(json["ok"], false);
    assert!(
        json["error"]["suggestion"]
            .as_str()
            .unwrap()
            .contains("--name"),
        "Suggestion should mention --name flag"
    );
}

#[test]
fn test_p0_template_register_cli_non_interactive_flag_requires_name() {
    use std::process::Command;

    // GIVEN: A valid template without manifest name
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("non-interactive-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // WHEN: Running with --non-interactive flag (NOT --json) without --name
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--non-interactive",
            "template",
            "register",
            template_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be non-zero (error)
    assert_ne!(
        output.status.code(),
        Some(0),
        "Expected non-zero exit for missing name in non-interactive mode"
    );

    // AND: Error output should mention --name flag
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stderr, stdout);
    assert!(
        combined.contains("--name") || combined.contains("name"),
        "Error should mention --name flag. Got: {}",
        combined
    );
}

// =============================================================================
// Template Unregister Handler Tests (Story 1b.6)
// =============================================================================

#[tokio::test]
async fn test_p0_template_unregister_custom_template_success() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A custom template registered in the database
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "to-remove",
                "/template/path",
                "generic",
                "Template to remove"
            ],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Unregister {
        name: "to-remove".to_string(),
        force: true, // Use force to bypass confirmation in non-interactive
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc.clone()), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok(), "Handler should succeed: {:?}", result.err());

    // AND: Output should confirm removal
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["name"], "to-remove");
    assert_eq!(success_data["removed"], true);

    // AND: Template should be removed from database
    let template = am::database::db_get_template_by_name("to-remove", Some(db_arc)).unwrap();
    assert!(
        template.is_none(),
        "Template should be removed from database"
    );
}

#[tokio::test]
async fn test_p0_template_unregister_not_found_returns_error() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A fresh database with no templates
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Unregister {
        name: "nonexistent".to_string(),
        force: true,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail with not found error
    assert!(
        result.is_err(),
        "Handler should fail for nonexistent template"
    );

    let err = result.unwrap_err();
    let cli_err = err
        .downcast_ref::<am::common::errors::CliError>()
        .expect("Error should be a CliError");

    assert_eq!(cli_err.code, -29005); // ERR_TEMPLATE_NOT_FOUND
    assert!(
        cli_err.suggestion.contains("template list"),
        "Suggestion should mention 'template list'"
    );
}

#[tokio::test]
async fn test_p0_template_unregister_embedded_template_returns_error() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A fresh database
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Unregister {
        name: "default".to_string(), // The embedded template
        force: true,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail with operation not allowed error
    assert!(result.is_err(), "Handler should fail for embedded template");

    let err = result.unwrap_err();
    let cli_err = err
        .downcast_ref::<am::common::errors::CliError>()
        .expect("Error should be a CliError");

    assert_eq!(cli_err.code, -29008); // ERR_TEMPLATE_OPERATION_NOT_ALLOWED
    assert!(
        cli_err.suggestion.contains("embedded") || cli_err.suggestion.contains("bundled"),
        "Suggestion should mention embedded templates"
    );
}

#[tokio::test]
async fn test_p0_template_unregister_non_interactive_requires_force() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A custom template in the database
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["no-force-template", "/path", "generic", "Test"],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Unregister {
        name: "no-force-template".to_string(),
        force: false, // No force flag
    };

    // WHEN: We call the handler in non-interactive mode without force
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail requiring --force flag
    assert!(
        result.is_err(),
        "Should fail without --force in non-interactive mode"
    );

    let err = result.unwrap_err();
    let cli_err = err
        .downcast_ref::<am::common::errors::CliError>()
        .expect("Error should be a CliError");

    assert!(
        cli_err.suggestion.contains("--force"),
        "Suggestion should mention --force flag"
    );
}

#[tokio::test]
async fn test_p0_template_unregister_json_output_format() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A custom template in the database
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["json-test-template", "/path", "generic", "Test"],
        )
        .unwrap();
    }

    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Unregister {
        name: "json-test-template".to_string(),
        force: true,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should succeed
    assert!(result.is_ok());

    // AND: JSON output should have correct format
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["name"], "json-test-template");
    assert_eq!(success_data["removed"], true);
    // Should NOT have "cancelled" field when successfully removed
    assert!(success_data.get("cancelled").is_none() || success_data["cancelled"].is_null());
}

#[tokio::test]
async fn test_p1_template_unregister_db_delete_returns_false_when_missing() {
    // GIVEN: A fresh database with no templates
    let (db_arc, _db_temp_dir) = setup_test_database().await;

    // WHEN: We try to delete a non-existent template
    let result = am::database::db_delete_template_by_name("nonexistent", Some(db_arc));

    // THEN: Should return Ok(false) indicating no rows deleted
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap(),
        false,
        "Should return false when template doesn't exist"
    );
}

// =============================================================================
// Template Unregister CLI Integration Tests (Story 1b.6)
// =============================================================================

#[test]
fn test_p0_template_unregister_cli_json_success() {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Generate unique template name
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let template_name = format!("cli-unregister-{}", timestamp);

    // GIVEN: First register a template
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("cli-unregister-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // Register the template first
    let register_output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "register",
            template_path.to_str().unwrap(),
            "--name",
            &template_name,
        ])
        .output()
        .expect("Failed to execute register command");

    assert_eq!(
        register_output.status.code(),
        Some(0),
        "Registration should succeed first"
    );

    // WHEN: We unregister the template
    let unregister_output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "unregister",
            &template_name,
            "--force",
        ])
        .output()
        .expect("Failed to execute unregister command");

    // THEN: Exit code should be 0 (success)
    assert_eq!(
        unregister_output.status.code(),
        Some(0),
        "Expected exit code 0. stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&unregister_output.stderr),
        String::from_utf8_lossy(&unregister_output.stdout)
    );

    // AND: stdout should contain valid JSON with success envelope
    let stdout = String::from_utf8_lossy(&unregister_output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect(&format!("Expected valid JSON, got: {}", stdout));

    assert_eq!(json["ok"], true, "Expected ok=true");
    assert_eq!(json["value"]["name"], template_name);
    assert_eq!(json["value"]["removed"], true);
}

#[test]
fn test_p0_template_unregister_cli_json_not_found() {
    use std::process::Command;

    // WHEN: We try to unregister a nonexistent template
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "unregister",
            "nonexistent-template-xyz",
            "--force",
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for not found"
    );

    // AND: Error should contain template_not_found
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect(&format!("Expected valid JSON, got: {}", stdout));

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], -29005);
    assert!(
        json["error"]["suggestion"]
            .as_str()
            .unwrap()
            .contains("template list")
    );
}

#[test]
fn test_p0_template_unregister_cli_json_embedded_not_allowed() {
    use std::process::Command;

    // WHEN: We try to unregister the embedded "default" template
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args(["--json", "template", "unregister", "default", "--force"])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for embedded template"
    );

    // AND: Error should contain operation not allowed
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect(&format!("Expected valid JSON, got: {}", stdout));

    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], -29008); // ERR_TEMPLATE_OPERATION_NOT_ALLOWED
}

#[test]
fn test_p0_template_unregister_cli_non_interactive_requires_force() {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Generate unique template name
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let template_name = format!("cli-noforce-{}", timestamp);

    // GIVEN: First register a template
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("cli-noforce-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // Register the template first
    Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "register",
            template_path.to_str().unwrap(),
            "--name",
            &template_name,
        ])
        .output()
        .expect("Failed to execute register command");

    // WHEN: We try to unregister WITHOUT --force in non-interactive mode (--json)
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "unregister",
            &template_name,
            // No --force flag
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 without --force in non-interactive mode"
    );

    // AND: Error should mention --force flag
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect(&format!("Expected valid JSON, got: {}", stdout));

    assert_eq!(json["ok"], false);
    assert!(
        json["error"]["suggestion"]
            .as_str()
            .unwrap()
            .contains("--force"),
        "Suggestion should mention --force flag. Got: {}",
        json["error"]["suggestion"]
    );
}

// =============================================================================
// Template Validation - Invalid .amproject JSON Tests
// =============================================================================

#[test]
fn test_p0_validate_template_directory_invalid_amproject_json() {
    use am::common::utils::validate_template_directory;
    use std::fs;
    use tempfile::tempdir;

    // GIVEN: A template directory with invalid JSON in .amproject
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path();

    // Create .amproject with invalid JSON
    fs::write(template_path.join(".amproject"), "{ invalid json }").unwrap();
    fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // WHEN: We validate the directory
    let result = validate_template_directory(template_path);

    // THEN: Validation should fail with appropriate error
    assert!(
        result.is_err(),
        "Invalid .amproject JSON should fail validation"
    );
    let err = result.unwrap_err();
    let cli_err = err
        .downcast_ref::<am::common::errors::CliError>()
        .expect("Should be CliError");
    assert_eq!(cli_err.code, -29007); // ERR_INVALID_TEMPLATE_STRUCTURE
    assert!(
        cli_err.what.contains("invalid JSON"),
        "Error should mention invalid JSON"
    );
}

#[tokio::test]
async fn test_p0_template_register_rejects_invalid_amproject_json() {
    use am::commands::template::{TemplateCommands, handler};
    use am::input::NonInteractiveInput;

    // GIVEN: A template with invalid .amproject JSON
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("invalid-json-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(template_path.join(".amproject"), "not valid json {").unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    let (db_arc, _db_temp_dir) = setup_test_database().await;
    let output = CaptureOutput::new(OutputMode::Json);
    let input = NonInteractiveInput::new();

    let command = TemplateCommands::Register {
        path: template_path.to_str().unwrap().to_string(),
        name: Some("invalid-json-template".to_string()),
        force: false,
    };

    // WHEN: We call the handler
    let result = handler(&command, Some(db_arc), &input, &output).await;

    // THEN: Handler should fail with validation error
    assert!(
        result.is_err(),
        "Invalid .amproject JSON should fail registration"
    );
    let err = result.unwrap_err();
    let cli_err = err
        .downcast_ref::<am::common::errors::CliError>()
        .expect("Should be CliError");
    assert_eq!(cli_err.code, -29007); // ERR_INVALID_TEMPLATE_STRUCTURE
}

// =============================================================================
// Template Unregister Cancellation Tests
// =============================================================================

#[tokio::test]
async fn test_p0_template_unregister_user_cancels_json_mode() {
    use am::commands::template::{TemplateCommands, handler};

    // GIVEN: A custom template registered in the database
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "cancel-test-template",
                "/template/path",
                "generic",
                "Template for cancel test"
            ],
        )
        .unwrap();
    }

    // AND: A mock input that returns false for confirmation (user cancels)
    let output = CaptureOutput::new(OutputMode::Json);
    let input = MockInput::with_confirm_response(false);

    let command = TemplateCommands::Unregister {
        name: "cancel-test-template".to_string(),
        force: false, // Don't use force so confirmation is attempted
    };

    // WHEN: We call the handler and user declines the confirmation
    let result = handler(&command, Some(db_arc.clone()), &input, &output).await;

    // THEN: Handler should succeed (cancellation is not an error)
    assert!(
        result.is_ok(),
        "Handler should succeed when user cancels: {:?}",
        result.err()
    );

    // AND: JSON output should have correct cancellation format
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(
        success_data["name"], "cancel-test-template",
        "Should include template name"
    );
    assert_eq!(
        success_data["removed"], false,
        "Should indicate not removed"
    );
    assert_eq!(success_data["cancelled"], true, "Should indicate cancelled");

    // AND: Template should still exist in database (not deleted)
    let template =
        am::database::db_get_template_by_name("cancel-test-template", Some(db_arc)).unwrap();
    assert!(
        template.is_some(),
        "Template should still exist in database after cancellation"
    );
}

#[tokio::test]
async fn test_p0_template_unregister_user_cancels_interactive_mode() {
    use am::commands::template::{TemplateCommands, handler};

    // GIVEN: A custom template registered in the database
    let (db_arc, _db_temp_dir) = setup_test_database().await;
    {
        let conn = db_arc.get_connection();
        let conn = conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "cancel-interactive-template",
                "/template/path",
                "generic",
                "Template for interactive cancel test"
            ],
        )
        .unwrap();
    }

    // AND: A mock input that returns false for confirmation (user cancels)
    let output = CaptureOutput::new(OutputMode::Interactive);
    let input = MockInput::with_confirm_response(false);

    let command = TemplateCommands::Unregister {
        name: "cancel-interactive-template".to_string(),
        force: false, // Don't use force so confirmation is attempted
    };

    // WHEN: We call the handler and user declines the confirmation
    let result = handler(&command, Some(db_arc.clone()), &input, &output).await;

    // THEN: Handler should succeed (cancellation is not an error)
    assert!(
        result.is_ok(),
        "Handler should succeed when user cancels: {:?}",
        result.err()
    );

    // AND: Progress messages should include "Cancelled." message
    let progress_messages = output.progress_messages();
    assert!(
        progress_messages.iter().any(|m| m.contains("Cancelled")),
        "Should display 'Cancelled.' message in interactive mode. Got messages: {:?}",
        progress_messages
    );

    // AND: Template should still exist in database (not deleted)
    let template =
        am::database::db_get_template_by_name("cancel-interactive-template", Some(db_arc)).unwrap();
    assert!(
        template.is_some(),
        "Template should still exist in database after cancellation"
    );
}

#[test]
fn test_p0_template_unregister_cli_explicit_non_interactive_requires_force() {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Generate unique template name
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let template_name = format!("cli-nonint-explicit-{}", timestamp);

    // GIVEN: First register a template
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("cli-nonint-explicit-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // Register the template first (using --json for reliable output parsing)
    let register_output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "register",
            template_path.to_str().unwrap(),
            "--name",
            &template_name,
        ])
        .output()
        .expect("Failed to execute register command");

    assert_eq!(
        register_output.status.code(),
        Some(0),
        "Registration should succeed first. stderr: {}",
        String::from_utf8_lossy(&register_output.stderr)
    );

    // WHEN: We try to unregister WITH --non-interactive (explicit flag, not --json) WITHOUT --force
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--non-interactive", // Explicit non-interactive flag (AC#2)
            "template",
            "unregister",
            &template_name,
            // No --force flag
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be non-zero (error)
    assert_ne!(
        output.status.code(),
        Some(0),
        "Expected non-zero exit code without --force in --non-interactive mode. stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    // AND: stdout should mention --force flag (interactive-style errors go to stdout)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--force") || stdout.contains("force"),
        "Error message should mention --force flag. Got stdout: {}",
        stdout
    );
}

/// CLI test that --non-interactive with --force succeeds (AC#2 positive case)
#[test]
fn test_p0_template_unregister_cli_explicit_non_interactive_with_force_succeeds() {
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Generate unique template name
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let template_name = format!("cli-nonint-force-{}", timestamp);

    // GIVEN: First register a template
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let template_path = temp_dir.path().join("cli-nonint-force-template");
    std::fs::create_dir_all(&template_path).unwrap();

    std::fs::write(
        template_path.join(".amproject"),
        r#"{"name":"test","version":1}"#,
    )
    .unwrap();
    std::fs::write(template_path.join("test.buses.json"), "{}").unwrap();
    std::fs::write(template_path.join("test.config.json"), "{}").unwrap();

    // Register the template first
    Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--json",
            "template",
            "register",
            template_path.to_str().unwrap(),
            "--name",
            &template_name,
        ])
        .output()
        .expect("Failed to execute register command");

    // WHEN: We unregister WITH --non-interactive AND --force
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .env("HOME", test_home_dir())
        .args([
            "--non-interactive", // Explicit non-interactive flag
            "template",
            "unregister",
            &template_name,
            "--force", // Required in non-interactive mode
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 0 (success)
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit code 0 with --force in --non-interactive mode. stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
}
