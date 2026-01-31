//! Integration tests for ProjectValidator cross-asset reference validation.
//!
//! These tests exercise ProjectValidator in more realistic scenarios,
//! including interaction with ProjectContext and Sound::validate_rules.

mod common;

use am::assets::Asset;
use am::assets::{AssetType, ProjectContext, ProjectValidator, Sound};
use serde_json::json;
use std::fs;
use tempfile::tempdir;

/// Helper: create a sound JSON file.
fn write_sound_json(sounds_dir: &std::path::Path, filename: &str, id: u64, name: &str) {
    let sound_json = json!({
        "id": id,
        "name": name,
        "path": format!("data/{}.wav", name),
        "bus": 0,
        "gain": { "kind": "Static", "value": 1.0 },
        "priority": { "kind": "Static", "value": 128.0 },
        "stream": false,
        "loop": { "enabled": false, "loop_count": 0 },
        "spatialization": "None",
        "attenuation": 0,
        "scope": "World",
        "fader": "Linear",
        "effect": 0
    });
    fs::write(
        sounds_dir.join(filename),
        serde_json::to_string_pretty(&sound_json).unwrap(),
    )
    .unwrap();
}

/// Helper: create a minimal asset JSON file (just id and name).
fn write_minimal_asset_json(dir: &std::path::Path, filename: &str, id: u64, name: &str) {
    let json = json!({ "id": id, "name": name });
    fs::write(
        dir.join(filename),
        serde_json::to_string_pretty(&json).unwrap(),
    )
    .unwrap();
}

// =============================================================================
// Integration tests: ProjectValidator with disk scanning
// =============================================================================

#[test]
fn test_p0_validator_scans_multiple_asset_types() {
    let dir = tempdir().unwrap();

    // Create assets of various types
    let sounds_dir = dir.path().join("sources/sounds");
    let effects_dir = dir.path().join("sources/effects");
    let collections_dir = dir.path().join("sources/collections");
    let switches_dir = dir.path().join("sources/switches");
    let events_dir = dir.path().join("sources/events");

    for d in [
        &sounds_dir,
        &effects_dir,
        &collections_dir,
        &switches_dir,
        &events_dir,
    ] {
        fs::create_dir_all(d).unwrap();
    }

    write_sound_json(&sounds_dir, "footstep.json", 1, "footstep");
    write_sound_json(&sounds_dir, "explosion.json", 2, "explosion");
    write_minimal_asset_json(&effects_dir, "reverb.json", 10, "reverb");
    write_minimal_asset_json(&effects_dir, "delay.json", 11, "delay");
    write_minimal_asset_json(&collections_dir, "footsteps.json", 20, "footsteps");
    write_minimal_asset_json(&switches_dir, "surface.json", 30, "surface");
    write_minimal_asset_json(&events_dir, "play_music.json", 40, "play_music");

    let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();

    // Sounds
    assert!(validator.validate_sound_exists(1).is_ok());
    assert!(validator.validate_sound_exists(2).is_ok());
    assert!(validator.validate_sound_exists(999).is_err());

    // Effects
    assert!(validator.validate_effect_exists(10).is_ok());
    assert!(validator.validate_effect_exists(11).is_ok());
    assert!(validator.validate_effect_exists(999).is_err());

    // Collections
    assert!(validator.validate_collection_exists(20).is_ok());
    assert!(validator.validate_collection_exists(999).is_err());

    // Switches
    assert!(validator.validate_switch_exists(30).is_ok());
    assert!(validator.validate_switch_exists(999).is_err());

    // Events (via generic method)
    assert!(
        validator
            .validate_asset_exists(AssetType::Event, 40)
            .is_ok()
    );
    assert!(
        validator
            .validate_asset_exists(AssetType::Event, 999)
            .is_err()
    );
}

#[test]
fn test_p0_validator_integrated_with_project_context() {
    let dir = tempdir().unwrap();

    // Set up project with sounds and effects
    let sounds_dir = dir.path().join("sources/sounds");
    let effects_dir = dir.path().join("sources/effects");
    fs::create_dir_all(&sounds_dir).unwrap();
    fs::create_dir_all(&effects_dir).unwrap();

    write_sound_json(&sounds_dir, "beep.json", 1, "beep");
    write_minimal_asset_json(&effects_dir, "reverb.json", 10, "reverb");

    let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();
    let context = ProjectContext::new(dir.path().to_path_buf()).with_validator(validator);

    // Verify context has validator
    assert!(context.validator.is_some());
}

#[test]
fn test_p0_sound_validate_rules_checks_effect_reference_when_validator_present() {
    let dir = tempdir().unwrap();

    // Create data and source dirs
    let data_dir = dir.path().join("data/sfx");
    let sounds_dir = dir.path().join("sources/sounds");
    let effects_dir = dir.path().join("sources/effects");
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(&sounds_dir).unwrap();
    fs::create_dir_all(&effects_dir).unwrap();

    // Create an audio file and an effect
    fs::write(data_dir.join("beep.wav"), b"fake audio").unwrap();
    write_minimal_asset_json(&effects_dir, "reverb.json", 10, "reverb");

    let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();
    let context = ProjectContext::new(dir.path().to_path_buf()).with_validator(validator);

    // Sound referencing a valid effect -> Ok
    let sound_valid = Sound::builder(1, "beep")
        .path("sfx/beep.wav")
        .effect(10)
        .build();
    assert!(sound_valid.validate_rules(&context).is_ok());

    // Sound referencing a non-existent effect -> Err
    let sound_invalid = Sound::builder(2, "beep2")
        .path("sfx/beep.wav")
        .effect(999)
        .build();
    let result = sound_invalid.validate_rules(&context);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.what().contains("Effect"));
    assert!(err.field.as_deref() == Some("effect"));

    // Sound with effect = 0 (no reference) -> Ok
    let sound_no_effect = Sound::builder(3, "beep3")
        .path("sfx/beep.wav")
        .effect(0)
        .build();
    assert!(sound_no_effect.validate_rules(&context).is_ok());
}

