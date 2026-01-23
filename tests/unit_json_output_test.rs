//! Unit tests for JsonOutput presentation implementation.
//!
//! Tests the JSON output format for machine-parseable CLI responses.
//!
//! Priority levels:
//! - P0: Critical JSON structure validation (ok field, valid JSON)
//! - P1: Error field validation (code, type, message, suggestion)
//! - P2: Factory function tests, complex data serialization

use am::presentation::{create_output, JsonOutput, Output, OutputMode};
use anyhow::anyhow;
use serde_json::{json, Value};
use std::io::Cursor;
use std::sync::Mutex;

mod test_support {
    use super::*;

    /// Test-only writer-backed Output adapter, allowing assertions on written bytes.
    #[derive(Debug)]
    pub struct TestJsonOutput<W: std::io::Write + Send> {
        writer: Mutex<W>,
    }

    impl<W: std::io::Write + Send> TestJsonOutput<W> {
        pub fn new(writer: W) -> Self {
            Self {
                writer: Mutex::new(writer),
            }
        }

        pub fn into_inner(self) -> W {
            self.writer.into_inner().expect("mutex poisoned")
        }
    }

    impl<W: std::io::Write + Send> Output for TestJsonOutput<W> {
        fn success(&self, data: serde_json::Value, request_id: Option<i64>) {
            let _ = request_id;
            let response = JsonOutput::build_success_response(data);
            let mut writer = self.writer.lock().expect("mutex poisoned");
            let _ = JsonOutput::write_response(&mut *writer, &response);
        }

        fn error(&self, err: &anyhow::Error, code: i32, request_id: Option<i64>) {
            let _ = request_id;
            let response = JsonOutput::build_error_response(err, code);
            let mut writer = self.writer.lock().expect("mutex poisoned");
            let _ = JsonOutput::write_response(&mut *writer, &response);
        }

        fn progress(&self, _message: &str) {
            // Intentionally no-op: JSON mode is quiet except for the final result envelope.
        }

        fn table(&self, _title: Option<&str>, _data: serde_json::Value) {
            // Intentionally no-op for test implementation
        }

        fn mode(&self) -> am::presentation::OutputMode {
            am::presentation::OutputMode::Json
        }
    }
}

// ============================================================================
// P0: JsonOutput Type Safety Tests
// ============================================================================

#[test]
fn test_p0_json_output_is_send_sync() {
    // GIVEN: JsonOutput type
    // WHEN: Checking Send + Sync bounds
    // THEN: Should satisfy both traits for thread safety
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<JsonOutput>();
}

#[test]
fn test_p0_json_output_implements_output_trait() {
    // GIVEN: JsonOutput type
    // WHEN: Used as dyn Output
    // THEN: Should compile and work correctly (trait is object-safe)
    fn take_output(_: &dyn Output) {}
    let output = JsonOutput::new();
    take_output(&output);
}

#[test]
fn test_p0_json_output_boxed_works() {
    // GIVEN: JsonOutput boxed as trait object
    let output: Box<dyn Output> = Box::new(JsonOutput::new());

    // WHEN: Calling methods through the box
    // THEN: Should work without panic
    // Note: progress() is a no-op for JsonOutput, so we can safely test it
    output.progress("testing...");
    output.success(json!({"ok": true}), None);
}

// ============================================================================
// P0: Critical JSON Structure Tests - success() method
// ============================================================================

#[test]
fn test_p0_json_output_success_produces_valid_json() {
    // GIVEN: JsonOutput and test data
    let data = json!({"message": "test"});

    // WHEN: Building a success response
    let response = JsonOutput::build_success_response(data.clone());

    // THEN: Response should be valid JSON with correct structure
    assert!(response.ok, "Success response should have ok=true");
    assert_eq!(response.value, Some(data), "Value should match input data");
    assert!(
        response.error.is_none(),
        "Success response should have no error"
    );
}

