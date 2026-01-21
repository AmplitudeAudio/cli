//! Structured error handling for the Amplitude CLI.
//!
//! This module provides:
//! - Error code constants organized by range (validation, asset, project, SDK)
//! - `CliError` struct with What/Why/Fix components for structured error responses
//! - Helper functions for error type mapping and suggestions
//! - Convenience constructors for common error scenarios

use std::fmt;

/// Error codes organized by range as defined in Architecture spec.
///
/// Ranges:
/// - `-31xxx`: Validation errors (schema, field, format)
/// - `-30xxx`: Asset errors (not found, already exists, in use)
/// - `-29xxx`: Project errors (not initialized, not registered, already exists)
/// - `-28xxx`: SDK errors (not found, schema load failed)
pub mod codes {
    // =========================================================================
    // Validation errors (-31xxx)
    // =========================================================================

    /// Schema validation failed (e.g., JSON doesn't match expected structure)
    pub const ERR_VALIDATION_SCHEMA: i32 = -31001;

    /// Field validation failed (e.g., required field missing, invalid value)
    pub const ERR_VALIDATION_FIELD: i32 = -31002;

    /// Format validation failed (e.g., invalid date format, malformed ID)
    pub const ERR_VALIDATION_FORMAT: i32 = -31003;

    // =========================================================================
    // Asset errors (-30xxx)
    // =========================================================================

    /// Asset not found in the project
    pub const ERR_ASSET_NOT_FOUND: i32 = -30001;

    /// Asset with this name/ID already exists
    pub const ERR_ASSET_ALREADY_EXISTS: i32 = -30002;

    /// Asset is referenced by other assets and cannot be modified/deleted
    pub const ERR_ASSET_IN_USE: i32 = -30003;

    // =========================================================================
    // Project errors (-29xxx)
    // =========================================================================

    /// Project directory exists but has no .amproject file
    pub const ERR_PROJECT_NOT_INITIALIZED: i32 = -29001;

    /// Project exists on disk but is not registered in the database
    pub const ERR_PROJECT_NOT_REGISTERED: i32 = -29002;

    /// A project with this name already exists
    pub const ERR_PROJECT_ALREADY_EXISTS: i32 = -29003;

    /// Failed to copy template files during project initialization
    pub const ERR_TEMPLATE_COPY_FAILED: i32 = -29004;

    // =========================================================================
    // SDK errors (-28xxx)
    // =========================================================================

    /// Amplitude SDK installation not found (AM_SDK_PATH not set or invalid)
    pub const ERR_SDK_NOT_FOUND: i32 = -28001;

    /// Failed to load SDK schema files (.bfbs files)
    pub const ERR_SDK_SCHEMA_LOAD_FAILED: i32 = -28002;
}

/// Structured CLI error with What/Why/Fix components.
///
/// This error type provides rich context for debugging:
/// - `what`: The specific operation that failed
/// - `why`: The reason for the failure
/// - `suggestion`: How to fix the issue
/// - `context`: Optional additional context (file path, asset name, etc.)
///
/// # Example
///
/// ```
/// use am::common::errors::{codes, CliError};
///
/// let err = CliError::new(
///     codes::ERR_PROJECT_NOT_REGISTERED,
///     "Project 'myproject' is not registered",
///     "The project directory exists but is not tracked in the database",
/// )
/// .with_context("/home/user/myproject");
/// ```
#[derive(Debug, Clone)]
pub struct CliError {
    /// Error code from the codes module
    pub code: i32,
    /// What operation failed
    pub what: String,
    /// Why it failed
    pub why: String,
    /// How to fix it (defaults to suggestion based on error code)
    pub suggestion: String,
    /// Optional context (file path, asset name, etc.)
    pub context: Option<String>,
}

impl CliError {
    /// Create a new CliError with the given code, what, and why.
    ///
    /// The suggestion is automatically populated based on the error code.
    /// Use `with_suggestion()` to override with a custom suggestion.
    pub fn new(code: i32, what: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            code,
            what: what.into(),
            why: why.into(),
            suggestion: error_suggestion(code),
            context: None,
        }
    }

    /// Override the default suggestion with a custom one.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = suggestion.into();
        self
    }

    /// Add optional context (file path, asset name, etc.).
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Get the error type name for JSON serialization.
    ///
    /// Maps the error code to a human-readable type string like
    /// "project_not_registered" or "asset_not_found".
    pub fn type_name(&self) -> String {
        error_type_name(self.code)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.what, self.why)?;
        if let Some(ctx) = &self.context {
            write!(f, " ({})", ctx)?;
        }
        Ok(())
    }
}

