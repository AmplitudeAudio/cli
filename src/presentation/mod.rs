//! Presentation layer for CLI output abstraction.
//!
//! This module provides the `Output` trait that abstracts how command results
//! are presented, allowing command handlers to return pure data without knowing
//! the output format.

mod interactive;
mod json;

pub use interactive::InteractiveOutput;
#[allow(unused_imports)] // Exported for library consumers and tests
pub use json::{JsonErrorDetails, JsonOutput, JsonResponse};

use anyhow::Error;

/// Output mode for CLI presentation.
///
/// Determines which output implementation is used for formatting command results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// Interactive terminal output with colors and formatting.
    /// Default mode for human users.
    #[default]
    Interactive,
    /// JSON output for machine-parseable responses.
    /// Used by integration tools like Amplitude Studio.
    #[allow(dead_code)] // Used in Story 1.3 (Global Flag Wiring)
    Json,
    // Future: StudioIpc for JSON-RPC 2.0 communication
}

/// Trait for abstracting CLI output presentation.
///
/// This trait allows command handlers to produce output without knowing
/// the specific format (interactive terminal, JSON, JSON-RPC, etc.).
///
/// The `request_id` parameter is for future JSON-RPC 2.0 support.
/// Interactive implementations ignore it.
///
/// Note: Uses `serde_json::Value` instead of generics to maintain dyn-compatibility.
/// Callers should use `serde_json::to_value()` or `json!()` macro to convert their data.
pub trait Output: Send + Sync {
    /// Display a successful result.
    ///
    /// # Arguments
    /// * `data` - JSON value representing the result data
    /// * `request_id` - Optional JSON-RPC request ID (ignored by interactive output)
    fn success(&self, data: serde_json::Value, request_id: Option<i64>);

    /// Display an error.
    ///
    /// # Arguments
    /// * `err` - The error to display
    /// * `code` - Error code (following error code ranges in project-context.md)
    /// * `request_id` - Optional JSON-RPC request ID (ignored by interactive output)
    ///
    /// Note: This method will be used for structured error handling in future stories.
    /// Currently, command handlers use anyhow::Result propagation with the ? operator,
    /// and errors are handled at the top level in main.rs.
    #[allow(dead_code)]
    fn error(&self, err: &Error, code: i32, request_id: Option<i64>);

    /// Display a progress message.
    ///
    /// # Arguments
    /// * `message` - Progress message to display
    fn progress(&self, message: &str);
}

/// Create an Output implementation based on the requested mode.
///
/// # Arguments
/// * `mode` - The output mode determining which implementation to use
///
/// # Returns
/// A boxed Output implementation
///
/// Note: Non-interactive behavior is handled by the `Input` abstraction.
/// `Output` is presentation-only.
pub fn create_output(mode: OutputMode) -> Box<dyn Output> {
    match mode {
        OutputMode::Interactive => Box::new(InteractiveOutput::new()),
        OutputMode::Json => Box::new(JsonOutput::new()),
    }
}