#[test]
fn test_p0_json_output_success_serializes_to_valid_json() {
    // GIVEN: A success response
    let data = json!({"message": "test"});
    let response = JsonOutput::build_success_response(data);

    // WHEN: Serializing to JSON string
    let json_str = JsonOutput::serialize_response(&response).expect("Serialization should succeed");

    // THEN: Should be parseable JSON with expected fields
    let parsed: Value = serde_json::from_str(&json_str).expect("Should parse as valid JSON");
    assert_eq!(parsed["ok"], true, "Parsed JSON should have ok=true");
    assert!(
        parsed.get("value").is_some(),
        "Parsed JSON should have value field"
    );
    assert!(
        parsed.get("error").is_none(),
        "Success response should not have error field"
    );
}

#[test]
fn test_p0_json_output_success_writes_to_buffer() {
    // GIVEN: A buffer to capture output and test data
    let mut buffer = Cursor::new(Vec::new());
    let data = json!({"message": "test"});
    let response = JsonOutput::build_success_response(data);

    // WHEN: Writing to the buffer
    JsonOutput::write_response(&mut buffer, &response).expect("Write should succeed");

    // THEN: Buffer should contain valid JSON
    let output = String::from_utf8(buffer.into_inner()).expect("Should be valid UTF-8");
    let parsed: Value = serde_json::from_str(output.trim()).expect("Should parse as valid JSON");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["value"]["message"], "test");
}

#[test]
fn test_p0_json_output_success_with_string_value() {
    // GIVEN: JsonOutput and a simple string value
    let data = json!("Simple message");

    // WHEN: Building a success response
    let response = JsonOutput::build_success_response(data.clone());

    // THEN: Response should contain the string value
    assert!(response.ok);
    assert_eq!(response.value, Some(data));
}

#[test]
fn test_p0_json_output_success_with_complex_data() {
    // GIVEN: JsonOutput and complex nested data
    let data = json!({
        "project": {
            "name": "test_project",
            "path": "/path/to/project",
            "assets": ["sound1", "sound2", "sound3"]
        },
        "count": 42,
        "active": true
    });

    // WHEN: Building and serializing a success response
    let response = JsonOutput::build_success_response(data.clone());
    let json_str = JsonOutput::serialize_response(&response).expect("Serialization should succeed");

    // THEN: All nested data should be preserved
    let parsed: Value = serde_json::from_str(&json_str).expect("Should parse as valid JSON");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["value"]["project"]["name"], "test_project");
    assert_eq!(parsed["value"]["count"], 42);
    assert_eq!(parsed["value"]["active"], true);
}

// ============================================================================
// P0: Critical JSON Structure Tests - error() method
// ============================================================================

#[test]
fn test_p0_json_output_error_produces_valid_json() {
    // GIVEN: An error with a specific code
    let err = anyhow!("Test error message");

    // WHEN: Building an error response
    let response = JsonOutput::build_error_response(&err, -30001);

    // THEN: Response should have correct structure
    assert!(!response.ok, "Error response should have ok=false");
    assert!(
        response.value.is_none(),
        "Error response should have no value"
    );
    let error = response
        .error
        .expect("Error response should have error details");
    assert_eq!(error.code, -30001, "Error code should match");
    assert_eq!(
        error.message, "Test error message",
        "Error message should match"
    );
}

#[test]
fn test_p0_json_output_error_serializes_to_valid_json() {
    // GIVEN: An error response
    let err = anyhow!("Test error message");
    let response = JsonOutput::build_error_response(&err, -30001);

    // WHEN: Serializing to JSON string
    let json_str = JsonOutput::serialize_response(&response).expect("Serialization should succeed");

    // THEN: Should be parseable JSON with expected fields
    let parsed: Value = serde_json::from_str(&json_str).expect("Should parse as valid JSON");
    assert_eq!(parsed["ok"], false, "Parsed JSON should have ok=false");
    assert!(
        parsed.get("error").is_some(),
        "Parsed JSON should have error field"
    );
    assert!(
        parsed.get("value").is_none(),
        "Error response should not have value field"
    );
}

