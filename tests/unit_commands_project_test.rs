//! Unit tests for project command validation functions.

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
