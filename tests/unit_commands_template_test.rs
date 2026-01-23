//! Unit tests for template commands module.
//!
//! Tests the Template entity, TemplateSource enum, EMBEDDED_TEMPLATES constant,
//! EmbeddedTemplate::to_template() conversion, and serialization behavior.

use am::commands::template::{EMBEDDED_TEMPLATES, EmbeddedTemplate};
use am::database::entities::{Template, TemplateSource};

// =============================================================================
// TemplateSource Display Tests
// =============================================================================

#[test]
fn test_p1_template_source_display_embedded() {
    // GIVEN: An embedded template source
    let source = TemplateSource::Embedded;

    // WHEN: We format it for display
    let display = format!("{}", source);

    // THEN: It should show "Embedded"
    assert_eq!(display, "Embedded");
}

#[test]
fn test_p1_template_source_display_custom() {
    // GIVEN: A custom template source
    let source = TemplateSource::Custom;

    // WHEN: We format it for display
    let display = format!("{}", source);

    // THEN: It should show "Custom"
    assert_eq!(display, "Custom");
}

#[test]
fn test_p1_template_source_default_is_embedded() {
    // GIVEN: The default TemplateSource
    let source = TemplateSource::default();

    // THEN: It should be Embedded
    assert_eq!(source, TemplateSource::Embedded);
}

// =============================================================================
// TemplateSource Equality Tests
// =============================================================================

#[test]
fn test_p2_template_source_equality() {
    // GIVEN: Two template sources of the same type
    let source1 = TemplateSource::Embedded;
    let source2 = TemplateSource::Embedded;

    // THEN: They should be equal
    assert_eq!(source1, source2);

    // AND: Different sources should not be equal
    let source3 = TemplateSource::Custom;
    assert_ne!(source1, source3);
}

// =============================================================================
// Template Serialization Tests
// =============================================================================

#[test]
fn test_p1_template_with_metadata_serializes() {
    // GIVEN: A template with all metadata fields
    let template = Template {
        id: Some(1),
        name: "test-template".to_string(),
        path: "/templates/test".to_string(),
        engine: Some("o3de".to_string()),
        description: Some("Test template for O3DE".to_string()),
        source: TemplateSource::Custom,
    };

    // WHEN: We serialize to JSON
    let json = serde_json::to_string(&template);

    // THEN: Serialization should succeed
    assert!(json.is_ok());
    let json_str = json.unwrap();

    // AND: All fields should be present with correct casing
    assert!(json_str.contains("\"name\":\"test-template\""));
    assert!(json_str.contains("\"engine\":\"o3de\""));
    assert!(json_str.contains("\"description\":\"Test template for O3DE\""));
    assert!(
        json_str.contains("\"source\":\"custom\""),
        "Source should be snake_case in JSON"
    );
}

#[test]
fn test_p2_template_with_none_engine_serializes() {
    // GIVEN: A template with None engine
    let template = Template {
        id: None,
        name: "minimal".to_string(),
        path: "bundled".to_string(),
        engine: None,
        description: None,
        source: TemplateSource::Embedded,
    };

    // WHEN: We serialize to JSON
    let json = serde_json::to_string(&template);

    // THEN: Serialization should succeed
    assert!(json.is_ok());
}

#[test]
fn test_p1_template_source_serializes_to_snake_case() {
    // GIVEN: Template sources
    let embedded = TemplateSource::Embedded;
    let custom = TemplateSource::Custom;

    // WHEN: We serialize them
    let embedded_json = serde_json::to_string(&embedded).unwrap();
    let custom_json = serde_json::to_string(&custom).unwrap();

    // THEN: They should be snake_case (serde rename_all)
    assert_eq!(embedded_json, "\"embedded\"");
    assert_eq!(custom_json, "\"custom\"");
}

#[test]
fn test_p1_template_source_deserializes_from_snake_case() {
    // GIVEN: Lowercase JSON strings
    let embedded_str = "\"embedded\"";
    let custom_str = "\"custom\"";

    // WHEN: We deserialize them
    let embedded: TemplateSource = serde_json::from_str(embedded_str).unwrap();
    let custom: TemplateSource = serde_json::from_str(custom_str).unwrap();

    // THEN: They should match the expected values
    assert_eq!(embedded, TemplateSource::Embedded);
    assert_eq!(custom, TemplateSource::Custom);
}

// =============================================================================
// Template Display Tests
// =============================================================================

#[test]
fn test_p2_template_display_with_id() {
    // GIVEN: A template with an id
    let template = Template {
        id: Some(1),
        name: "my-template".to_string(),
        path: "/path/to/template".to_string(),
        engine: Some("generic".to_string()),
        description: Some("A template".to_string()),
        source: TemplateSource::Custom,
    };

    // WHEN: We format it for display
    let display = format!("{}", template);

    // THEN: It should show "name (path)"
    assert_eq!(display, "my-template (/path/to/template)");
}

