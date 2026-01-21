//! Interactive terminal output implementation.
//!
//! This module provides colored terminal output matching the existing
//! CLI patterns using the `success!` macro and `log` macros.

use crate::common::errors::CliError;
use crate::presentation::Output;
use crate::success;
use anyhow::Error;
use colored::Colorize;
use log::{error, info, warn};

/// Interactive terminal output with colored formatting.
///
/// This implementation wraps existing colored terminal behavior,
/// matching the patterns established in `src/common/logger.rs`.
#[derive(Debug, Default)]
pub struct InteractiveOutput;

impl InteractiveOutput {
    /// Create a new InteractiveOutput instance.
    pub fn new() -> Self {
        Self
    }
}

impl Output for InteractiveOutput {
    fn success(&self, data: serde_json::Value, _request_id: Option<i64>) {
        // Use the success! macro for consistent formatting and crash logging
        if let Some(s) = data.as_str() {
            success!("{}", s);
            return;
        }

        // For complex data, pretty-print the JSON
        match serde_json::to_string_pretty(&data) {
            Ok(json) => success!("{}", json),
            Err(_) => success!("Operation completed successfully"),
        }
    }

    fn error(&self, err: &Error, _code: i32, _request_id: Option<i64>) {
        // Try to downcast to CliError for structured display with What/Why/Fix
        if let Some(cli_err) = err.downcast_ref::<CliError>() {
            // Display "What failed" in red
            error!("{}: {}", "Error".red().bold(), cli_err.what);

            // Display context if provided
            if let Some(ctx) = &cli_err.context {
                error!("  {}: {}", "Context".dimmed(), ctx);
            }

            error!("");

            // Display "Why" with the reason
            error!("{}: {}", "Why".yellow(), cli_err.why);

            // Display "Fix" with the suggestion in cyan
            error!("{}: {}", "Fix".cyan(), cli_err.suggestion);
        } else {
            // Fallback for non-CliError: display error with chain
            error!("{}", err);

            // Display the error chain if present
            for cause in err.chain().skip(1) {
                warn!("  caused by: {}", cause);
            }
        }
    }

    fn progress(&self, message: &str) {
        // Use info! macro for consistent formatting and crash logging
        info!("{}", message);
    }
}