#[test]
fn test_p0_sound_validate_rules_skips_effect_check_without_validator() {
    let dir = tempdir().unwrap();

    // Create data dir with audio file
    let data_dir = dir.path().join("data/sfx");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("beep.wav"), b"fake audio").unwrap();

    // Context WITHOUT validator
    let context = ProjectContext::new(dir.path().to_path_buf());
    assert!(context.validator.is_none());

    // Sound referencing a non-existent effect -> Ok (no validator to check)
    let sound = Sound::builder(1, "beep")
        .path("sfx/beep.wav")
        .effect(999)
        .build();
    assert!(sound.validate_rules(&context).is_ok());
}

#[test]
fn test_p1_validator_handles_empty_project_gracefully() {
    let dir = tempdir().unwrap();
    // No sources directory at all

    let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();

    // All checks should fail for non-zero IDs
    assert!(validator.validate_sound_exists(1).is_err());
    assert!(validator.validate_effect_exists(1).is_err());
    assert!(validator.validate_collection_exists(1).is_err());
    assert!(validator.validate_switch_exists(1).is_err());

    // Zero IDs should always pass
    assert!(validator.validate_sound_exists(0).is_ok());
    assert!(validator.validate_effect_exists(0).is_ok());
}

#[test]
fn test_p1_validator_handles_partial_directories() {
    let dir = tempdir().unwrap();

    // Only sounds directory exists
    let sounds_dir = dir.path().join("sources/sounds");
    fs::create_dir_all(&sounds_dir).unwrap();
    write_sound_json(&sounds_dir, "test.json", 42, "test");

    let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();

    // Sound exists
    assert!(validator.validate_sound_exists(42).is_ok());

    // Other types have no assets (missing dirs), but don't error on construction
    assert!(validator.validate_effect_exists(42).is_err());
    assert!(validator.validate_collection_exists(42).is_err());
}

#[test]
fn test_p1_validator_skips_malformed_and_non_json_files() {
    let dir = tempdir().unwrap();
    let sounds_dir = dir.path().join("sources/sounds");
    fs::create_dir_all(&sounds_dir).unwrap();

    // Valid sound
    write_sound_json(&sounds_dir, "valid.json", 42, "valid_sound");

    // Malformed JSON
    fs::write(sounds_dir.join("broken.json"), "NOT JSON AT ALL").unwrap();

    // Non-JSON file (should be ignored)
    fs::write(sounds_dir.join("notes.txt"), "this is a text file").unwrap();

    // JSON missing id field
    fs::write(
        sounds_dir.join("no_id.json"),
        r#"{"name": "orphan", "path": "data/orphan.wav"}"#,
    )
    .unwrap();

    // Subdirectory (should be ignored)
    fs::create_dir_all(sounds_dir.join("subdir")).unwrap();

    let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();

    // Only the valid sound should be found
    assert!(validator.validate_sound_exists(42).is_ok());
    assert!(validator.validate_sound_exists(0).is_ok()); // Zero is always ok
}

#[test]
fn test_p1_zero_id_references_are_noop_for_all_types() {
    let validator = ProjectValidator::empty();

    // Zero means "no reference" in SDK convention - always returns Ok
    assert!(validator.validate_sound_exists(0).is_ok());
    assert!(validator.validate_collection_exists(0).is_ok());
    assert!(validator.validate_effect_exists(0).is_ok());
    assert!(validator.validate_switch_exists(0).is_ok());
    assert!(validator.validate_switch_state_exists(0, 0).is_ok());
    assert!(validator.validate_switch_state_exists(0, 999).is_ok());
    assert!(
        validator
            .validate_asset_exists(AssetType::Soundbank, 0)
            .is_ok()
    );
    assert!(validator.validate_asset_exists(AssetType::Event, 0).is_ok());
    assert!(
        validator
            .validate_asset_exists(AssetType::SwitchContainer, 0)
            .is_ok()
    );
}

#[test]
fn test_p2_validation_error_message_quality() {
    let validator = ProjectValidator::empty();

    // Test error for missing sound
    let err = validator.validate_sound_exists(12345).unwrap_err();
    assert!(err.what().contains("Sound"));
    assert!(err.what().contains("12345"));
    assert!(err.why().contains("does not exist"));
    assert!(!err.suggestion().is_empty());

    // Test error for missing effect
    let err = validator.validate_effect_exists(99).unwrap_err();
    assert!(err.what().contains("Effect"));
    assert!(err.what().contains("99"));

    // Test error for missing collection
    let err = validator.validate_collection_exists(77).unwrap_err();
    assert!(err.what().contains("Collection"));
    assert!(err.what().contains("77"));
}
