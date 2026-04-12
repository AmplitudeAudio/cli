// Copyright (c) 2026-present Sparky Studios. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Integration tests for edge case standardization (Story 2b-5).
//!
//! Tests cover:
//! - Error code mappings (AC #7)
//! - Type-directory mismatch detection (AC #8)
//! - ProjectContext population via with_validator (AC #2)
//! - ID uniqueness checking (AC #1)
//! - Name uniqueness via registry (AC #4)
//! - Missing audio file warnings in list (AC #3)
//! - Missing/unreadable directory handling in list (AC #6)
//! - Non-interactive defaults vs invalid values (AC #5)

mod common;

use am::assets::{AssetType, ProjectContext};
use am::commands::asset::{SoundCommands, handle_sound_command};
use am::common::errors::codes;
use am::input::NonInteractiveInput;
use common::fixtures::{AssetTestFixture, CaptureOutput, TestProjectFixture};
use serde_json::json;

// =============================================================================
// AC #7: Error code tests
// =============================================================================

#[test]
fn test_p0_empty_reference_error_code_is_31005() {
    assert_eq!(codes::ERR_VALIDATION_EMPTY_REFERENCE, -31005);
}

#[test]
fn test_p0_circular_reference_error_code_is_31006() {
    assert_eq!(codes::ERR_VALIDATION_CIRCULAR_REFERENCE, -31006);
}

#[test]
fn test_p0_new_error_codes_in_validation_range() {
    assert!((-31999..=-31000).contains(&codes::ERR_VALIDATION_EMPTY_REFERENCE));
    assert!((-31999..=-31000).contains(&codes::ERR_VALIDATION_CIRCULAR_REFERENCE));
}

// =============================================================================
// AC #2: ProjectContext populated with ProjectValidator
// =============================================================================

#[test]
fn test_p0_with_validator_populates_id_registry() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    fixture.create_test_sound("footstep", 42).unwrap();
    fixture.create_test_sound("explosion", 100).unwrap();

    let validator = fixture.create_project_validator().unwrap();
    let context =
        ProjectContext::new(fixture.project_root().to_path_buf()).with_validator(validator);

    assert!(context.has_id(42));
    assert!(context.has_id(100));
    assert!(!context.has_id(999));
}

#[test]
fn test_p0_with_validator_populates_name_registry() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    fixture.create_test_sound("footstep", 42).unwrap();

    let validator = fixture.create_project_validator().unwrap();
    let context =
        ProjectContext::new(fixture.project_root().to_path_buf()).with_validator(validator);

    assert!(context.has_name(AssetType::Sound, "footstep"));
    assert!(!context.has_name(AssetType::Sound, "nonexistent"));
    // Different type should not match
    assert!(!context.has_name(AssetType::Effect, "footstep"));
}

// =============================================================================
// AC #8: Type-directory mismatch detection
// =============================================================================

#[test]
fn test_p0_validator_detects_type_directory_mismatch() {
    let fixture = AssetTestFixture::new("test_project").unwrap();

    // Place a sound asset in the effects directory (wrong type)
    fixture
        .write_asset_json(
            "effects",
            "misplaced",
            json!({"id": 100, "name": "misplaced"}),
        )
        .unwrap();

    let validator = fixture.create_project_validator().unwrap();

    // ID 100 is in effects directory, so asking for it as Sound should fail
    let result = validator.validate_asset_exists(AssetType::Sound, 100);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.what().contains("Effect"));
    assert!(err.what().contains("not a Sound"));
}

#[test]
fn test_p0_validator_allows_correct_type_directory() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    fixture.create_test_sound("beep", 10).unwrap();

    let validator = fixture.create_project_validator().unwrap();
    assert!(
        validator
            .validate_asset_exists(AssetType::Sound, 10)
            .is_ok()
    );
}

#[test]
fn test_p1_validate_asset_in_correct_directory_explicit() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    fixture.create_test_sound("beep", 10).unwrap();
    fixture
        .write_asset_json("effects", "reverb", json!({"id": 20, "name": "reverb"}))
        .unwrap();

    let validator = fixture.create_project_validator().unwrap();

    // Correct directories
    assert!(
        validator
            .validate_asset_in_correct_directory(AssetType::Sound, 10)
            .is_ok()
    );
    assert!(
        validator
            .validate_asset_in_correct_directory(AssetType::Effect, 20)
            .is_ok()
    );

    // Wrong directory
    assert!(
        validator
            .validate_asset_in_correct_directory(AssetType::Effect, 10)
            .is_err()
    );

    // Zero ID always OK
    assert!(
        validator
            .validate_asset_in_correct_directory(AssetType::Sound, 0)
            .is_ok()
    );
}

