//! JSON output implementation for machine-parseable responses.
//!
//! This module provides structured JSON output for CLI integration tools
//! (e.g., Amplitude Studio) that need to parse CLI responses programmatically.
//!
//! Unlike InteractiveOutput which uses log macros, JsonOutput writes directly
//! to stdout to ensure the output is valid parseable JSON.

use crate::presentation::Output;
use anyhow::Error;
use serde::Serialize;
use std::io::{self, Write};

/// JSON response envelope for success responses.
#[derive(Serialize)]
struct JsonResponse<T: Serialize> {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonError>,
}

/// Structured error information for JSON error responses.
#[derive(Serialize)]
struct JsonError {
    code: i32,
    #[serde(rename = "type")]
    type_: String,
    message: String,
    suggestion: String,
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

impl Output for JsonOutput {
    fn success(&self, data: serde_json::Value, _request_id: Option<i64>) {
        let response = JsonResponse {
            ok: true,
            value: Some(data),
            error: None,
        };
        // Write directly to stdout, not via log macros, for parseable JSON
        if let Ok(json) = serde_json::to_string_pretty(&response) {
            let _ = writeln!(io::stdout(), "{}", json);
        }
    }

    fn error(&self, err: &Error, code: i32, _request_id: Option<i64>) {
        let error = JsonError {
            code,
            type_: error_type_from_code(code),
            message: err.to_string(),
            suggestion: suggestion_from_code(code),
        };
        let response: JsonResponse<()> = JsonResponse {
            ok: false,
            value: None,
            error: Some(error),
        };
        // Write directly to stdout for parseable JSON
        if let Ok(json) = serde_json::to_string_pretty(&response) {
            let _ = writeln!(io::stdout(), "{}", json);
        }
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
fn error_type_from_code(code: i32) -> String {
    match code {
        -31999..=-31000 => "validation_error".to_string(),
        -30999..=-30000 => "asset_error".to_string(),
        -29999..=-29000 => "project_error".to_string(),
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
    fn test_error_type_from_code_validation() {
        assert_eq!(error_type_from_code(-31001), "validation_error");
        assert_eq!(error_type_from_code(-31999), "validation_error");
    }

    #[test]
    fn test_error_type_from_code_asset() {
        assert_eq!(error_type_from_code(-30001), "asset_error");
        assert_eq!(error_type_from_code(-30999), "asset_error");
    }

    #[test]
    fn test_error_type_from_code_project() {
        assert_eq!(error_type_from_code(-29001), "project_error");
        assert_eq!(error_type_from_code(-29999), "project_error");
    }

    #[test]
    fn test_error_type_from_code_sdk() {
        assert_eq!(error_type_from_code(-28001), "sdk_error");
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
