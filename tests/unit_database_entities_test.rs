//! Unit tests for database entities module.

use am::database::entities::{Project, ProjectConfiguration, Template};

// =============================================================================
// ProjectConfiguration Tests
// =============================================================================

#[test]
fn test_p1_project_configuration_to_project_converts_correctly() {
    let config = ProjectConfiguration {
        name: "my_project".to_string(),
        default_configuration: "pc.config.amconfig".to_string(),
        sources_dir: "sources".to_string(),
        data_dir: "data".to_string(),
        build_dir: "build".to_string(),
        version: 1,
    };

    let project = config.to_project("/path/to/project");

    assert_eq!(project.name, "my_project");
    assert_eq!(project.path, "/path/to/project");
    assert!(project.id.is_none(), "New project should have no ID");
}

#[test]
fn test_p1_project_configuration_default_has_empty_strings() {
    let config = ProjectConfiguration::default();

    assert!(config.name.is_empty());
    assert!(config.default_configuration.is_empty());
    assert!(config.sources_dir.is_empty());
    assert!(config.data_dir.is_empty());
    assert!(config.build_dir.is_empty());
    assert_eq!(config.version, 0);
}

#[test]
fn test_p2_project_configuration_serializes_to_json() {
    let config = ProjectConfiguration {
        name: "test_project".to_string(),
        default_configuration: "pc.config.amconfig".to_string(),
        sources_dir: "sources".to_string(),
        data_dir: "data".to_string(),
        build_dir: "build".to_string(),
        version: 1,
    };

    let json = serde_json::to_string(&config);

    assert!(json.is_ok(), "Serialization should succeed");
    let json_str = json.unwrap();
    assert!(json_str.contains("\"name\""));
    assert!(json_str.contains("\"default_configuration\""));
    assert!(json_str.contains("\"sources_dir\""));
}

#[test]
fn test_p2_project_configuration_deserializes_from_json() {
    let json = r#"{
        "name": "deserialized_project",
        "default_configuration": "custom.config",
        "sources_dir": "src",
        "data_dir": "assets",
        "build_dir": "out",
        "version": 2
    }"#;

    let config: Result<ProjectConfiguration, _> = serde_json::from_str(json);

    assert!(config.is_ok(), "Deserialization should succeed");
    let config = config.unwrap();
    assert_eq!(config.name, "deserialized_project");
    assert_eq!(config.version, 2);
}

// =============================================================================
// Project Tests
// =============================================================================

#[test]
fn test_p1_project_default_has_no_id() {
    let project = Project::default();

    assert!(project.id.is_none());
    assert!(project.name.is_empty());
    assert!(project.path.is_empty());
}

#[test]
fn test_p2_project_clone_creates_independent_copy() {
    let project = Project {
        id: Some(42),
        name: "original".to_string(),
        path: "/original/path".to_string(),
    };

    let cloned = project.clone();

    assert_eq!(cloned.id, project.id);
    assert_eq!(cloned.name, project.name);
    assert_eq!(cloned.path, project.path);
}

#[test]
fn test_p2_project_serializes_to_json() {
    let project = Project {
        id: Some(1),
        name: "test".to_string(),
        path: "/test/path".to_string(),
    };

    let json = serde_json::to_string(&project);

    assert!(json.is_ok());
    let json_str = json.unwrap();
    assert!(json_str.contains("\"id\":1"));
    assert!(json_str.contains("\"name\":\"test\""));
}

// =============================================================================
// Template Tests
// =============================================================================

#[test]
fn test_p1_template_display_with_id_shows_path() {
    let template = Template {
        id: Some(1),
        name: "custom_template".to_string(),
        path: "/templates/custom".to_string(),
    };

    let display = format!("{}", template);

    assert_eq!(display, "custom_template (/templates/custom)");
}

#[test]
fn test_p1_template_display_without_id_shows_name_only() {
    let template = Template {
        id: None,
        name: "default".to_string(),
        path: "bundled".to_string(),
    };

    let display = format!("{}", template);

    assert_eq!(display, "default");
}

#[test]
fn test_p2_template_clone_creates_independent_copy() {
    let template = Template {
        id: Some(5),
        name: "original_template".to_string(),
        path: "/original/template/path".to_string(),
    };

    let cloned = template.clone();

    assert_eq!(cloned.id, template.id);
    assert_eq!(cloned.name, template.name);
    assert_eq!(cloned.path, template.path);
}
