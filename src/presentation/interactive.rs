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

    fn table(&self, title: Option<&str>, data: serde_json::Value) {
        // Display title if provided
        if let Some(t) = title {
            info!("{}", t.cyan().bold());
        }

        // Extract rows from JSON array
        let rows = match data.as_array() {
            Some(arr) => arr,
            None => return,
        };

        if rows.is_empty() {
            return;
        }

        // Extract headers from first row's keys
        let first_row = match rows.first().and_then(|r| r.as_object()) {
            Some(obj) => obj,
            None => return,
        };

        let headers: Vec<&str> = first_row.keys().map(|k| k.as_str()).collect();

        // Convert rows to string values
        let row_data: Vec<Vec<String>> = rows
            .iter()
            .filter_map(|r| r.as_object())
            .map(|obj| {
                headers
                    .iter()
                    .map(|h| {
                        obj.get(*h)
                            .map(|v| match v {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Null => "-".to_string(),
                                other => other.to_string(),
                            })
                            .unwrap_or_else(|| "-".to_string())
                    })
                    .collect()
            })
            .collect();

        // Calculate column widths based on headers and data
        let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in &row_data {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        let total_width: usize = widths.iter().sum::<usize>() + (widths.len() - 1) * 2 + 2;
        let separator = "─".repeat(total_width);

        // Print header
        info!("{}", separator);
        let header_line: String = headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                format!("{:<width$}", h.bold(), width = widths[i])
            })
            .collect::<Vec<_>>()
            .join("  ");
        info!(" {}", header_line);
        info!("{}", separator);

        // Print rows
        for row in &row_data {
            let row_line: String = row
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let width = widths.get(i).copied().unwrap_or(cell.len());
                    if i == 0 {
                        // The first column (name) is green
                        format!("{:<width$}", cell.green(), width = width)
                    } else {
                        format!("{:<width$}", cell, width = width)
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            info!(" {}", row_line);
        }

        info!("{}", separator);
    }
}
