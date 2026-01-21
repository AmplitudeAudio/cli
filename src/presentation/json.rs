//! JSON output implementation for machine-parseable responses.
//!
//! This module provides structured JSON output for CLI integration tools
//! (e.g., Amplitude Studio) that need to parse CLI responses programmatically.
//!
//! Unlike InteractiveOutput which uses log macros, JsonOutput writes directly
//! to stdout to ensure the output is valid parseable JSON.

use crate::common::errors::{CliError, error_suggestion, error_type_name};
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
    /// Human-readable error message (the "what" from CliError)
    pub message: String,
    /// Detailed reason for the failure
    pub why: String,
    /// Actionable suggestion for resolving the error
    pub suggestion: String,
    /// Optional context (file path, asset name, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
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
    ///
    /// If the error is a `CliError`, extracts structured fields (code, what, why, suggestion, context).
    /// Otherwise, falls back to the provided code and generates type/suggestion from that code.
    #[allow(dead_code)] // Used by tests via library crate
    pub fn build_error_response(err: &Error, code: i32) -> JsonResponse<()> {
        // Try to downcast to CliError for rich error information
        let error = if let Some(cli_err) = err.downcast_ref::<CliError>() {
            JsonErrorDetails {
                code: cli_err.code,
                type_: cli_err.type_name(),
                message: cli_err.what.clone(),
                why: cli_err.why.clone(),
                suggestion: cli_err.suggestion.clone(),
                context: cli_err.context.clone(),
            }
        } else {
            // Fallback for non-CliError: use provided code and generic mappings
            JsonErrorDetails {
                code,
                type_: error_type_name(code),
                message: err.to_string(),
                why: err.to_string(),
                suggestion: error_suggestion(code),
                context: None,
            }
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
        // Silently ignore write errors to avoid panic in the output path
        let _ = Self::write_response(&mut io::stdout(), &response);
    }

    fn error(&self, err: &Error, code: i32, _request_id: Option<i64>) {
        let response = Self::build_error_response(err, code);
        // Write directly to stdout for parseable JSON
        // Silently ignore write errors to avoid panic in the output path
        let _ = Self::write_response(&mut io::stdout(), &response);
    }

    fn progress(&self, _message: &str) {
        // JSON mode suppresses progress messages for clean, parseable output.
        // Progress is intended for interactive users, not machine consumers.
    }
}
