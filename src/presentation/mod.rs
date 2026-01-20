//! Presentation layer for CLI output abstraction.
//!
//! This module provides the `Output` trait that abstracts how command results
//! are presented, allowing command handlers to return pure data without knowing
//! the output format.

mod interactive;

pub use interactive::InteractiveOutput;

use anyhow::Error;

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
/// * `json_mode` - If true, returns JsonOutput (not yet implemented, returns Interactive)
///
/// # Returns
/// A boxed Output implementation
pub fn create_output(_json_mode: bool) -> Box<dyn Output> {
    // For Story 1.1, always return InteractiveOutput
    // JsonOutput will be added in Story 1.2
    Box::new(InteractiveOutput::new())
}