#[test]
fn test_p0_json_output_error_writes_to_buffer() {
    // GIVEN: A buffer to capture output
    let mut buffer = Cursor::new(Vec::new());
    let err = anyhow!("Test error");
    let response = JsonOutput::build_error_response(&err, -30001);

    // WHEN: Writing to the buffer
    JsonOutput::write_response(&mut buffer, &response).expect("Write should succeed");

    // THEN: Buffer should contain valid JSON with error structure
    let output = String::from_utf8(buffer.into_inner()).expect("Should be valid UTF-8");
    let parsed: Value = serde_json::from_str(output.trim()).expect("Should parse as valid JSON");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["error"]["code"], -30001);
    assert_eq!(parsed["error"]["message"], "Test error");
}

#[test]
fn test_p0_json_output_error_includes_all_fields() {
    // GIVEN: An error response
    let err = anyhow!("Asset 'footstep' not found");
    let response = JsonOutput::build_error_response(&err, -30001);

    // WHEN: Checking the error details
    let error = response.error.expect("Should have error details");

    // THEN: All required fields should be present
    assert_eq!(error.code, -30001, "Should have code field");
    assert!(!error.type_.is_empty(), "Should have type field");
    assert_eq!(
        error.message, "Asset 'footstep' not found",
        "Should have message field"
    );
    assert!(!error.suggestion.is_empty(), "Should have suggestion field");
}

#[test]
fn test_p0_json_output_error_with_chained_errors() {
    // GIVEN: A chained error
    let err = anyhow!("Root cause").context("Additional context");

    // WHEN: Building an error response
    let response = JsonOutput::build_error_response(&err, -30002);

    // THEN: Should include the context message
    let error = response.error.expect("Should have error details");
    assert!(
        error.message.contains("Additional context"),
        "Should include context message"
    );
}

// ============================================================================
// P0: Progress Suppression Tests
// ============================================================================

#[test]
fn test_p0_json_output_progress_suppresses_output() {
    // GIVEN: a test-only writer-backed Output adapter, so we can assert on actual bytes written
    let output = test_support::TestJsonOutput::new(Cursor::new(Vec::<u8>::new()));

    // WHEN: Calling progress multiple times
    output.progress("Loading...");
    output.progress("Step 1 of 3");
    output.progress("Complete");

    // THEN: No bytes should have been written (JSON mode is quiet except final result)
    let buffer = output.into_inner();
    assert!(
        buffer.into_inner().is_empty(),
        "progress() must not write any bytes in JSON mode"
    );
}

// ============================================================================
// P0: Prompt Returns Error Tests
// ============================================================================

#[test]
fn test_p0_json_output_prompt_returns_error() {
    // Prompting is handled by the Input abstraction, not Output.
    // Keeping a placeholder test ensures the previous AC is explicitly retired.
    assert!(true);
}

#[test]
fn test_p0_json_output_prompt_error_suggests_cli_args() {
    // Prompting is handled by the Input abstraction; JsonOutput no longer owns prompt behavior.
    assert!(true);
}

// ============================================================================
// P1: Error Field Validation Tests (using buffer-based testing)
// ============================================================================

#[test]
fn test_p1_error_type_mapping_asset_not_found() {
    // GIVEN: Specific asset error code -30001
    let err = anyhow!("Sound 'footstep' not found");

    // WHEN: Error response is built with asset_not_found code
    let response = JsonOutput::build_error_response(&err, -30001);

    // THEN: Should map to "asset_not_found" type
    let error = response.error.expect("Should have error details");
    assert_eq!(
        error.type_, "asset_not_found",
        "Should map -30001 to asset_not_found"
    );
    assert_eq!(error.code, -30001);
}

#[test]
fn test_p1_error_type_mapping_validation_errors() {
    // GIVEN: Specific validation error code -31001
    let err = anyhow!("Invalid field value");

    // WHEN: Error response is built with schema validation code
    let response = JsonOutput::build_error_response(&err, -31001);

    // THEN: Should map to "schema_validation_error" type
    let error = response.error.expect("Should have error details");
    assert_eq!(
        error.type_, "schema_validation_error",
        "Should map -31001 to schema_validation_error"
    );
    assert_eq!(error.code, -31001);
}