impl std::error::Error for CliError {}

/// Map error code to a human-readable error type name.
///
/// Used for JSON serialization to provide a consistent type field
/// that machines can parse and humans can read.
///
/// # Error Code Ranges
///
/// - `-31xxx` → validation errors
/// - `-30xxx` → asset errors
/// - `-29xxx` → project errors
/// - `-28xxx` → SDK errors
pub fn error_type_name(code: i32) -> String {
    match code {
        // Validation errors (-31xxx)
        codes::ERR_VALIDATION_SCHEMA => "schema_validation_error".to_string(),
        codes::ERR_VALIDATION_FIELD => "field_validation_error".to_string(),
        codes::ERR_VALIDATION_FORMAT => "format_validation_error".to_string(),
        -31999..=-31000 => "validation_error".to_string(),

        // Asset errors (-30xxx)
        codes::ERR_ASSET_NOT_FOUND => "asset_not_found".to_string(),
        codes::ERR_ASSET_ALREADY_EXISTS => "asset_already_exists".to_string(),
        codes::ERR_ASSET_IN_USE => "asset_in_use".to_string(),
        -30999..=-30000 => "asset_error".to_string(),

        // Project errors (-29xxx)
        codes::ERR_PROJECT_NOT_INITIALIZED => "project_not_initialized".to_string(),
        codes::ERR_PROJECT_NOT_REGISTERED => "project_not_registered".to_string(),
        codes::ERR_PROJECT_ALREADY_EXISTS => "project_already_exists".to_string(),
        codes::ERR_TEMPLATE_COPY_FAILED => "template_copy_failed".to_string(),
        -29999..=-29000 => "project_error".to_string(),

        // SDK errors (-28xxx)
        codes::ERR_SDK_NOT_FOUND => "sdk_not_found".to_string(),
        codes::ERR_SDK_SCHEMA_LOAD_FAILED => "schema_load_failed".to_string(),
        -28999..=-28000 => "sdk_error".to_string(),

        _ => "unknown_error".to_string(),
    }
}

/// Get a default suggestion based on error code.
///
/// Provides actionable suggestions for common error scenarios.
/// These can be overridden using `CliError::with_suggestion()`.
pub fn error_suggestion(code: i32) -> String {
    match code {
        // Specific project errors
        codes::ERR_PROJECT_NOT_REGISTERED => {
            "Register the project with 'am project register <path>'".to_string()
        }
        codes::ERR_PROJECT_NOT_INITIALIZED => {
            "Initialize a project with 'am project init <name>'".to_string()
        }
        codes::ERR_PROJECT_ALREADY_EXISTS => {
            "Use a different name or remove the existing project first".to_string()
        }
        codes::ERR_TEMPLATE_COPY_FAILED => {
            "Check file permissions and ensure the template path is correct".to_string()
        }

        // Specific SDK errors
        codes::ERR_SDK_NOT_FOUND => {
            "Set the AM_SDK_PATH environment variable to your SDK installation".to_string()
        }
        codes::ERR_SDK_SCHEMA_LOAD_FAILED => {
            "Verify your SDK installation is complete and AM_SDK_PATH is correct".to_string()
        }

        // Specific asset errors
        codes::ERR_ASSET_NOT_FOUND => {
            "Verify the asset name or create it with the appropriate create command".to_string()
        }
        codes::ERR_ASSET_ALREADY_EXISTS => {
            "Use a different name or delete the existing asset first".to_string()
        }
        codes::ERR_ASSET_IN_USE => {
            "Remove references to this asset from other assets before modifying".to_string()
        }

        // Specific validation errors
        codes::ERR_VALIDATION_SCHEMA => {
            "Check that your JSON structure matches the expected schema".to_string()
        }
        codes::ERR_VALIDATION_FIELD => {
            "Check your input values and correct the invalid field".to_string()
        }
        codes::ERR_VALIDATION_FORMAT => "Check the format of your input and try again".to_string(),

        // Generic fallbacks by range
        -31999..=-31000 => "Check your input values and try again".to_string(),
        -30999..=-30000 => "Verify the asset exists or create it first".to_string(),
        -29999..=-29000 => "Initialize a project or register an existing one".to_string(),
        -28999..=-28000 => "Set AM_SDK_PATH environment variable".to_string(),

        _ => "Check the error message for details".to_string(),
    }
}

