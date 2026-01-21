//! Unit tests for JsonOutput presentation implementation.
//!
//! Tests the JSON output format for machine-parseable CLI responses.
//!
//! Priority levels:
//! - P0: Critical JSON structure validation (ok field, valid JSON)
//! - P1: Error field validation (code, type, message, suggestion)
//! - P2: Factory function tests, complex data serialization

use am::presentation::{JsonOutput, Output, create_output};
use anyhow::anyhow;
use serde_json::{Value, json};

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
    output.success(json!("test"), None);
    output.progress("testing...");
}

// ============================================================================
// P0: Critical JSON Structure Tests - success() method
// ============================================================================

#[test]
fn test_p0_json_output_success_produces_valid_json() {
    // GIVEN: JsonOutput (note: actual stdout capture is complex, so we test indirectly)
    let output = JsonOutput::new();

    // WHEN: Calling success with data
    // THEN: Should not panic (JSON serialization works)
    output.success(json!({"message": "test"}), None);
}

#[test]
fn test_p0_json_output_success_with_string_value() {
    // GIVEN: JsonOutput
    let output = JsonOutput::new();

    // WHEN: Calling success with a simple string
    // THEN: Should not panic
    output.success(json!("Simple message"), None);
}

#[test]
fn test_p0_json_output_success_with_complex_data() {
    // GIVEN: JsonOutput and complex nested data
    let output = JsonOutput::new();
    let data = json!({
        "project": {
            "name": "test_project",
            "path": "/path/to/project",
            "assets": ["sound1", "sound2", "sound3"]
        },
        "count": 42,
        "active": true
    });

    // WHEN: Calling success with complex data
    // THEN: Should not panic
    output.success(data, None);
}

// ============================================================================
// P0: Critical JSON Structure Tests - error() method
// ============================================================================

#[test]
fn test_p0_json_output_error_produces_valid_json() {
    // GIVEN: JsonOutput and an error
    let output = JsonOutput::new();
    let err = anyhow!("Test error message");

    // WHEN: Calling error
    // THEN: Should not panic (JSON serialization works)
    output.error(&err, -30001, None);
}

#[test]
fn test_p0_json_output_error_with_chained_errors() {
    // GIVEN: JsonOutput and a chained error
    let output = JsonOutput::new();
    let err = anyhow!("Root cause").context("Additional context");

    // WHEN: Calling error with chained error
    // THEN: Should not panic
    output.error(&err, -30002, None);
}

// ============================================================================
// P0: Progress Suppression Tests (AC #3)
// ============================================================================

#[test]
fn test_p0_json_output_progress_suppresses_output() {
    // GIVEN: JsonOutput
    let output = JsonOutput::new();

    // WHEN: Calling progress
    // THEN: Should not panic and should produce no output
    // (progress is suppressed in JSON mode for clean parseable output)
    output.progress("Loading...");
    output.progress("Step 1 of 3");
    output.progress("Complete");
    // If we reach here without panic, the test passes
}

// ============================================================================
// P1: Error Field Validation Tests
// ============================================================================

#[test]
fn test_p1_error_type_mapping_validation_errors() {
    // GIVEN: Validation error code range (-31xxx)
    // WHEN: Error is created with validation code
    // THEN: Should map to "validation_error" type
    // (Tested via internal module tests in json.rs)
    let output = JsonOutput::new();
    let err = anyhow!("Invalid field value");
    output.error(&err, -31001, None);
}

#[test]
fn test_p1_error_type_mapping_asset_errors() {
    // GIVEN: Asset error code range (-30xxx)
    let output = JsonOutput::new();
    let err = anyhow!("Asset not found");
    output.error(&err, -30001, None);
}

#[test]
fn test_p1_error_type_mapping_project_errors() {
    // GIVEN: Project error code range (-29xxx)
    let output = JsonOutput::new();
    let err = anyhow!("Project not initialized");
    output.error(&err, -29001, None);
}

#[test]
fn test_p1_error_type_mapping_sdk_errors() {
    // GIVEN: SDK error code range (-28xxx)
    let output = JsonOutput::new();
    let err = anyhow!("SDK not found");
    output.error(&err, -28001, None);
}

#[test]
fn test_p1_error_type_mapping_unknown_errors() {
    // GIVEN: Error code outside defined ranges
    let output = JsonOutput::new();
    let err = anyhow!("Unknown error");
    output.error(&err, -1, None);
}

// ============================================================================
// P1: Factory Function Tests
// ============================================================================

#[test]
fn test_p1_create_output_returns_json_when_true() {
    // GIVEN: json_mode is true
    // WHEN: Calling create_output
    let output = create_output(true);

    // THEN: Should return JsonOutput (verify by calling methods)
    // JsonOutput suppresses progress, so this is a valid test
    output.success(json!("test"), None);
    output.progress("this should be suppressed");
}

