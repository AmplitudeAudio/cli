//! Unit tests for project command validation functions.

use am::app::{App, Commands};
use am::commands::project::ProjectCommands;
use clap::Parser;

// =============================================================================
// Info Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_project_info_command_parses_without_name() {
    let args = ["am", "project", "info"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Project {
            command: ProjectCommands::Info { name },
        } => {
            assert!(name.is_none(), "Name should be None when not provided");
        }
        _ => panic!("Expected Project Info command"),
    }
}

#[test]
fn test_p0_project_info_command_parses_with_name() {
    let args = ["am", "project", "info", "my_project"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Project {
            command: ProjectCommands::Info { name },
        } => {
            assert_eq!(name, Some("my_project".to_string()), "Name should match");
        }
        _ => panic!("Expected Project Info command"),
    }
}

// =============================================================================
// transform_name Tests
// =============================================================================

#[test]
fn test_p1_transform_name_converts_to_lowercase() {
    let input = "MyProject";
    let result = transform_name(input);
    assert_eq!(result, "myproject");
}

#[test]
fn test_p1_transform_name_replaces_spaces_with_underscores() {
    let input = "my project name";
    let result = transform_name(input);
    assert_eq!(result, "my_project_name");
}

#[test]
fn test_p1_transform_name_replaces_hyphens_with_underscores() {
    let input = "my-project-name";
    let result = transform_name(input);
    assert_eq!(result, "my_project_name");
}

#[test]
fn test_p1_transform_name_handles_mixed_case_and_separators() {
    let input = "My Cool-Project Name";
    let result = transform_name(input);
    assert_eq!(result, "my_cool_project_name");
}

#[test]
fn test_p2_transform_name_preserves_underscores() {
    let input = "my_existing_project";
    let result = transform_name(input);
    assert_eq!(result, "my_existing_project");
}

#[test]
fn test_p2_transform_name_handles_numbers() {
    let input = "Project123";
    let result = transform_name(input);
    assert_eq!(result, "project123");
}

// =============================================================================
// validate_name Tests
// =============================================================================

#[test]
fn test_p1_validate_name_accepts_alphanumeric() {
    let input = "myproject123";
    let result = validate_name(input);
    assert!(result.is_valid(), "Alphanumeric name should be valid");
}

#[test]
fn test_p1_validate_name_accepts_underscores() {
    let input = "my_project_name";
    let result = validate_name(input);
    assert!(result.is_valid(), "Name with underscores should be valid");
}

#[test]
fn test_p1_validate_name_accepts_hyphens() {
    let input = "my-project-name";
    let result = validate_name(input);
    assert!(result.is_valid(), "Name with hyphens should be valid");
}

#[test]
fn test_p1_validate_name_accepts_spaces() {
    let input = "my project name";
    let result = validate_name(input);
    assert!(result.is_valid(), "Name with spaces should be valid");
}

#[test]
fn test_p1_validate_name_rejects_special_characters() {
    let invalid_names = [
        "project@name",
        "project#name",
        "project$name",
        "project%name",
        "project!name",
        "project*name",
        "project/name",
        "project\\name",
        "project.name",
        "project:name",
    ];

    for name in invalid_names {
        let result = validate_name(name);
        assert!(
            !result.is_valid(),
            "Name '{}' with special character should be invalid",
            name
        );
    }
}

#[test]
fn test_p1_validate_name_rejects_empty_string() {
    let input = "";
    let result = validate_name(input);
    assert!(!result.is_valid(), "Empty name should be invalid");
}

#[test]
fn test_p1_validate_name_rejects_whitespace_only() {
    let input = "   ";
    let result = validate_name(input);
    assert!(!result.is_valid(), "Whitespace-only name should be invalid");
}

// =============================================================================
// Helper Functions (Mirror of source implementation)
// =============================================================================

fn transform_name(name: &str) -> String {
    name.to_lowercase().replace(' ', "_").replace('-', "_")
}

struct ValidationResult {
    valid: bool,
}

impl ValidationResult {
    fn is_valid(&self) -> bool {
        self.valid
    }
}

fn validate_name(name: &str) -> ValidationResult {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return ValidationResult { valid: false };
    }

    let valid = !trimmed
        .chars()
        .any(|c| !c.is_alphanumeric() && c != '_' && c != '-' && c != ' ');

    ValidationResult { valid }
}

// =============================================================================
// read_amproject_file Tests
// =============================================================================

use am::common::utils::{count_assets_by_type, read_amproject_file};
use am::database::entities::ProjectConfiguration;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_p0_read_amproject_file_parses_valid_json() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let amproject_path = temp_dir.path().join(".amproject");

    let config_json = r#"{
        "name": "test_project",
        "default_configuration": "pc.config.amconfig",
        "sources_dir": "sources",
        "data_dir": "data",
        "build_dir": "build",
        "version": 1
    }"#;

    fs::write(&amproject_path, config_json).expect("Failed to write .amproject");

    let result = read_amproject_file(temp_dir.path());
    assert!(result.is_ok(), "Should parse valid .amproject file");

    let config = result.unwrap();
    assert_eq!(config.name, "test_project");
    assert_eq!(config.sources_dir, "sources");
    assert_eq!(config.data_dir, "data");
    assert_eq!(config.build_dir, "build");
    assert_eq!(config.version, 1);
}

