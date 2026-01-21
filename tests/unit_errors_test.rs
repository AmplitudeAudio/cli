//! Unit tests for the error system.
//!
//! Tests cover:
//! - CliError construction and Display impl
//! - Error code to type name mapping
//! - Error code to suggestion mapping
//! - Convenience helper functions
//!
//! Priority levels:
//! - P0: Core error contract, type safety, error code ranges
//! - P1: Display formatting, builder pattern, mapping functions, helper functions
//! - P2: Edge cases, generic fallbacks, unknown codes

use am::common::errors::{
    CliError, codes, error_suggestion, error_type_name, project_already_exists,
    project_not_registered, validation_error,
};

// =============================================================================
// P0: Core CliError Contract Tests
// =============================================================================

#[test]
fn test_p0_cli_error_construction_basic() {
    // GIVEN: Error code, what, and why strings
    // WHEN: Creating a new CliError
    let err = CliError::new(
        codes::ERR_PROJECT_NOT_REGISTERED,
        "Project 'test' not found",
        "Not tracked in database",
    );

    // THEN: All fields should be set correctly
    assert_eq!(err.code, codes::ERR_PROJECT_NOT_REGISTERED);
    assert_eq!(err.what, "Project 'test' not found");
    assert_eq!(err.why, "Not tracked in database");
    assert!(err.context.is_none());
}

#[test]
fn test_p0_cli_error_implements_std_error() {
    // GIVEN: A CliError instance
    let err = CliError::new(codes::ERR_PROJECT_NOT_REGISTERED, "Test", "Test");

    // WHEN: Using std::error::Error trait methods
    // THEN: Should work correctly (source returns None for CliError)
    assert!(std::error::Error::source(&err).is_none());
}

#[test]
fn test_p0_cli_error_converts_to_anyhow() {
    // GIVEN: A CliError
    let cli_err = CliError::new(
        codes::ERR_PROJECT_NOT_REGISTERED,
        "Test error",
        "Test reason",
    );

    // WHEN: Converting to anyhow::Error
    let anyhow_err: anyhow::Error = cli_err.into();

    // THEN: Should be able to downcast back to CliError
    let downcast = anyhow_err.downcast_ref::<CliError>();
    assert!(downcast.is_some());
    assert_eq!(downcast.unwrap().code, codes::ERR_PROJECT_NOT_REGISTERED);
}

#[test]
fn test_p0_cli_error_propagates_with_question_mark() {
    // GIVEN: A function that returns CliError via ? operator
    fn inner() -> anyhow::Result<()> {
        let err = CliError::new(codes::ERR_ASSET_NOT_FOUND, "Not found", "Does not exist");
        Err(err)?
    }

    // WHEN: Calling the function
    let result = inner();

    // THEN: Error should propagate and be downcastable
    assert!(result.is_err());
    let err = result.unwrap_err();
    let cli_err = err.downcast_ref::<CliError>();
    assert!(cli_err.is_some());
}

// =============================================================================
// P0: Error Code Range Tests
// =============================================================================

#[test]
fn test_p0_error_codes_validation_range() {
    // GIVEN: Validation error codes
    // WHEN: Checking their values
    // THEN: Should be in -31xxx range
    assert!((-31999..=-31000).contains(&codes::ERR_VALIDATION_SCHEMA));
    assert!((-31999..=-31000).contains(&codes::ERR_VALIDATION_FIELD));
    assert!((-31999..=-31000).contains(&codes::ERR_VALIDATION_FORMAT));
}

#[test]
fn test_p0_error_codes_asset_range() {
    // GIVEN: Asset error codes
    // WHEN: Checking their values
    // THEN: Should be in -30xxx range
    assert!((-30999..=-30000).contains(&codes::ERR_ASSET_NOT_FOUND));
    assert!((-30999..=-30000).contains(&codes::ERR_ASSET_ALREADY_EXISTS));
    assert!((-30999..=-30000).contains(&codes::ERR_ASSET_IN_USE));
}

#[test]
fn test_p0_error_codes_project_range() {
    // GIVEN: Project error codes
    // WHEN: Checking their values
    // THEN: Should be in -29xxx range
    assert!((-29999..=-29000).contains(&codes::ERR_PROJECT_NOT_INITIALIZED));
    assert!((-29999..=-29000).contains(&codes::ERR_PROJECT_NOT_REGISTERED));
    assert!((-29999..=-29000).contains(&codes::ERR_PROJECT_ALREADY_EXISTS));
}

#[test]
fn test_p0_error_codes_sdk_range() {
    // GIVEN: SDK error codes
    // WHEN: Checking their values
    // THEN: Should be in -28xxx range
    assert!((-28999..=-28000).contains(&codes::ERR_SDK_NOT_FOUND));
    assert!((-28999..=-28000).contains(&codes::ERR_SDK_SCHEMA_LOAD_FAILED));
}

// =============================================================================
// P1: CliError Builder Pattern Tests
// =============================================================================

