//! Interactive terminal output implementation.
//!
//! This module provides colored terminal output matching the existing
//! CLI patterns using the `success!` macro and `log` macros.

use crate::presentation::Output;
use crate::success;
use anyhow::Error;
use colored::Colorize;
use log::{error, info};

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
        // For complex data, show success indicator
        success!("Operation completed successfully");
    }

    fn error(&self, err: &Error, code: i32, _request_id: Option<i64>) {
        // Use error! macro for consistent formatting and crash logging
        // Include error code for structured error reporting
        error!("[{}] {}", code, err);

        // Display error chain if present
        for cause in err.chain().skip(1) {
            println!("  {} {}", "caused by:".red(), cause);
        }
    }

    fn progress(&self, message: &str) {
        // Use info! macro for consistent formatting and crash logging
        info!("{}", message);
    }
}