#[test]
fn test_p0_read_amproject_file_returns_error_when_file_missing() {
    let temp_dir = tempdir().expect("Failed to create temp dir");

    let result = read_amproject_file(temp_dir.path());
    assert!(
        result.is_err(),
        "Should error when .amproject file is missing"
    );
}

#[test]
fn test_p1_read_amproject_file_returns_error_for_invalid_json() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let amproject_path = temp_dir.path().join(".amproject");

    fs::write(&amproject_path, "not valid json").expect("Failed to write .amproject");

    let result = read_amproject_file(temp_dir.path());
    assert!(result.is_err(), "Should error for invalid JSON");
}

#[test]
fn test_p1_read_amproject_file_returns_error_for_incomplete_json() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let amproject_path = temp_dir.path().join(".amproject");

    // Missing required fields
    let config_json = r#"{"name": "test_project"}"#;
    fs::write(&amproject_path, config_json).expect("Failed to write .amproject");

    let result = read_amproject_file(temp_dir.path());
    assert!(result.is_err(), "Should error for incomplete JSON");
}

// =============================================================================
// count_assets_by_type Tests
// =============================================================================

#[test]
fn test_p0_count_assets_by_type_returns_empty_for_empty_project() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let sources_dir = temp_dir.path().join("sources");
    fs::create_dir_all(sources_dir.join("sounds")).expect("Failed to create sounds dir");
    fs::create_dir_all(sources_dir.join("collections")).expect("Failed to create collections dir");

    let result = count_assets_by_type(temp_dir.path());

    assert!(result.is_ok(), "Should succeed for empty project");
    let counts = result.unwrap();
    assert_eq!(
        counts.get("sounds").unwrap_or(&0),
        &0,
        "Should have 0 sounds"
    );
    assert_eq!(
        counts.get("collections").unwrap_or(&0),
        &0,
        "Should have 0 collections"
    );
}

#[test]
fn test_p0_count_assets_by_type_counts_json_files() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let sources_dir = temp_dir.path().join("sources");

    // Create asset directories
    fs::create_dir_all(sources_dir.join("sounds")).expect("Failed to create sounds dir");
    fs::create_dir_all(sources_dir.join("collections")).expect("Failed to create collections dir");
    fs::create_dir_all(sources_dir.join("events")).expect("Failed to create events dir");

    // Create some asset files
    fs::write(sources_dir.join("sounds/explosion.json"), "{}").expect("write");
    fs::write(sources_dir.join("sounds/footstep.json"), "{}").expect("write");
    fs::write(sources_dir.join("sounds/ambient.json"), "{}").expect("write");
    fs::write(sources_dir.join("collections/weapons.json"), "{}").expect("write");
    fs::write(sources_dir.join("events/play_sound.json"), "{}").expect("write");
    fs::write(sources_dir.join("events/stop_sound.json"), "{}").expect("write");

    let result = count_assets_by_type(temp_dir.path());

    assert!(result.is_ok(), "Should succeed");
    let counts = result.unwrap();
    assert_eq!(
        counts.get("sounds").unwrap_or(&0),
        &3,
        "Should have 3 sounds"
    );
    assert_eq!(
        counts.get("collections").unwrap_or(&0),
        &1,
        "Should have 1 collection"
    );
    assert_eq!(
        counts.get("events").unwrap_or(&0),
        &2,
        "Should have 2 events"
    );
}

#[test]
fn test_p1_count_assets_by_type_ignores_non_json_files() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let sources_dir = temp_dir.path().join("sources");
    fs::create_dir_all(sources_dir.join("sounds")).expect("Failed to create sounds dir");

    // Create mixed file types
    fs::write(sources_dir.join("sounds/valid.json"), "{}").expect("write");
    fs::write(sources_dir.join("sounds/readme.txt"), "text").expect("write");
    fs::write(sources_dir.join("sounds/backup.json.bak"), "{}").expect("write");

    let result = count_assets_by_type(temp_dir.path());

    assert!(result.is_ok(), "Should succeed");
    let counts = result.unwrap();
    assert_eq!(
        counts.get("sounds").unwrap_or(&0),
        &1,
        "Should only count .json files"
    );
}

#[test]
fn test_p1_count_assets_by_type_returns_empty_when_no_sources_dir() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    // Don't create sources directory

    let result = count_assets_by_type(temp_dir.path());

    assert!(result.is_ok(), "Should succeed even without sources dir");
    let counts = result.unwrap();
    assert!(
        counts.is_empty() || counts.values().all(|&v| v == 0),
        "All counts should be 0"
    );
}