// =============================================================================
// Convenience constructors for common errors (Task 6)
// =============================================================================

/// Create an error for a project that exists but is not registered.
pub fn project_not_registered(name: &str) -> CliError {
    CliError::new(
        codes::ERR_PROJECT_NOT_REGISTERED,
        format!("Project '{}' is not registered", name),
        "The project directory exists but is not tracked in the database",
    )
}

/// Create an error for a project that already exists.
pub fn project_already_exists(name: &str) -> CliError {
    CliError::new(
        codes::ERR_PROJECT_ALREADY_EXISTS,
        format!("Project '{}' already exists", name),
        "A project with this name is already registered",
    )
}

/// Create an error for a project that is not initialized.
pub fn project_not_initialized(path: &str) -> CliError {
    CliError::new(
        codes::ERR_PROJECT_NOT_INITIALIZED,
        "Project is not initialized",
        "The directory does not contain a .amproject file",
    )
    .with_context(path)
}

/// Create a validation error for an invalid field.
pub fn validation_error(field: &str, reason: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid value for field '{}'", field),
        reason,
    )
}

/// Create an error for an asset that was not found.
pub fn asset_not_found(asset_type: &str, name: &str) -> CliError {
    CliError::new(
        codes::ERR_ASSET_NOT_FOUND,
        format!("{} '{}' not found", asset_type, name),
        "The requested asset does not exist in this project",
    )
}

/// Create an error for an asset that already exists.
pub fn asset_already_exists(asset_type: &str, name: &str) -> CliError {
    CliError::new(
        codes::ERR_ASSET_ALREADY_EXISTS,
        format!("{} '{}' already exists", asset_type, name),
        "An asset with this name already exists in the project",
    )
}

/// Create an error for the SDK not being found.
pub fn sdk_not_found() -> CliError {
    CliError::new(
        codes::ERR_SDK_NOT_FOUND,
        "Amplitude SDK not found",
        "The AM_SDK_PATH environment variable is not set or points to an invalid location",
    )
}

// =============================================================================
// Macro for quick error construction (Task 6.4)
// =============================================================================

/// Macro for quick CliError construction.
///
/// # Usage
///
/// ```
/// use am::cli_error;
/// use am::common::errors::codes;
///
/// // Basic usage with code, what, and why
/// let err = cli_error!(codes::ERR_ASSET_NOT_FOUND, "Sound not found", "Does not exist");
///
/// // With context
/// let err = cli_error!(
///     codes::ERR_ASSET_NOT_FOUND,
///     "Sound not found",
///     "Does not exist",
///     context: "sounds/explosion.json"
/// );
/// ```
#[macro_export]
macro_rules! cli_error {
    ($code:expr, $what:expr, $why:expr) => {
        $crate::common::errors::CliError::new($code, $what, $why)
    };
    ($code:expr, $what:expr, $why:expr, context: $ctx:expr) => {
        $crate::common::errors::CliError::new($code, $what, $why).with_context($ctx)
    };
    ($code:expr, $what:expr, $why:expr, suggestion: $sug:expr) => {
        $crate::common::errors::CliError::new($code, $what, $why).with_suggestion($sug)
    };
    ($code:expr, $what:expr, $why:expr, context: $ctx:expr, suggestion: $sug:expr) => {
        $crate::common::errors::CliError::new($code, $what, $why)
            .with_context($ctx)
            .with_suggestion($sug)
    };
}

// =============================================================================
// Exit Code Determination
// =============================================================================

/// Exit codes for the CLI process.
///
/// These codes follow standard Unix conventions:
/// - `0`: Success - command completed successfully
/// - `1`: User Error - error caused by invalid user input or state
/// - `2`: System Error - error caused by environment or system issues
pub mod exit_codes {
    /// Command completed successfully.
    pub const SUCCESS: i32 = 0;

