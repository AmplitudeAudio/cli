//! JSON output implementation for machine-parseable responses.
//!
//! This module provides structured JSON output for CLI integration tools
//! (e.g., Amplitude Studio) that need to parse CLI responses programmatically.
//!
//! Unlike InteractiveOutput which uses log macros, JsonOutput writes directly
//! to stdout to ensure the output is valid parseable JSON.

use crate::presentation::Output;
use anyhow::{Error, Result};
use serde::Serialize;
use std::io::{self, Write};

/// JSON response envelope for success responses.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct JsonResponse<T: Serialize> {
    /// Indicates success (true) or failure (false)
    pub ok: bool,
    /// The success value (present when ok=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<T>,
    /// The error details (present when ok=false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonErrorDetails>,
}

/// Structured error information for JSON error responses.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct JsonErrorDetails {
    /// Numeric error code (from error code ranges)
    pub code: i32,
    /// Error type category (validation_error, asset_error, etc.)
    #[serde(rename = "type")]
    pub type_: String,
    /// Human-readable error message
    pub message: String,
    /// Actionable suggestion for resolving the error
    pub suggestion: String,
}

/// JSON output implementation for machine-parseable CLI responses.
///
/// This implementation outputs structured JSON to stdout in the envelope format:
/// - Success: `{ "ok": true, "value": {...} }`
/// - Error: `{ "ok": false, "error": { "code": ..., "type": ..., "message": ..., "suggestion": ... } }`
///
/// Unlike InteractiveOutput, this writes directly to stdout (not via log macros)
/// to ensure the output is valid, parseable JSON without any prefixes or formatting.
#[derive(Debug, Default)]
pub struct JsonOutput;

impl JsonOutput {
    /// Create a new JsonOutput instance.
    pub fn new() -> Self {
        Self
    }
}

impl JsonOutput {
    /// Build a success response structure without writing to stdout.
    /// Useful for testing and for building responses that will be written elsewhere.
    #[allow(dead_code)] // Used by tests via library crate
    pub fn build_success_response(data: serde_json::Value) -> JsonResponse<serde_json::Value> {
        JsonResponse {
            ok: true,
            value: Some(data),
            error: None,
        }
    }

    /// Build an error response structure without writing to stdout.
    /// Useful for testing and for building responses that will be written elsewhere.
    #[allow(dead_code)] // Used by tests via library crate
    pub fn build_error_response(err: &Error, code: i32) -> JsonResponse<()> {
        let error = JsonErrorDetails {
            code,
            type_: error_type_from_code(code),
            message: err.to_string(),
            suggestion: suggestion_from_code(code),
        };
        JsonResponse {
            ok: false,
            value: None,
            error: Some(error),
        }
    }

    /// Serialize a response to a pretty-printed JSON string.
    #[allow(dead_code)] // Used by tests via library crate
    pub fn serialize_response<T: Serialize>(response: &JsonResponse<T>) -> Result<String> {
        serde_json::to_string_pretty(response)
            .map_err(|e| anyhow::anyhow!("JSON serialization failed: {}", e))
    }

    /// Write a response to a writer with proper flushing.
    #[allow(dead_code)] // Used by tests via library crate
    pub fn write_response<W: Write, T: Serialize>(
        writer: &mut W,
        response: &JsonResponse<T>,
    ) -> Result<()> {
        let json = Self::serialize_response(response)?;
        writeln!(writer, "{}", json)?;
        writer.flush()?;
        Ok(())
    }
}

impl Output for JsonOutput {
    fn success(&self, data: serde_json::Value, _request_id: Option<i64>) {
        let response = Self::build_success_response(data);
        // Write directly to stdout, not via log macros, for parseable JSON
        // Silently ignore write errors to avoid panic in output path
        let _ = Self::write_response(&mut io::stdout(), &response);
    }

    fn error(&self, err: &Error, code: i32, _request_id: Option<i64>) {
        let response = Self::build_error_response(err, code);
        // Write directly to stdout for parseable JSON
        // Silently ignore write errors to avoid panic in output path
        let _ = Self::write_response(&mut io::stdout(), &response);
    }

    fn progress(&self, _message: &str) {
        // JSON mode suppresses progress messages for clean, parseable output.
        // Progress is intended for interactive users, not machine consumers.
    }
}