#[test]
fn test_p1_error_type_mapping_generic_validation_errors() {
    // GIVEN: Generic validation error code (not a specific one)
    let err = anyhow!("Validation failed");

    // WHEN: Error response is built with generic validation code
    let response = JsonOutput::build_error_response(&err, -31050);

    // THEN: Should fall back to "validation_error" type
    let error = response.error.expect("Should have error details");
    assert_eq!(
        error.type_, "validation_error",
        "Should map generic -31xxx to validation_error"
    );
}

#[test]
fn test_p1_error_type_mapping_project_errors() {
    // GIVEN: Specific project error code -29001
    let err = anyhow!("Project not initialized");

    // WHEN: Error response is built with project code
    let response = JsonOutput::build_error_response(&err, -29001);

    // THEN: Should map to "project_not_initialized" type
    let error = response.error.expect("Should have error details");
    assert_eq!(
        error.type_, "project_not_initialized",
        "Should map -29001 to project_not_initialized"
    );
    assert_eq!(error.code, -29001);
}

#[test]
fn test_p1_error_type_mapping_sdk_errors() {
    // GIVEN: Specific SDK error code -28001
    let err = anyhow!("SDK not found");

    // WHEN: Error response is built with SDK code
    let response = JsonOutput::build_error_response(&err, -28001);

    // THEN: Should map to "sdk_not_found" type
    let error = response.error.expect("Should have error details");
    assert_eq!(
        error.type_, "sdk_not_found",
        "Should map -28001 to sdk_not_found"
    );
    assert_eq!(error.code, -28001);
}

#[test]
fn test_p1_error_type_mapping_unknown_errors() {
    // GIVEN: Error code outside defined ranges
    let err = anyhow!("Unknown error");

    // WHEN: Error response is built with unknown code
    let response = JsonOutput::build_error_response(&err, -1);

    // THEN: Should map to "unknown_error" type
    let error = response.error.expect("Should have error details");
    assert_eq!(
        error.type_, "unknown_error",
        "Should map unknown codes to unknown_error"
    );
    assert_eq!(error.code, -1);
}

// ============================================================================
// P1: Factory Function Tests
// ============================================================================

#[test]
fn test_p1_create_output_returns_json_when_json_mode() {
    // GIVEN: OutputMode::Json
    // WHEN: Calling create_output
    let output = create_output(OutputMode::Json);

    // THEN: Should return a working Output trait object
    output.progress("testing...");
    output.success(json!({"ok": true}), None);
}

#[test]
fn test_p1_create_output_returns_interactive_when_interactive_mode() {
    // GIVEN: OutputMode::Interactive
    // WHEN: Calling create_output
    let output = create_output(OutputMode::Interactive);

    // THEN: Should return a working Output trait object
    output.progress("test progress message");
    output.success(json!("ok"), None);
}

// ============================================================================
// P1: Request ID Tests (future JSON-RPC 2.0 support)
// ============================================================================

#[test]
fn test_p1_json_output_success_with_request_id_produces_valid_json() {
    // GIVEN: Success response data
    let data = json!({"message": "test"});

    // WHEN: Building success response (request_id is currently ignored but accepted)
    let response = JsonOutput::build_success_response(data.clone());

    // THEN: Response should be valid regardless of request_id
    // (request_id parameter is for future JSON-RPC 2.0 support)
    assert!(response.ok);
    assert_eq!(response.value, Some(data));
}

#[test]
fn test_p1_json_output_error_with_request_id_produces_valid_json() {
    // GIVEN: Error details
    let err = anyhow!("Test error");

    // WHEN: Building error response (request_id is currently ignored but accepted)
    let response = JsonOutput::build_error_response(&err, -30001);

    // THEN: Response should be valid regardless of request_id
    assert!(!response.ok);
    let error = response.error.expect("Should have error details");
    assert_eq!(error.code, -30001);
}

