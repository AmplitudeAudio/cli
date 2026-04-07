//! Tests for AssetTestFixture in tests/common/fixtures.rs.
//!
//! Validates that the fixture creates the correct directory structure,
//! writes valid JSON for sound and collection assets, and integrates
//! with ProjectValidator for cross-asset validation.

mod common;

use am::assets::Sound;
use common::fixtures::AssetTestFixture;
use serde_json::Value;

// =============================================================================
// P1: AssetTestFixture construction tests
// =============================================================================

#[test]
fn test_p1_asset_fixture_new_creates_all_directories() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    let root = fixture.project_root();

    // Verify all SDK source directories exist
    assert!(root.join("sources/sounds").is_dir());
    assert!(root.join("sources/collections").is_dir());
    assert!(root.join("sources/effects").is_dir());
    assert!(root.join("sources/switches").is_dir());
    assert!(root.join("sources/switch_containers").is_dir());
    assert!(root.join("sources/soundbanks").is_dir());
    assert!(root.join("sources/events").is_dir());
    assert!(root.join("sources/attenuators").is_dir());
    assert!(root.join("sources/pipelines").is_dir());
    assert!(root.join("sources/rtpc").is_dir());

    // Verify additional project directories
    assert!(root.join("data").is_dir());
    assert!(root.join("build").is_dir());
    assert!(root.join("plugins").is_dir());

    // Verify .amproject exists and is valid JSON
    let amproject_path = root.join(".amproject");
    assert!(amproject_path.is_file());
    let content = std::fs::read_to_string(&amproject_path).unwrap();
    let config: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(config["name"], "test_project");
    assert_eq!(config["version"], 1);
}

#[test]
fn test_p1_asset_fixture_sources_dir_accessor() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    let expected = fixture.project_root().join("sources");
    assert_eq!(fixture.sources_dir(), expected);
    assert!(fixture.sources_dir().is_dir());
}

// =============================================================================
// P1: create_test_sound tests
// =============================================================================

#[test]
fn test_p1_create_test_sound_writes_valid_json() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    let path = fixture.create_test_sound("footstep", 42).unwrap();

    // File exists at expected location
    assert!(path.is_file());
    assert!(path.ends_with("sources/sounds/footstep.json"));

    // JSON is valid and has correct fields
    let content = std::fs::read_to_string(&path).unwrap();
    let json: Value = serde_json::from_str(&content).unwrap();

    assert_eq!(json["id"], 42);
    assert_eq!(json["name"], "footstep");
    assert_eq!(json["path"], "data/footstep.wav");
    assert_eq!(json["bus"], 0);
    assert_eq!(json["stream"], false);
    assert_eq!(json["spatialization"], "None");
    assert_eq!(json["scope"], "World");
    assert_eq!(json["fader"], "Linear");
    assert_eq!(json["effect"], 0);
    assert_eq!(json["attenuation"], 0);

    // Verify RtpcCompatibleValue format for gain
    assert_eq!(json["gain"]["kind"], "Static");
    assert_eq!(json["gain"]["value"], 1.0);

    // Verify RtpcCompatibleValue format for priority
    assert_eq!(json["priority"]["kind"], "Static");
    assert_eq!(json["priority"]["value"], 128.0);

    // Verify SoundLoopConfig format
    assert_eq!(json["loop"]["enabled"], false);
    assert_eq!(json["loop"]["loop_count"], 0);
}

#[test]
fn test_p1_create_test_sound_deserializes_to_sound_struct() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    let path = fixture.create_test_sound("explosion", 100).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let sound: Sound = serde_json::from_str(&content).unwrap();

    assert_eq!(sound.id, 100);
    assert_eq!(sound.name.as_deref(), Some("explosion"));
}

// =============================================================================
// P1: create_test_collection tests
// =============================================================================

#[test]
fn test_p1_create_test_collection_writes_valid_json() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    let path = fixture
        .create_test_collection("footsteps", 200, &[42, 43, 44])
        .unwrap();

    // File exists at expected location
    assert!(path.is_file());
    assert!(path.ends_with("sources/collections/footsteps.json"));

    // JSON is valid and has correct fields
    let content = std::fs::read_to_string(&path).unwrap();
    let json: Value = serde_json::from_str(&content).unwrap();

    assert_eq!(json["id"], 200);
    assert_eq!(json["name"], "footsteps");
    assert_eq!(json["mode"], "random");
    assert_eq!(json["scope"], "World");

    // Verify sound_ids array
    let sound_ids = json["sound_ids"].as_array().unwrap();
    assert_eq!(sound_ids.len(), 3);
    assert_eq!(sound_ids[0], 42);
    assert_eq!(sound_ids[1], 43);
    assert_eq!(sound_ids[2], 44);
}

// =============================================================================
// P1: write_asset_json tests
// =============================================================================

#[test]
fn test_p1_write_asset_json_writes_to_correct_directory() {
    let fixture = AssetTestFixture::new("test_project").unwrap();

    let asset_types = ["sounds", "collections", "effects", "switches", "events"];

    for asset_type in &asset_types {
        let json = serde_json::json!({ "id": 1, "name": "test" });
        let path = fixture
            .write_asset_json(asset_type, "test_asset", json)
            .unwrap();

        assert!(path.is_file());
        let expected_dir = format!("sources/{}/test_asset.json", asset_type);
        assert!(
            path.ends_with(&expected_dir),
            "Expected path ending with {}, got {}",
            expected_dir,
            path.display()
        );
    }
}

// =============================================================================
// P1: create_project_validator tests
// =============================================================================

#[test]
fn test_p1_create_project_validator_finds_created_assets() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    fixture.create_test_sound("footstep", 42).unwrap();
    fixture.create_test_sound("explosion", 100).unwrap();

    let validator = fixture.create_project_validator().unwrap();

    assert!(validator.validate_sound_exists(42).is_ok());
    assert!(validator.validate_sound_exists(100).is_ok());
    assert!(validator.validate_sound_exists(999).is_err());
}

// =============================================================================
// P1: create_data_file tests
// =============================================================================

#[test]
fn test_p1_create_data_file_creates_file_in_data_dir() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    let path = fixture.create_data_file("footstep.wav").unwrap();

    assert!(path.is_file());
    assert!(path.ends_with("data/footstep.wav"));
}