/// Map error code to a human-readable error type.
///
/// Based on error code ranges defined in project-context.md:
/// - -31xxx: Validation errors
/// - -30xxx: Asset operation errors
/// - -29xxx: Project operation errors
/// - -28xxx: SDK errors
///
/// Within each range, more specific error codes map to specific types.
/// For example, -30001 maps to "asset_not_found" as shown in AC #2.
fn error_type_from_code(code: i32) -> String {
    match code {
        // Validation errors (-31xxx)
        -31001 => "schema_validation_error".to_string(),
        -31002 => "field_validation_error".to_string(),
        -31003 => "format_validation_error".to_string(),
        -31999..=-31000 => "validation_error".to_string(),

        // Asset operation errors (-30xxx)
        -30001 => "asset_not_found".to_string(),
        -30002 => "asset_already_exists".to_string(),
        -30003 => "asset_in_use".to_string(),
        -30999..=-30000 => "asset_error".to_string(),

        // Project operation errors (-29xxx)
        -29001 => "project_not_initialized".to_string(),
        -29002 => "project_not_registered".to_string(),
        -29003 => "project_already_exists".to_string(),
        -29999..=-29000 => "project_error".to_string(),

        // SDK errors (-28xxx)
        -28001 => "sdk_not_found".to_string(),
        -28002 => "schema_load_failed".to_string(),
        -28999..=-28000 => "sdk_error".to_string(),

        _ => "unknown_error".to_string(),
    }
}

/// Generate a suggestion based on error code range.
///
/// More specific suggestions will be provided in Story 1.4 (Structured Error Responses).
/// These are general fallback suggestions based on error category.
fn suggestion_from_code(code: i32) -> String {
    match code {
        -31999..=-31000 => "Check your input values and try again".to_string(),
        -30999..=-30000 => "Verify the asset exists or create it first".to_string(),
        -29999..=-29000 => "Initialize a project or register an existing one".to_string(),
        -28999..=-28000 => "Set AM_SDK_PATH environment variable".to_string(),
        _ => "Check the error message for details".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_type_from_code_validation_specific() {
        // Specific validation error codes
        assert_eq!(error_type_from_code(-31001), "schema_validation_error");
        assert_eq!(error_type_from_code(-31002), "field_validation_error");
        assert_eq!(error_type_from_code(-31003), "format_validation_error");
    }

    #[test]
    fn test_error_type_from_code_validation_generic() {
        // Generic validation errors fall back to validation_error
        assert_eq!(error_type_from_code(-31004), "validation_error");
        assert_eq!(error_type_from_code(-31999), "validation_error");
    }

    #[test]
    fn test_error_type_from_code_asset_specific() {
        // Specific asset error codes (AC #2 example uses -30001 -> asset_not_found)
        assert_eq!(error_type_from_code(-30001), "asset_not_found");
        assert_eq!(error_type_from_code(-30002), "asset_already_exists");
        assert_eq!(error_type_from_code(-30003), "asset_in_use");
    }

    #[test]
    fn test_error_type_from_code_asset_generic() {
        // Generic asset errors fall back to asset_error
        assert_eq!(error_type_from_code(-30004), "asset_error");
        assert_eq!(error_type_from_code(-30999), "asset_error");
    }

    #[test]
    fn test_error_type_from_code_project_specific() {
        // Specific project error codes
        assert_eq!(error_type_from_code(-29001), "project_not_initialized");
        assert_eq!(error_type_from_code(-29002), "project_not_registered");
        assert_eq!(error_type_from_code(-29003), "project_already_exists");
    }

    #[test]
    fn test_error_type_from_code_project_generic() {
        // Generic project errors fall back to project_error
        assert_eq!(error_type_from_code(-29004), "project_error");
        assert_eq!(error_type_from_code(-29999), "project_error");
    }

    #[test]
    fn test_error_type_from_code_sdk_specific() {
        // Specific SDK error codes
        assert_eq!(error_type_from_code(-28001), "sdk_not_found");
        assert_eq!(error_type_from_code(-28002), "schema_load_failed");
    }

    #[test]
    fn test_error_type_from_code_sdk_generic() {
        // Generic SDK errors fall back to sdk_error
        assert_eq!(error_type_from_code(-28003), "sdk_error");
        assert_eq!(error_type_from_code(-28999), "sdk_error");
    }

    #[test]
    fn test_error_type_from_code_unknown() {
        assert_eq!(error_type_from_code(0), "unknown_error");
        assert_eq!(error_type_from_code(-1), "unknown_error");
        assert_eq!(error_type_from_code(-27000), "unknown_error");
    }

    #[test]
    fn test_suggestion_from_code_categories() {
        assert!(suggestion_from_code(-31001).contains("input values"));
        assert!(suggestion_from_code(-30001).contains("asset"));
        assert!(suggestion_from_code(-29001).contains("project"));
        assert!(suggestion_from_code(-28001).contains("AM_SDK_PATH"));
        assert!(suggestion_from_code(0).contains("error message"));
    }
}
