//! Presentation layer for CLI output abstraction.
//!
//! This module provides the `Output` trait that abstracts how command results
//! are presented, allowing command handlers to return pure data without knowing
//! the output format.

mod interactive;
pub mod json;

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
    fn error(&self, err: &Error, code: i32, request_id: Option<i64>);

    /// Display a progress message.
    ///
    /// # Arguments
    /// * `message` - Progress message to display
    fn progress(&self, message: &str);

    /// Display tabular data with an optional title.
    ///
    /// In interactive mode, renders a formatted table with headers and rows.
    /// In JSON mode, outputs a JSON envelope with the data as array of objects.
    ///
    /// # Arguments
    /// * `title` - Optional title to display above the table (interactive mode only)
    /// * `data` - The data to display as a JSON array of objects
    fn table(&self, title: Option<&str>, data: serde_json::Value);

    /// Get the current output mode.
    ///
    /// Commands can use this to conditionally format output based on the mode,
    /// avoiding duplicate output in interactive mode where progress messages
    /// already display the information.
    fn mode(&self) -> OutputMode;
}

/// Create an Output implementation based on the requested mode.
///
/// # Arguments
/// * `mode` - The output mode determining which implementation to use
///
/// # Returns
/// A boxed Output implementation
///
pub fn create_output(mode: OutputMode) -> Box<dyn Output> {
    match mode {
        OutputMode::Interactive => Box::new(InteractiveOutput::new()),
        OutputMode::Json => Box::new(JsonOutput::new()),
    }
}