// =============================================================================
// AC #1: Global ID uniqueness at create time
// =============================================================================

#[test]
fn test_p0_context_has_id_after_validator_population() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    fixture.create_test_sound("existing", 42).unwrap();

    let validator = fixture.create_project_validator().unwrap();
    let context =
        ProjectContext::new(fixture.project_root().to_path_buf()).with_validator(validator);

    assert!(context.has_id(42)); // Should find existing ID
    assert!(!context.has_id(0)); // Zero is not registered
    assert!(!context.has_id(99999)); // Non-existing ID
}

// =============================================================================
// AC #4: Name uniqueness via registry
// =============================================================================

#[test]
fn test_p0_context_has_name_after_validator_population() {
    let fixture = AssetTestFixture::new("test_project").unwrap();
    fixture.create_test_sound("explosion", 42).unwrap();

    let validator = fixture.create_project_validator().unwrap();
    let context =
        ProjectContext::new(fixture.project_root().to_path_buf()).with_validator(validator);

    assert!(context.has_name(AssetType::Sound, "explosion"));
    assert!(!context.has_name(AssetType::Sound, "does_not_exist"));
}

// =============================================================================
// AC #3: Missing audio file warnings in list
// =============================================================================

#[tokio::test]
async fn test_p1_list_warns_about_missing_audio_files() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory and a sound file
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory but WITHOUT the referenced audio file
    let data_dir = project_path.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();

    // Sound references sfx/missing.wav which doesn't exist
    let sound_json = json!({
        "id": 42,
        "name": "missing_audio",
        "path": "sfx/missing.wav",
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
    std::fs::write(
        sounds_dir.join("missing_audio.json"),
        serde_json::to_string_pretty(&sound_json).unwrap(),
    )
    .unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(
        result.is_ok(),
        "List should succeed even with missing audio"
    );

    let success_data = output.last_success().expect("Should have success output");
    let warnings = success_data["warnings"]
        .as_array()
        .expect("Should have warnings");
    assert!(!warnings.is_empty(), "Should have at least one warning");

    let warning_text = warnings[0].as_str().unwrap();
    assert!(
        warning_text.contains("missing_audio"),
        "Warning should mention sound name"
    );
    assert!(
        warning_text.contains("sfx/missing.wav"),
        "Warning should mention file path"
    );

    // Sound should still be listed
    assert_eq!(success_data["count"], 1, "Sound should still be listed");
}

// =============================================================================
// AC #6: Missing directory handling in list
// =============================================================================

#[tokio::test]
async fn test_p1_list_missing_sounds_dir_returns_info_message() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Do NOT create sources/sounds/ directory

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Should not fail for missing directory");

    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["count"], 0);

    let warnings = success_data["warnings"]
        .as_array()
        .expect("Should have warnings");
    assert!(!warnings.is_empty());
    assert!(
        warnings[0]
            .as_str()
            .unwrap()
            .contains("No sounds directory found")
    );
}

// =============================================================================
// AC #5: Non-interactive defaults
// =============================================================================

#[tokio::test]
async fn test_p1_create_rejects_invalid_gain_value() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create required directories
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("test.wav"), b"fake audio").unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Explicit invalid gain value should fail
    let result = handle_sound_command(
        &SoundCommands::Create {
            name: "test_sound".to_string(),
            file: Some("sfx/test.wav".to_string()),
            gain: Some(5.0), // Invalid: > 1.0
            bus: None,
            priority: None,
            stream: false,
            loop_enabled: false,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(
        result.is_err(),
        "Should reject explicitly invalid gain value"
    );
}

// =============================================================================
// Helpers
// =============================================================================

fn create_amproject(path: &std::path::Path, name: &str) -> anyhow::Result<()> {
    let config = serde_json::json!({
        "name": name,
        "default_configuration": "pc.config.amconfig",
        "sources_dir": "sources",
        "data_dir": "data",
        "build_dir": "build",
        "version": 1
    });
    std::fs::write(
        path.join(".amproject"),
        serde_json::to_string_pretty(&config)?,
    )?;
    Ok(())
}