    /// Error caused by invalid user input or state.
    /// Examples: invalid input, missing file, validation failure, asset not found.
    pub const USER_ERROR: i32 = 1;

    /// Error caused by environment or system issues.
    /// Examples: database failure, disk full, SDK not found, unexpected panic.
    pub const SYSTEM_ERROR: i32 = 2;
}

/// Determine the appropriate exit code based on an error.
///
/// Maps error codes to exit codes according to these rules:
/// - `-28xxx` (SDK errors) → exit code 2 (system error)
/// - `-29xxx` (Project errors) → exit code 1 (user error)
/// - `-30xxx` (Asset errors) → exit code 1 (user error)
/// - `-31xxx` (Validation errors) → exit code 1 (user error)
/// - Unknown/other errors → exit code 1 (user error, safe default)
///
/// # Arguments
///
/// * `error` - The error to analyze
///
/// # Returns
///
/// The appropriate exit code (0, 1, or 2)
pub fn determine_exit_code(error: &anyhow::Error) -> i32 {
    if let Some(cli_err) = error.downcast_ref::<CliError>() {
        match cli_err.code {
            // SDK errors (-28xxx) are system/environment issues
            -28999..=-28000 => exit_codes::SYSTEM_ERROR,
            // All other CliError codes are user errors
            _ => exit_codes::USER_ERROR,
        }
    } else {
        // Non-CliError errors default to system error
        // (conservative choice: unexpected errors are more likely system/environment issues)
        exit_codes::SYSTEM_ERROR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_error_basic() {
        let err = CliError::new(codes::ERR_PROJECT_NOT_REGISTERED, "Test what", "Test why");
        assert_eq!(err.code, codes::ERR_PROJECT_NOT_REGISTERED);
        assert_eq!(err.what, "Test what");
        assert_eq!(err.why, "Test why");
    }

    #[test]
    fn test_error_type_ranges() {
        // Ensure codes are in their expected ranges
        assert!((-31999..=-31000).contains(&codes::ERR_VALIDATION_SCHEMA));
        assert!((-30999..=-30000).contains(&codes::ERR_ASSET_NOT_FOUND));
        assert!((-29999..=-29000).contains(&codes::ERR_PROJECT_NOT_REGISTERED));
        assert!((-28999..=-28000).contains(&codes::ERR_SDK_NOT_FOUND));
    }

    #[test]
    fn test_determine_exit_code_sdk_error() {
        // SDK errors should return exit code 2 (system error)
        let err = CliError::new(
            codes::ERR_SDK_NOT_FOUND,
            "SDK not found",
            "AM_SDK_PATH not set",
        );
        let anyhow_err: anyhow::Error = err.into();
        assert_eq!(determine_exit_code(&anyhow_err), exit_codes::SYSTEM_ERROR);
    }

    #[test]
    fn test_determine_exit_code_project_error() {
        // Project errors should return exit code 1 (user error)
        let err = CliError::new(
            codes::ERR_PROJECT_NOT_REGISTERED,
            "Project not registered",
            "Not in database",
        );
        let anyhow_err: anyhow::Error = err.into();
        assert_eq!(determine_exit_code(&anyhow_err), exit_codes::USER_ERROR);
    }

    #[test]
    fn test_determine_exit_code_asset_error() {
        // Asset errors should return exit code 1 (user error)
        let err = CliError::new(
            codes::ERR_ASSET_NOT_FOUND,
            "Asset not found",
            "Does not exist",
        );
        let anyhow_err: anyhow::Error = err.into();
        assert_eq!(determine_exit_code(&anyhow_err), exit_codes::USER_ERROR);
    }

    #[test]
    fn test_determine_exit_code_validation_error() {
        // Validation errors should return exit code 1 (user error)
        let err = CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Invalid field",
            "Value out of range",
        );
        let anyhow_err: anyhow::Error = err.into();
        assert_eq!(determine_exit_code(&anyhow_err), exit_codes::USER_ERROR);
    }

    #[test]
    fn test_determine_exit_code_non_cli_error() {
        // Non-CliError errors should default to exit code 2 (system error)
        let anyhow_err = anyhow::anyhow!("Some generic I/O error or other system issue");
        assert_eq!(determine_exit_code(&anyhow_err), exit_codes::SYSTEM_ERROR);
    }
}