#[test]
fn test_p2_template_display_without_id() {
    // GIVEN: A template without an id (embedded template)
    let template = Template {
        id: None,
        name: "default".to_string(),
        path: "bundled".to_string(),
        engine: Some("generic".to_string()),
        description: Some("Default template".to_string()),
        source: TemplateSource::Embedded,
    };

    // WHEN: We format it for display
    let display = format!("{}", template);

    // THEN: It should show just the name
    assert_eq!(display, "default");
}

// =============================================================================
// Template Clone/Debug Tests
// =============================================================================

#[test]
fn test_p2_template_clone() {
    // GIVEN: A template
    let original = Template {
        id: Some(1),
        name: "original".to_string(),
        path: "/path".to_string(),
        engine: Some("o3de".to_string()),
        description: Some("Original template".to_string()),
        source: TemplateSource::Custom,
    };

    // WHEN: We clone it
    let cloned = original.clone();

    // THEN: All fields should be equal
    assert_eq!(original.id, cloned.id);
    assert_eq!(original.name, cloned.name);
    assert_eq!(original.path, cloned.path);
    assert_eq!(original.engine, cloned.engine);
    assert_eq!(original.description, cloned.description);
    assert_eq!(original.source, cloned.source);
}

#[test]
fn test_p2_template_debug() {
    // GIVEN: A template
    let template = Template {
        id: Some(1),
        name: "debug-test".to_string(),
        path: "/path".to_string(),
        engine: Some("generic".to_string()),
        description: None,
        source: TemplateSource::Embedded,
    };

    // WHEN: We format with debug
    let debug = format!("{:?}", template);

    // THEN: It should contain the struct name and field values
    assert!(debug.contains("Template"));
    assert!(debug.contains("debug-test"));
}

// =============================================================================
// EMBEDDED_TEMPLATES Constant Tests
// =============================================================================

#[test]
fn test_p0_embedded_templates_contains_default() {
    // GIVEN: The EMBEDDED_TEMPLATES constant

    // THEN: It should contain at least one template
    assert!(
        !EMBEDDED_TEMPLATES.is_empty(),
        "EMBEDDED_TEMPLATES should not be empty"
    );

    // AND: The first template should be named "default"
    let first = &EMBEDDED_TEMPLATES[0];
    assert_eq!(
        first.name, "default",
        "First embedded template should be 'default'"
    );
}

#[test]
fn test_p1_embedded_templates_default_has_correct_fields() {
    // GIVEN: The default embedded template
    let default_template = EMBEDDED_TEMPLATES
        .iter()
        .find(|t| t.name == "default")
        .expect("Default template should exist");

    // THEN: It should have the expected field values
    assert_eq!(default_template.engine, "generic");
    assert_eq!(
        default_template.description,
        "Default project template for any engine"
    );
}

#[test]
fn test_p1_embedded_templates_all_have_required_fields() {
    // GIVEN: All embedded templates
    for template in EMBEDDED_TEMPLATES {
        // THEN: Each should have non-empty required fields
        assert!(
            !template.name.is_empty(),
            "Template name should not be empty"
        );
        assert!(
            !template.engine.is_empty(),
            "Template engine should not be empty"
        );
        assert!(
            !template.description.is_empty(),
            "Template description should not be empty"
        );
    }
}

#[test]
fn test_p1_embedded_templates_names_are_unique() {
    // GIVEN: All embedded templates
    let names: Vec<&str> = EMBEDDED_TEMPLATES.iter().map(|t| t.name).collect();

    // THEN: All names should be unique
    let mut seen = std::collections::HashSet::new();
    for name in &names {
        assert!(seen.insert(*name), "Duplicate template name: {}", name);
    }
}

// =============================================================================
// EmbeddedTemplate::to_template() Conversion Tests
// =============================================================================

#[test]
fn test_p0_embedded_template_to_template_conversion() {
    // GIVEN: An embedded template definition
    let embedded = EmbeddedTemplate {
        name: "test-template",
        engine: "test-engine",
        description: "Test description",
        config_options: &[],
    };

    // WHEN: We convert to Template struct
    let template = embedded.to_template();

    // THEN: All fields should be correctly mapped
    assert_eq!(template.name, "test-template");
    assert_eq!(template.engine, Some("test-engine".to_string()));
    assert_eq!(template.description, Some("Test description".to_string()));
    assert_eq!(template.source, TemplateSource::Embedded);
    assert_eq!(template.path, "bundled");
    assert!(
        template.id.is_none(),
        "Embedded templates should have no database id"
    );
}