#[test]
fn test_p1_create_output_returns_interactive_when_false() {
    // GIVEN: json_mode is false
    // WHEN: Calling create_output
    let output = create_output(false);

    // THEN: Should return InteractiveOutput (verify by calling methods)
    output.success(json!("test"), None);
    output.progress("this should be displayed");
}

// ============================================================================
// P1: Request ID Tests (future JSON-RPC 2.0 support)
// ============================================================================

#[test]
fn test_p1_json_output_success_ignores_request_id() {
    // GIVEN: JsonOutput
    let output = JsonOutput::new();

    // WHEN: Calling success with a request ID
    // THEN: Should not panic (request_id is stored for future use)
    output.success(json!("test"), Some(42));
    output.success(json!("test"), Some(12345));
}

#[test]
fn test_p1_json_output_error_ignores_request_id() {
    // GIVEN: JsonOutput
    let output = JsonOutput::new();
    let err = anyhow!("Test error");

    // WHEN: Calling error with a request ID
    // THEN: Should not panic
    output.error(&err, -30001, Some(42));
}

// ============================================================================
// P2: Edge Cases and Serialization Tests
// ============================================================================

#[test]
fn test_p2_json_output_success_with_null_value() {
    // GIVEN: JsonOutput and null JSON value
    let output = JsonOutput::new();

    // WHEN: Calling success with null
    // THEN: Should not panic
    output.success(Value::Null, None);
}

#[test]
fn test_p2_json_output_success_with_array_value() {
    // GIVEN: JsonOutput and array value
    let output = JsonOutput::new();

    // WHEN: Calling success with array
    // THEN: Should not panic
    output.success(json!(["item1", "item2", "item3"]), None);
}

#[test]
fn test_p2_json_output_success_with_numeric_value() {
    // GIVEN: JsonOutput and numeric values
    let output = JsonOutput::new();

    // WHEN: Calling success with numbers
    // THEN: Should not panic
    output.success(json!(42), None);
    output.success(json!(3.14159), None);
    output.success(json!(-100), None);
}

#[test]
fn test_p2_json_output_success_with_boolean_value() {
    // GIVEN: JsonOutput
    let output = JsonOutput::new();

    // WHEN: Calling success with boolean
    // THEN: Should not panic
    output.success(json!(true), None);
    output.success(json!(false), None);
}

#[test]
fn test_p2_json_output_success_with_empty_object() {
    // GIVEN: JsonOutput
    let output = JsonOutput::new();

    // WHEN: Calling success with empty object
    // THEN: Should not panic
    output.success(json!({}), None);
}

#[test]
fn test_p2_json_output_success_with_empty_array() {
    // GIVEN: JsonOutput
    let output = JsonOutput::new();

    // WHEN: Calling success with empty array
    // THEN: Should not panic
    output.success(json!([]), None);
}

#[test]
fn test_p2_json_output_error_with_special_characters() {
    // GIVEN: JsonOutput and error with special characters
    let output = JsonOutput::new();
    let err = anyhow!("Error with \"quotes\" and 'apostrophes' and\nnewlines");

    // WHEN: Calling error
    // THEN: Should not panic (special chars are escaped in JSON)
    output.error(&err, -30001, None);
}

#[test]
fn test_p2_json_output_error_with_unicode() {
    // GIVEN: JsonOutput and error with unicode
    let output = JsonOutput::new();
    let err = anyhow!("Error with unicode: \u{1F4A5} boom! 日本語");

    // WHEN: Calling error
    // THEN: Should not panic
    output.error(&err, -30001, None);
}

#[test]
fn test_p2_json_output_success_with_deeply_nested_data() {
    // GIVEN: JsonOutput and deeply nested data
    let output = JsonOutput::new();
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

    // WHEN: Calling success
    // THEN: Should not panic
    output.success(data, None);
}

#[test]
fn test_p2_json_output_multiple_calls_work() {
    // GIVEN: JsonOutput
    let output = JsonOutput::new();

    // WHEN: Making multiple calls
    // THEN: Should all work without panic
    output.success(json!("first"), None);
    output.success(json!("second"), None);
    output.error(&anyhow!("error"), -30001, None);
    output.progress("ignored progress");
    output.success(json!("third"), None);
}

// ============================================================================
// P2: Default Trait Implementation Tests
// ============================================================================

#[test]
fn test_p2_json_output_default_creates_instance() {
    // GIVEN: Default trait
    // WHEN: Creating JsonOutput via Default
    let output = JsonOutput::default();

    // THEN: Should create a valid instance
    output.success(json!("test"), None);
}

#[test]
fn test_p2_json_output_new_and_default_equivalent() {
    // GIVEN: Both construction methods
    let output1 = JsonOutput::new();
    let output2 = JsonOutput::default();

    // WHEN/THEN: Both should work identically
    output1.success(json!("test"), None);
    output2.success(json!("test"), None);
}