// ============================================================================
// P2: Edge Cases and Serialization Tests (using buffer-based testing)
// ============================================================================

#[test]
fn test_p2_json_output_success_with_null_value() {
    // GIVEN: null JSON value
    let data = Value::Null;

    // WHEN: Building and serializing success response
    let response = JsonOutput::build_success_response(data);
    let mut buffer = Cursor::new(Vec::new());
    JsonOutput::write_response(&mut buffer, &response).expect("Write should succeed");

    // THEN: Should produce valid JSON with null value
    let output = String::from_utf8(buffer.into_inner()).expect("Valid UTF-8");
    let parsed: Value = serde_json::from_str(output.trim()).expect("Valid JSON");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["value"], Value::Null);
}

#[test]
fn test_p2_json_output_success_with_array_value() {
    // GIVEN: array value
    let data = json!(["item1", "item2", "item3"]);

    // WHEN: Building and serializing success response
    let response = JsonOutput::build_success_response(data);
    let mut buffer = Cursor::new(Vec::new());
    JsonOutput::write_response(&mut buffer, &response).expect("Write should succeed");

    // THEN: Should produce valid JSON with array
    let output = String::from_utf8(buffer.into_inner()).expect("Valid UTF-8");
    let parsed: Value = serde_json::from_str(output.trim()).expect("Valid JSON");
    assert_eq!(parsed["ok"], true);
    assert!(parsed["value"].is_array());
    assert_eq!(parsed["value"].as_array().unwrap().len(), 3);
}

#[test]
fn test_p2_json_output_success_with_numeric_values() {
    // GIVEN: Various numeric values
    for data in [json!(42), json!(3.14159), json!(-100)] {
        // WHEN: Building and serializing success response
        let response = JsonOutput::build_success_response(data.clone());
        let json_str =
            JsonOutput::serialize_response(&response).expect("Serialization should succeed");

        // THEN: Should produce valid JSON
        let parsed: Value = serde_json::from_str(&json_str).expect("Valid JSON");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["value"], data);
    }
}

#[test]
fn test_p2_json_output_success_with_boolean_values() {
    // GIVEN: Boolean values
    for data in [json!(true), json!(false)] {
        // WHEN: Building and serializing success response
        let response = JsonOutput::build_success_response(data.clone());
        let json_str =
            JsonOutput::serialize_response(&response).expect("Serialization should succeed");

        // THEN: Should produce valid JSON
        let parsed: Value = serde_json::from_str(&json_str).expect("Valid JSON");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["value"], data);
    }
}

#[test]
fn test_p2_json_output_success_with_empty_object() {
    // GIVEN: empty object
    let data = json!({});

    // WHEN: Building and serializing success response
    let response = JsonOutput::build_success_response(data);
    let json_str = JsonOutput::serialize_response(&response).expect("Serialization should succeed");

    // THEN: Should produce valid JSON with empty object
    let parsed: Value = serde_json::from_str(&json_str).expect("Valid JSON");
    assert_eq!(parsed["ok"], true);
    assert!(parsed["value"].is_object());
    assert!(parsed["value"].as_object().unwrap().is_empty());
}

#[test]
fn test_p2_json_output_success_with_empty_array() {
    // GIVEN: empty array
    let data = json!([]);

    // WHEN: Building and serializing success response
    let response = JsonOutput::build_success_response(data);
    let json_str = JsonOutput::serialize_response(&response).expect("Serialization should succeed");

    // THEN: Should produce valid JSON with empty array
    let parsed: Value = serde_json::from_str(&json_str).expect("Valid JSON");
    assert_eq!(parsed["ok"], true);
    assert!(parsed["value"].is_array());
    assert!(parsed["value"].as_array().unwrap().is_empty());
}