#[test]
fn test_p1_embedded_template_to_template_preserves_all_fields() {
    // GIVEN: The default embedded template from constants
    let default_embedded = &EMBEDDED_TEMPLATES[0];

    // WHEN: We convert to Template
    let template = default_embedded.to_template();

    // THEN: All fields should match the embedded definition
    assert_eq!(template.name, default_embedded.name);
    assert_eq!(template.engine.as_deref(), Some(default_embedded.engine));
    assert_eq!(
        template.description.as_deref(),
        Some(default_embedded.description)
    );
    assert_eq!(template.source, TemplateSource::Embedded);
}

#[test]
fn test_p2_embedded_template_is_clone_and_eq() {
    // GIVEN: An embedded template
    let original = EmbeddedTemplate {
        name: "clone-test",
        engine: "generic",
        description: "Clone test",
        config_options: &[],
    };

    // WHEN: We clone it
    let cloned = original.clone();

    // THEN: Clone should equal original
    assert_eq!(original, cloned);
}

#[test]
fn test_p2_embedded_template_debug() {
    // GIVEN: An embedded template
    let template = EmbeddedTemplate {
        name: "debug-test",
        engine: "generic",
        description: "Debug test template",
        config_options: &[],
    };

    // WHEN: We format with debug
    let debug = format!("{:?}", template);

    // THEN: It should contain the struct name and field values
    assert!(debug.contains("EmbeddedTemplate"));
    assert!(debug.contains("debug-test"));
}

// =============================================================================
// Template Info Command - Embedded Template File Enumeration Tests
// =============================================================================

#[test]
fn test_p0_get_embedded_template_files_default() {
    use am::commands::template::get_embedded_template_files;

    // GIVEN: The "default" embedded template name
    let template_name = "default";

    // WHEN: We enumerate its files
    let files = get_embedded_template_files(template_name);

    // THEN: Should return a non-empty list
    assert!(!files.is_empty(), "Default template should have files");

    // AND: All files should start with "default."
    for file in &files {
        assert!(
            file.starts_with("default."),
            "File '{}' should start with 'default.'",
            file
        );
    }
}

#[test]
fn test_p1_get_embedded_template_files_returns_all_default_files() {
    use am::commands::template::get_embedded_template_files;

    // GIVEN: The "default" embedded template
    let files = get_embedded_template_files("default");

    // THEN: Should contain the expected default template files
    assert!(
        files.contains(&"default.buses.json".to_string()),
        "Should contain default.buses.json"
    );
    assert!(
        files.contains(&"default.config.json".to_string()),
        "Should contain default.config.json"
    );
    assert!(
        files.contains(&"default.pipeline.json".to_string()),
        "Should contain default.pipeline.json"
    );
    assert!(
        files.contains(&"default.source.json".to_string()),
        "Should contain default.source.json"
    );
}

#[test]
fn test_p2_get_embedded_template_files_nonexistent_returns_empty() {
    use am::commands::template::get_embedded_template_files;

    // GIVEN: A non-existent template name
    let template_name = "nonexistent-template";

    // WHEN: We enumerate its files
    let files = get_embedded_template_files(template_name);

    // THEN: Should return an empty list
    assert!(
        files.is_empty(),
        "Non-existent template should have no files"
    );
}

// =============================================================================
// Template Config Options Tests
// =============================================================================

#[test]
fn test_p1_embedded_templates_have_config_options_field() {
    use am::commands::template::EMBEDDED_TEMPLATES;

    // GIVEN: All embedded templates
    for template in EMBEDDED_TEMPLATES {
        // THEN: Each should have a config_options field (may be empty)
        // Just accessing the field verifies it exists
        let _ = template.config_options;
    }
}

#[test]
fn test_p2_embedded_template_with_config_options() {
    use am::commands::template::{EmbeddedTemplate, TemplateConfigOption};

    // GIVEN: A template with configuration options
    let config_opts = &[TemplateConfigOption {
        name: "sample_rate",
        description: "Audio sample rate in Hz",
        default_value: "44100",
    }];

    let template = EmbeddedTemplate {
        name: "test-with-config",
        engine: "generic",
        description: "Template with config options",
        config_options: config_opts,
    };

    // THEN: Config options should be accessible
    assert_eq!(template.config_options.len(), 1);
    assert_eq!(template.config_options[0].name, "sample_rate");
    assert_eq!(template.config_options[0].default_value, "44100");
}

// =============================================================================
// Custom Template File Enumeration Tests
// =============================================================================

#[test]
fn test_p0_get_embedded_template_files_is_sorted() {
    use am::commands::template::get_embedded_template_files;

    // GIVEN: The "default" embedded template
    let files = get_embedded_template_files("default");

    // THEN: Files should be sorted alphabetically
    let mut sorted_files = files.clone();
    sorted_files.sort();
    assert_eq!(
        files, sorted_files,
        "Embedded template files should be sorted for deterministic output"
    );
}