#[test]
fn test_p1_cli_error_with_context() {
    // GIVEN: A CliError
    // WHEN: Adding context via builder
    let err = CliError::new(
        codes::ERR_ASSET_NOT_FOUND,
        "Asset not found",
        "No such asset exists",
    )
    .with_context("/path/to/asset.json");

    // THEN: Context should be set
    assert_eq!(err.context, Some("/path/to/asset.json".to_string()));
}

#[test]
fn test_p1_cli_error_with_custom_suggestion() {
    // GIVEN: A CliError
    // WHEN: Overriding the default suggestion
    let err = CliError::new(
        codes::ERR_PROJECT_NOT_INITIALIZED,
        "Not initialized",
        "No .amproject",
    )
    .with_suggestion("Run 'am project init myproject' first");

    // THEN: Custom suggestion should override default
    assert_eq!(err.suggestion, "Run 'am project init myproject' first");
}

#[test]
fn test_p1_cli_error_builder_chain() {
    // GIVEN: A CliError
    // WHEN: Chaining multiple builder methods
    let err = CliError::new(codes::ERR_VALIDATION_FIELD, "Field error", "Invalid value")
        .with_suggestion("Use a valid value")
        .with_context("field: name");

    // THEN: All fields should be set correctly
    assert_eq!(err.code, codes::ERR_VALIDATION_FIELD);
    assert_eq!(err.suggestion, "Use a valid value");
    assert_eq!(err.context, Some("field: name".to_string()));
}

// =============================================================================
// P1: CliError Display Tests
// =============================================================================

#[test]
fn test_p1_cli_error_display_without_context() {
    // GIVEN: A CliError without context
    let err = CliError::new(
        codes::ERR_ASSET_NOT_FOUND,
        "Sound 'explosion' not found",
        "Does not exist",
    );

    // WHEN: Formatting as Display
    let display = format!("{}", err);

    // THEN: Should show the "what" and "why" message
    assert_eq!(display, "Sound 'explosion' not found: Does not exist");
}

#[test]
fn test_p1_cli_error_display_with_context() {
    // GIVEN: A CliError with context
    let err = CliError::new(
        codes::ERR_ASSET_NOT_FOUND,
        "Sound not found",
        "Does not exist",
    )
    .with_context("sources/sounds/explosion.json");

    // WHEN: Formatting as Display
    let display = format!("{}", err);

    // THEN: Should include what, why and context in parentheses
    assert_eq!(
        display,
        "Sound not found: Does not exist (sources/sounds/explosion.json)"
    );
}

#[test]
fn test_p1_cli_error_type_name() {
    // GIVEN: A CliError with a known error code
    let err = CliError::new(codes::ERR_PROJECT_NOT_REGISTERED, "Test", "Test");

    // WHEN: Getting the type name
    // THEN: Should return the correct type string
    assert_eq!(err.type_name(), "project_not_registered");
}

// =============================================================================
// P1: Error Type Name Mapping Tests
// =============================================================================

#[test]
fn test_p1_error_type_name_validation_specific() {
    // GIVEN: Specific validation error codes
    // WHEN: Mapping to type names
    // THEN: Should return specific type names
    assert_eq!(
        error_type_name(codes::ERR_VALIDATION_SCHEMA),
        "schema_validation_error"
    );
    assert_eq!(
        error_type_name(codes::ERR_VALIDATION_FIELD),
        "field_validation_error"
    );
    assert_eq!(
        error_type_name(codes::ERR_VALIDATION_FORMAT),
        "format_validation_error"
    );
}

#[test]
fn test_p1_error_type_name_asset_specific() {
    // GIVEN: Specific asset error codes
    // WHEN: Mapping to type names
    // THEN: Should return specific type names
    assert_eq!(
        error_type_name(codes::ERR_ASSET_NOT_FOUND),
        "asset_not_found"
    );
    assert_eq!(
        error_type_name(codes::ERR_ASSET_ALREADY_EXISTS),
        "asset_already_exists"
    );
    assert_eq!(error_type_name(codes::ERR_ASSET_IN_USE), "asset_in_use");
}

#[test]
fn test_p1_error_type_name_project_specific() {
    // GIVEN: Specific project error codes
    // WHEN: Mapping to type names
    // THEN: Should return specific type names
    assert_eq!(
        error_type_name(codes::ERR_PROJECT_NOT_INITIALIZED),
        "project_not_initialized"
    );
    assert_eq!(
        error_type_name(codes::ERR_PROJECT_NOT_REGISTERED),
        "project_not_registered"
    );
    assert_eq!(
        error_type_name(codes::ERR_PROJECT_ALREADY_EXISTS),
        "project_already_exists"
    );
}

#[test]
fn test_p1_error_type_name_sdk_specific() {
    // GIVEN: Specific SDK error codes
    // WHEN: Mapping to type names
    // THEN: Should return specific type names
    assert_eq!(error_type_name(codes::ERR_SDK_NOT_FOUND), "sdk_not_found");
    assert_eq!(
        error_type_name(codes::ERR_SDK_SCHEMA_LOAD_FAILED),
        "schema_load_failed"
    );
}

// =============================================================================
// P1: Error Suggestion Mapping Tests
// =============================================================================