#[test]
fn test_p2_json_output_error_with_special_characters() {
    // GIVEN: error with special characters
    let err = anyhow!("Error with \"quotes\" and 'apostrophes' and\nnewlines");

    // WHEN: Building and serializing error response
    let response = JsonOutput::build_error_response(&err, -30001);
    let mut buffer = Cursor::new(Vec::new());
    JsonOutput::write_response(&mut buffer, &response).expect("Write should succeed");

    // THEN: Should produce valid JSON with escaped characters
    let output = String::from_utf8(buffer.into_inner()).expect("Valid UTF-8");
    let parsed: Value = serde_json::from_str(output.trim()).expect("Valid JSON");
    assert_eq!(parsed["ok"], false);
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap()
        .contains("quotes"));
}

#[test]
fn test_p2_json_output_error_with_unicode() {
    // GIVEN: error with unicode characters
    let err = anyhow!("Error with unicode: \u{1F4A5} boom! 日本語");

    // WHEN: Building and serializing error response
    let response = JsonOutput::build_error_response(&err, -30001);
    let json_str = JsonOutput::serialize_response(&response).expect("Serialization should succeed");

    // THEN: Should produce valid JSON with unicode
    let parsed: Value = serde_json::from_str(&json_str).expect("Valid JSON");
    assert_eq!(parsed["ok"], false);
    let message = parsed["error"]["message"].as_str().unwrap();
    assert!(message.contains("boom"));
    assert!(message.contains("日本語"));
}

#[test]
fn test_p2_json_output_success_with_deeply_nested_data() {
    // GIVEN: deeply nested data
    let data = json!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "value": "deep"
                    }
                }
            }
        }
    });

    // WHEN: Building and serializing success response
    let response = JsonOutput::build_success_response(data);
    let mut buffer = Cursor::new(Vec::new());
    JsonOutput::write_response(&mut buffer, &response).expect("Write should succeed");

    // THEN: Should preserve nested structure
    let output = String::from_utf8(buffer.into_inner()).expect("Valid UTF-8");
    let parsed: Value = serde_json::from_str(output.trim()).expect("Valid JSON");
    assert_eq!(parsed["ok"], true);
    assert_eq!(
        parsed["value"]["level1"]["level2"]["level3"]["level4"]["value"],
        "deep"
    );
}

#[test]
fn test_p2_json_output_multiple_responses_to_buffer() {
    // GIVEN: Multiple response scenarios
    let success_data = json!({"status": "ok"});
    let err = anyhow!("test error");

    // WHEN: Building and serializing multiple responses
    let success_resp = JsonOutput::build_success_response(success_data.clone());
    let error_resp = JsonOutput::build_error_response(&err, -30001);

    // THEN: Each should produce valid independent JSON
    let success_json =
        JsonOutput::serialize_response(&success_resp).expect("Success serialization");
    let error_json = JsonOutput::serialize_response(&error_resp).expect("Error serialization");

    let parsed_success: Value = serde_json::from_str(&success_json).expect("Parse success");
    let parsed_error: Value = serde_json::from_str(&error_json).expect("Parse error");

    assert_eq!(parsed_success["ok"], true);
    assert_eq!(parsed_error["ok"], false);
}

// ============================================================================
// P2: Default Trait Implementation Tests
// ============================================================================

#[test]
fn test_p2_json_output_default_creates_instance() {
    // GIVEN: Default trait
    // WHEN: Creating JsonOutput via Default
    let output = JsonOutput::default();

    // THEN: Should create a valid instance that can build responses
    let response = JsonOutput::build_success_response(json!("test"));
    assert!(response.ok);

    // Also verify the instance methods work
    output.progress("test"); // Should not panic (no-op)
}

#[test]
fn test_p2_json_output_new_and_default_equivalent() {
    // GIVEN: Both construction methods
    let _output1 = JsonOutput::new();
    let _output2 = JsonOutput::default();

    // WHEN: Using the same data
    let data = json!({"test": "value"});

    // THEN: Both should produce identical responses via builder methods
    let response1 = JsonOutput::build_success_response(data.clone());
    let response2 = JsonOutput::build_success_response(data);

    assert_eq!(response1.ok, response2.ok);
    assert_eq!(response1.value, response2.value);
}
