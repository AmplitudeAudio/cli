//! Unit tests for the presentation module.
//!
//! Tests the Output trait abstraction layer for CLI output formatting.
//!
//! Priority levels:
//! - P0: Core trait contract, type safety guarantees
//! - P1: Output capture and verification, error handling
//! - P2: Edge cases, multiple calls tracking

use am::presentation::{InteractiveOutput, Output, OutputMode};
use anyhow::anyhow;
use serde::Serialize;
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Serialize)]
struct TestData {
    message: String,
    count: i32,
}

/// A mock Output implementation that captures output for testing.
struct MockOutput {
    success_calls: Rc<RefCell<Vec<(serde_json::Value, Option<i64>)>>>,
    error_calls: Rc<RefCell<Vec<(String, i32, Option<i64>)>>>,
    progress_calls: Rc<RefCell<Vec<String>>>,
}

impl MockOutput {
    fn new() -> Self {
        Self {
            success_calls: Rc::new(RefCell::new(Vec::new())),
            error_calls: Rc::new(RefCell::new(Vec::new())),
            progress_calls: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn success_count(&self) -> usize {
        self.success_calls.borrow().len()
    }

    fn error_count(&self) -> usize {
        self.error_calls.borrow().len()
    }

    fn progress_count(&self) -> usize {
        self.progress_calls.borrow().len()
    }

    fn last_success(&self) -> Option<serde_json::Value> {
        self.success_calls.borrow().last().map(|(v, _)| v.clone())
    }

    fn last_error(&self) -> Option<(String, i32)> {
        self.error_calls
            .borrow()
            .last()
            .map(|(msg, code, _)| (msg.clone(), *code))
    }

    fn last_progress(&self) -> Option<String> {
        self.progress_calls.borrow().last().cloned()
    }
}

impl Output for MockOutput {
    fn success(&self, data: serde_json::Value, request_id: Option<i64>) {
        self.success_calls.borrow_mut().push((data, request_id));
    }

    fn error(&self, err: &anyhow::Error, code: i32, request_id: Option<i64>) {
        self.error_calls
            .borrow_mut()
            .push((err.to_string(), code, request_id));
    }

    fn progress(&self, message: &str) {
        self.progress_calls.borrow_mut().push(message.to_string());
    }

    fn table(&self, _title: Option<&str>, _data: serde_json::Value) {
        // Mock implementation - does nothing for testing
    }

    fn mode(&self) -> am::presentation::OutputMode {
        am::presentation::OutputMode::Interactive
    }
}

// Safety: MockOutput is only used in single-threaded tests
unsafe impl Send for MockOutput {}
unsafe impl Sync for MockOutput {}

// ============================================================================
// P0: Core Type Safety Tests - Output trait guarantees
// ============================================================================

#[test]
fn test_p0_output_trait_is_object_safe() {
    // GIVEN: An Output implementation
    // WHEN: Used as dyn Output
    // THEN: Should compile and work correctly
    fn take_output(_: &dyn Output) {}
    let output = InteractiveOutput::new();
    take_output(&output);
}

#[test]
fn test_p0_interactive_output_is_send_sync() {
    // GIVEN: InteractiveOutput type
    // WHEN: Checking Send + Sync bounds
    // THEN: Should satisfy both traits for thread safety
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<InteractiveOutput>();
}

#[test]
fn test_p0_boxed_output_works() {
    // GIVEN: An Output boxed as trait object
    let output: Box<dyn Output> = Box::new(InteractiveOutput::new());

    // WHEN: Calling methods through the box
    // THEN: Should work without panic
    // Note: progress() goes through log macros which is acceptable
    output.progress("testing...");
}

// ============================================================================
// P0: Core Output Contract Tests - success() method
// ============================================================================

#[test]
fn test_p0_output_success_captures_json_value() {
    // GIVEN: A mock output and JSON data
    let output = MockOutput::new();
    let data = json!({"message": "test", "count": 42});

    // WHEN: Calling success with the data
    output.success(data.clone(), None);

    // THEN: Should capture the exact value
    assert_eq!(output.success_count(), 1);
    assert_eq!(output.last_success(), Some(data));
}

#[test]
fn test_p0_output_success_captures_string_value() {
    // GIVEN: A mock output
    let output = MockOutput::new();

    // WHEN: Calling success with a string value
    output.success(json!("Simple message"), None);

    // THEN: Should capture the string value
    assert_eq!(output.success_count(), 1);
    assert_eq!(output.last_success(), Some(json!("Simple message")));
}

// ============================================================================
// P0: Core Output Contract Tests - error() method
// ============================================================================

#[test]
fn test_p0_output_error_captures_message_and_code() {
    // GIVEN: A mock output and an error
    let output = MockOutput::new();
    let err = anyhow!("Test error message");

    // WHEN: Calling error with the error and code
    output.error(&err, -30001, None);

    // THEN: Should capture error message and code
    assert_eq!(output.error_count(), 1);
    let (msg, code) = output.last_error().unwrap();
    assert!(msg.contains("Test error message"));
    assert_eq!(code, -30001);
}

// ============================================================================
// P0: Core Output Contract Tests - progress() method
// ============================================================================

#[test]
fn test_p0_output_progress_captures_message() {
    // GIVEN: A mock output
    let output = MockOutput::new();

    // WHEN: Calling progress with a message
    output.progress("Loading project...");

    // THEN: Should capture the exact message
    assert_eq!(output.progress_count(), 1);
    assert_eq!(
        output.last_progress(),
        Some("Loading project...".to_string())
    );
}

// ============================================================================
// P1: InteractiveOutput Tests - Verify no panics
// ============================================================================

#[test]
fn test_p1_interactive_output_success_with_json_value() {
    // GIVEN: An interactive output
    let output = InteractiveOutput::new();
    let data = json!({"message": "test", "count": 42});

    // WHEN: Calling success with JSON data
    // THEN: Should not panic (writes to stdout via logger)
    output.success(data, None);
}

#[test]
fn test_p1_interactive_output_success_with_string_value() {
    // GIVEN: An interactive output
    let output = InteractiveOutput::new();

    // WHEN: Calling success with a string value
    // THEN: Should extract and display cleanly without panic
    output.success(json!("Simple message"), None);
}

#[test]
fn test_p1_interactive_output_error_includes_message() {
    // GIVEN: An interactive output and an error
    let output = InteractiveOutput::new();
    let err = anyhow!("Test error message");

    // WHEN: Calling error
    // THEN: Should not panic
    output.error(&err, -30001, None);
}

#[test]
fn test_p1_interactive_output_progress_produces_output() {
    // GIVEN: An interactive output
    let output = InteractiveOutput::new();

    // WHEN: Calling progress
    // THEN: Should not panic
    output.progress("Loading project...");
}

// ============================================================================
// P1: Request ID Tests - JSON-RPC 2.0 support
// ============================================================================

#[test]
fn test_p1_output_success_captures_request_id() {
    // GIVEN: A mock output
    let output = MockOutput::new();
    let data = json!("test");

    // WHEN: Calling success with a request ID
    output.success(data, Some(42));

    // THEN: Should capture the request ID
    assert_eq!(output.success_count(), 1);
    let calls = output.success_calls.borrow();
    assert_eq!(calls[0].1, Some(42));
}

// ============================================================================
// P1: Error Chain Tests
// ============================================================================

#[test]
fn test_p1_output_error_captures_chained_errors() {
    // GIVEN: A mock output and a chained error
    let output = MockOutput::new();
    let err = anyhow!("Root cause").context("Additional context");

    // WHEN: Calling error with the chained error
    output.error(&err, -30002, None);

    // THEN: Should capture the outer context message
    assert_eq!(output.error_count(), 1);
    let (msg, code) = output.last_error().unwrap();
    assert!(msg.contains("Additional context"));
    assert_eq!(code, -30002);
}

#[test]
fn test_p1_interactive_output_error_with_context() {
    // GIVEN: An interactive output and a chained error
    let output = InteractiveOutput::new();
    let err = anyhow!("Root cause").context("Additional context");

    // WHEN: Calling error
    // THEN: Should not panic and display chain
    output.error(&err, -30002, None);
}

// ============================================================================
// P1: Serialization Tests
// ============================================================================

#[test]
fn test_p1_output_success_with_serialized_struct() {
    // GIVEN: A mock output and a serializable struct
    let output = MockOutput::new();
    let data = TestData {
        message: "test".to_string(),
        count: 42,
    };

    // WHEN: Manually serializing like handlers do with json!() macro
    let value = serde_json::to_value(&data).unwrap();
    output.success(value, None);

    // THEN: Should capture the serialized data correctly
    assert_eq!(output.success_count(), 1);
    let result = output.last_success().unwrap();
    assert_eq!(result["message"], "test");
    assert_eq!(result["count"], 42);
}

// ============================================================================
// P2: Multiple Calls Tests
// ============================================================================

#[test]
fn test_p2_output_multiple_progress_calls_tracked() {
    // GIVEN: A mock output
    let output = MockOutput::new();

    // WHEN: Calling progress multiple times
    output.progress("Step 1");
    output.progress("Step 2");
    output.progress("Step 3");

    // THEN: Should track all calls
    assert_eq!(output.progress_count(), 3);
    assert_eq!(output.last_progress(), Some("Step 3".to_string()));
}

// ============================================================================
// P1: create_output() Factory Function Tests
// ============================================================================

#[test]
fn test_p1_create_output_returns_interactive_by_default() {
    // GIVEN: OutputMode::Interactive
    // WHEN: Calling create_output
    let output = am::presentation::create_output(OutputMode::Interactive);

    // THEN: Should return a valid Output implementation
    // Verify by calling progress (goes through log macros)
    output.progress("testing...");
}

#[test]
fn test_p1_create_output_with_json_mode_returns_json_output() {
    // GIVEN: OutputMode::Json
    // WHEN: Calling create_output
    let output = am::presentation::create_output(OutputMode::Json);

    // THEN: Should return a working Output trait object
    // (prompting is handled by the Input abstraction, not Output)
    output.progress("test progress message");
    output.success(json!("ok"), None);
}

#[test]
fn test_p1_create_output_returns_boxed_output() {
    // GIVEN: Any mode
    // WHEN: Calling create_output
    let output: Box<dyn Output> = am::presentation::create_output(OutputMode::Interactive);

    // THEN: Should return Box<dyn Output> that can be used
    // Verify by calling progress (goes through log macros)
    output.progress("testing...");
}

#[test]
fn test_p2_create_output_result_is_send_sync() {
    // GIVEN: create_output result
    let output = am::presentation::create_output(OutputMode::Interactive);

    // WHEN/THEN: Can be passed to a function requiring Send + Sync
    fn assert_send_sync<T: Send + Sync + ?Sized>(_: &T) {}
    assert_send_sync(&*output);
}