#[test]
fn test_p1_error_suggestion_project_specific() {
    // GIVEN: Specific project error codes
    // WHEN: Getting suggestions
    // THEN: Should return actionable suggestions
    let suggestion = error_suggestion(codes::ERR_PROJECT_NOT_REGISTERED);
    assert!(suggestion.contains("register") || suggestion.contains("Register"));

    let suggestion = error_suggestion(codes::ERR_PROJECT_NOT_INITIALIZED);
    assert!(suggestion.contains("init") || suggestion.contains("Initialize"));

    let suggestion = error_suggestion(codes::ERR_PROJECT_ALREADY_EXISTS);
    assert!(suggestion.contains("different") || suggestion.contains("remove"));
}

#[test]
fn test_p1_error_suggestion_sdk_specific() {
    // GIVEN: SDK not found error code
    // WHEN: Getting suggestion
    // THEN: Should mention AM_SDK_PATH
    let suggestion = error_suggestion(codes::ERR_SDK_NOT_FOUND);
    assert!(suggestion.contains("AM_SDK_PATH"));
}

// =============================================================================
// P1: Helper Function Tests
// =============================================================================

#[test]
fn test_p1_project_not_registered_helper() {
    // GIVEN: A project name
    // WHEN: Creating error with helper
    let err = project_not_registered("myproject");

    // THEN: Should have correct code and include project name
    assert_eq!(err.code, codes::ERR_PROJECT_NOT_REGISTERED);
    assert!(err.what.contains("myproject"));
    assert!(err.what.contains("not registered"));
    assert!(!err.why.is_empty());
    assert!(!err.suggestion.is_empty());
}

#[test]
fn test_p1_project_already_exists_helper() {
    // GIVEN: A project name
    // WHEN: Creating error with helper
    let err = project_already_exists("myproject");

    // THEN: Should have correct code and include project name
    assert_eq!(err.code, codes::ERR_PROJECT_ALREADY_EXISTS);
    assert!(err.what.contains("myproject"));
    assert!(err.what.contains("already exists"));
    assert!(!err.why.is_empty());
    assert!(!err.suggestion.is_empty());
}

#[test]
fn test_p1_validation_error_helper() {
    // GIVEN: A field name and reason
    // WHEN: Creating error with helper
    let err = validation_error("name", "cannot be empty");

    // THEN: Should have correct code and include field info
    assert_eq!(err.code, codes::ERR_VALIDATION_FIELD);
    assert!(err.what.contains("name"));
    assert!(err.why.contains("cannot be empty"));
}

// =============================================================================
// P2: Generic Fallback Tests
// =============================================================================

#[test]
fn test_p2_error_type_name_validation_generic() {
    // GIVEN: An unknown validation code in -31xxx range
    // WHEN: Mapping to type name
    // THEN: Should fall back to generic "validation_error"
    assert_eq!(error_type_name(-31099), "validation_error");
}

#[test]
fn test_p2_error_type_name_unknown() {
    // GIVEN: Error codes outside known ranges
    // WHEN: Mapping to type name
    // THEN: Should return "unknown_error"
    assert_eq!(error_type_name(0), "unknown_error");
    assert_eq!(error_type_name(-1), "unknown_error");
    assert_eq!(error_type_name(-27000), "unknown_error");
}

#[test]
fn test_p2_error_suggestion_generic_ranges() {
    // GIVEN: Unknown codes within known ranges
    // WHEN: Getting suggestions
    // THEN: Should return generic suggestions for each range

    // Validation range
    let suggestion = error_suggestion(-31050);
    assert!(suggestion.contains("input") || suggestion.contains("Check"));

    // Asset range
    let suggestion = error_suggestion(-30050);
    assert!(suggestion.contains("asset") || suggestion.contains("Asset"));

    // Project range
    let suggestion = error_suggestion(-29050);
    assert!(suggestion.contains("project") || suggestion.contains("Initialize"));

    // SDK range
    let suggestion = error_suggestion(-28050);
    assert!(suggestion.contains("AM_SDK_PATH"));
}

#[test]
fn test_p2_json_output_error_structure() {
    // GIVEN: a CliError and a JsonOutput
    let cli_err = CliError::new(
        codes::ERR_PROJECT_NOT_REGISTERED,
        "Project not registered",
        "The project is not tracked in the database",
    )
    .with_context("test/project")
    .with_suggestion("Run 'am project register'");
    let anyhow_err: anyhow::Error = cli_err.into();

    // WHEN: building an error response
    let response = am::presentation::JsonOutput::build_error_response(
        &anyhow_err,
        codes::ERR_PROJECT_NOT_REGISTERED,
    );

    // THEN: the response should have the correct structure
    assert!(!response.ok);
    let error_details = response.error.unwrap();
    assert_eq!(error_details.code, codes::ERR_PROJECT_NOT_REGISTERED);
    assert_eq!(error_details.type_, "project_not_registered");
    assert_eq!(error_details.message, "Project not registered");
    assert_eq!(
        error_details.why,
        "The project is not tracked in the database"
    );
    assert_eq!(error_details.suggestion, "Run 'am project register'");
    assert_eq!(error_details.context, Some("test/project".to_string()));
}
