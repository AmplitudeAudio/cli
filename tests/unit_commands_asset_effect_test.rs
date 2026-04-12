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

//! Unit tests for effect asset command.

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, EffectCommands};
use clap::Parser;

// =============================================================================
// Effect Create Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_effect_create_command_parses_with_name_only() {
    let args = ["am", "asset", "effect", "create", "reverb_hall"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command:
                        EffectCommands::Create {
                            name,
                            effect_type,
                            param,
                        },
                },
        } => {
            assert_eq!(name, "reverb_hall");
            assert!(effect_type.is_none());
            assert!(param.is_none());
        }
        _ => panic!("Expected Asset Effect Create command"),
    }
}

#[test]
fn test_p0_effect_create_command_parses_with_all_flags() {
    let args = [
        "am",
        "asset",
        "effect",
        "create",
        "reverb_hall",
        "--effect-type",
        "reverb",
        "--param",
        "0.8",
        "--param",
        "0.5",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command:
                        EffectCommands::Create {
                            name,
                            effect_type,
                            param,
                        },
                },
        } => {
            assert_eq!(name, "reverb_hall");
            assert_eq!(effect_type, Some("reverb".to_string()));
            let params = param.unwrap();
            assert_eq!(params.len(), 2);
            assert_eq!(params[0], 0.8);
            assert_eq!(params[1], 0.5);
        }
        _ => panic!("Expected Asset Effect Create command"),
    }
}

#[test]
fn test_p1_effect_create_command_requires_name() {
    let args = ["am", "asset", "effect", "create"];
    let result = App::try_parse_from(args);
    assert!(result.is_err(), "Should fail without name argument");
}

// =============================================================================
// Effect List Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_effect_list_command_parses() {
    let args = ["am", "asset", "effect", "list"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command: EffectCommands::List {},
                },
        } => {} // Success - it parsed correctly
        _ => panic!("Expected Asset Effect List command"),
    }
}

// =============================================================================
// Effect Update Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_effect_update_command_parses_with_name_only() {
    let args = ["am", "asset", "effect", "update", "reverb_hall"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command:
                        EffectCommands::Update {
                            name,
                            effect_type,
                            param,
                        },
                },
        } => {
            assert_eq!(name, "reverb_hall");
            assert!(effect_type.is_none());
            assert!(param.is_none());
        }
        _ => panic!("Expected Asset Effect Update command"),
    }
}

#[test]
fn test_p0_effect_update_command_parses_with_all_flags() {
    let args = [
        "am",
        "asset",
        "effect",
        "update",
        "reverb_hall",
        "--effect-type",
        "eq",
        "--param",
        "1.0",
        "--param",
        "0.3",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command:
                        EffectCommands::Update {
                            name,
                            effect_type,
                            param,
                        },
                },
        } => {
            assert_eq!(name, "reverb_hall");
            assert_eq!(effect_type, Some("eq".to_string()));
            let params = param.unwrap();
            assert_eq!(params.len(), 2);
            assert_eq!(params[0], 1.0);
            assert_eq!(params[1], 0.3);
        }
        _ => panic!("Expected Asset Effect Update command"),
    }
}

#[test]
fn test_p1_effect_update_command_requires_name() {
    let args = ["am", "asset", "effect", "update"];
    let result = App::try_parse_from(args);
    assert!(result.is_err(), "Should fail without name argument");
}

// =============================================================================
// Effect Delete Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_effect_delete_command_parses() {
    let args = ["am", "asset", "effect", "delete", "reverb_hall"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command: EffectCommands::Delete { name, force },
                },
        } => {
            assert_eq!(name, "reverb_hall");
            assert!(!force);
        }
        _ => panic!("Expected Asset Effect Delete command"),
    }
}

#[test]
fn test_p0_effect_delete_command_parses_with_force() {
    let args = ["am", "asset", "effect", "delete", "reverb_hall", "--force"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command: EffectCommands::Delete { name, force },
                },
        } => {
            assert_eq!(name, "reverb_hall");
            assert!(force);
        }
        _ => panic!("Expected Asset Effect Delete command"),
    }
}

#[test]
fn test_p1_effect_delete_command_requires_name() {
    let args = ["am", "asset", "effect", "delete"];
    let result = App::try_parse_from(args);
    assert!(result.is_err(), "Should fail without name argument");
}

// =============================================================================
// Non-Interactive Mode Tests
// =============================================================================

#[test]
fn test_p1_effect_create_with_non_interactive_flag() {
    let args = [
        "am",
        "--non-interactive",
        "asset",
        "effect",
        "create",
        "reverb_hall",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.non_interactive);
    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command: EffectCommands::Create { name, .. },
                },
        } => {
            assert_eq!(name, "reverb_hall");
        }
        _ => panic!("Expected Asset Effect Create command"),
    }
}

#[test]
fn test_p1_effect_create_with_json_flag() {
    let args = ["am", "--json", "asset", "effect", "create", "reverb_hall"];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.json);
    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Effect {
                    command: EffectCommands::Create { name, .. },
                },
        } => {
            assert_eq!(name, "reverb_hall");
        }
        _ => panic!("Expected Asset Effect Create command"),
    }
}

// =============================================================================
// Feature Tests: Effect CRUD with temp project directories
// =============================================================================

mod common;
use am::commands::asset::handle_effect_command;
use am::input::NonInteractiveInput;
use common::fixtures::{CaptureOutput, TestProjectFixture};

/// Create a minimal .amproject file.
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

/// Create an effect JSON file.
fn create_effect_file(
    effects_dir: &std::path::Path,
    name: &str,
    id: u64,
    effect_type: Option<&str>,
) -> anyhow::Result<()> {
    let mut effect = serde_json::json!({
        "id": id,
        "name": name,
    });
    if let Some(et) = effect_type {
        effect["effect"] = serde_json::json!(et);
    }
    std::fs::write(
        effects_dir.join(format!("{}.json", name)),
        serde_json::to_string_pretty(&effect)?,
    )?;
    Ok(())
}

// =============================================================================
// Create Tests
// =============================================================================

#[tokio::test]
async fn test_p0_effect_create_with_defaults() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("effects")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Create {
            name: "reverb_hall".to_string(),
            effect_type: None,
            param: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify file was created
    let effect_file = project_path
        .join("sources")
        .join("effects")
        .join("reverb_hall.json");
    assert!(effect_file.exists(), "Effect file should exist");

    // Verify JSON content
    let content = std::fs::read_to_string(&effect_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["name"], "reverb_hall");

    // Verify output
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["name"], "reverb_hall");
}

#[tokio::test]
async fn test_p0_effect_create_with_all_flags() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("effects")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Create {
            name: "reverb_large".to_string(),
            effect_type: Some("reverb".to_string()),
            param: Some(vec![0.8, 0.5]),
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    let effect_file = project_path
        .join("sources")
        .join("effects")
        .join("reverb_large.json");
    let content = std::fs::read_to_string(&effect_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["name"], "reverb_large");
    assert_eq!(parsed["effect"], "reverb");
    assert_eq!(parsed["parameters"].as_array().unwrap().len(), 2);

    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["effect_type"], "reverb");
}

#[tokio::test]
async fn test_p1_effect_create_duplicate_name_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let effects_dir = project_path.join("sources").join("effects");
    std::fs::create_dir_all(&effects_dir).unwrap();

    // Create an existing effect
    create_effect_file(&effects_dir, "reverb_hall", 12345, Some("reverb")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Create {
            name: "reverb_hall".to_string(),
            effect_type: Some("reverb".to_string()),
            param: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_err(), "Should fail with duplicate name");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("already exists"),
        "Error should mention 'already exists': {}",
        err_msg
    );
}

// =============================================================================
// List Tests
// =============================================================================

#[tokio::test]
async fn test_p0_effect_list_empty_directory() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let effects_dir = project_path.join("sources").join("effects");
    std::fs::create_dir_all(&effects_dir).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(&EffectCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["count"].as_u64().unwrap(), 0);
    assert!(success_data["effects"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_p0_effect_list_multiple_effects() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let effects_dir = project_path.join("sources").join("effects");
    std::fs::create_dir_all(&effects_dir).unwrap();

    create_effect_file(&effects_dir, "reverb_hall", 111, Some("reverb")).unwrap();
    create_effect_file(&effects_dir, "eq_bass", 222, Some("eq")).unwrap();
    create_effect_file(&effects_dir, "delay_short", 333, Some("delay")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(&EffectCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["count"].as_u64().unwrap(), 3);

    let effects = success_data["effects"]
        .as_array()
        .expect("Should have effects array");
    assert_eq!(effects.len(), 3);

    // Verify sorted alphabetically by name
    assert_eq!(effects[0]["name"].as_str().unwrap(), "delay_short");
    assert_eq!(effects[1]["name"].as_str().unwrap(), "eq_bass");
    assert_eq!(effects[2]["name"].as_str().unwrap(), "reverb_hall");

    // Verify effect_type in JSON output
    assert_eq!(effects[0]["effect_type"].as_str().unwrap(), "delay");
    assert_eq!(effects[1]["effect_type"].as_str().unwrap(), "eq");
    assert_eq!(effects[2]["effect_type"].as_str().unwrap(), "reverb");
}

// =============================================================================
// Update Tests
// =============================================================================

#[tokio::test]
async fn test_p0_effect_update_with_flags() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let effects_dir = project_path.join("sources").join("effects");
    std::fs::create_dir_all(&effects_dir).unwrap();

    create_effect_file(&effects_dir, "reverb_hall", 12345, Some("reverb")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Update {
            name: "reverb_hall".to_string(),
            effect_type: Some("eq".to_string()),
            param: Some(vec![1.0, 0.3]),
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify file was updated
    let effect_file = effects_dir.join("reverb_hall.json");
    let content = std::fs::read_to_string(&effect_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["effect"], "eq");
    assert_eq!(parsed["parameters"].as_array().unwrap().len(), 2);

    // Verify output lists updated fields
    let success_data = output.last_success().expect("Should have success output");
    let updated_fields = success_data["updated_fields"]
        .as_array()
        .expect("Should have updated_fields");
    assert!(
        updated_fields
            .iter()
            .any(|f| f.as_str() == Some("effect_type"))
    );
    assert!(
        updated_fields
            .iter()
            .any(|f| f.as_str() == Some("parameters"))
    );
}

#[tokio::test]
async fn test_p1_effect_update_nonexistent_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("effects")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Update {
            name: "nonexistent".to_string(),
            effect_type: Some("reverb".to_string()),
            param: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_err(), "Should fail for non-existent effect");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "Error should mention 'not found': {}",
        err_msg
    );
}

// =============================================================================
// Delete Tests
// =============================================================================

#[tokio::test]
async fn test_p0_effect_delete_with_force() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let effects_dir = project_path.join("sources").join("effects");
    std::fs::create_dir_all(&effects_dir).unwrap();

    create_effect_file(&effects_dir, "reverb_hall", 12345, Some("reverb")).unwrap();

    let effect_file = effects_dir.join("reverb_hall.json");
    assert!(effect_file.exists(), "File should exist before delete");

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Delete {
            name: "reverb_hall".to_string(),
            force: true,
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);
    assert!(
        !effect_file.exists(),
        "File should be deleted after delete command"
    );

    // Verify output
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["deleted"], true);
    assert_eq!(success_data["name"], "reverb_hall");
}

#[tokio::test]
async fn test_p1_effect_delete_nonexistent_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("effects")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Delete {
            name: "nonexistent".to_string(),
            force: true,
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_err(), "Should fail for non-existent effect");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "Error should mention 'not found': {}",
        err_msg
    );
}

#[tokio::test]
async fn test_p1_effect_delete_without_force_in_non_interactive_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let effects_dir = project_path.join("sources").join("effects");
    std::fs::create_dir_all(&effects_dir).unwrap();

    create_effect_file(&effects_dir, "reverb_hall", 12345, Some("reverb")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Delete {
            name: "reverb_hall".to_string(),
            force: false,
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
        "Should fail without --force in non-interactive mode"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("confirmation") || err_msg.contains("--force"),
        "Error should mention confirmation or --force: {}",
        err_msg
    );

    // Verify file was NOT deleted
    let effect_file = effects_dir.join("reverb_hall.json");
    assert!(
        effect_file.exists(),
        "File should still exist after failed delete"
    );
}

// =============================================================================
// JSON Output Format Tests
// =============================================================================

#[tokio::test]
async fn test_p1_effect_list_json_output_format() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let effects_dir = project_path.join("sources").join("effects");
    std::fs::create_dir_all(&effects_dir).unwrap();

    create_effect_file(&effects_dir, "reverb_hall", 111, Some("reverb")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(&EffectCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok());

    let success_data = output.last_success().expect("Should have success output");

    // Verify JSON structure matches expected envelope fields
    assert!(success_data["effects"].is_array());
    assert!(success_data["count"].is_number());
    assert!(success_data["warnings"].is_array());

    let effects = success_data["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 1);

    let first = &effects[0];
    assert!(first["id"].is_number());
    assert!(first["name"].is_string());
    assert!(first["effect_type"].is_string());
    assert!(first["parameter_count"].is_number());
}

#[tokio::test]
async fn test_p1_effect_create_json_output_format() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("effects")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Create {
            name: "test_output".to_string(),
            effect_type: Some("reverb".to_string()),
            param: Some(vec![0.9]),
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok());

    let success_data = output.last_success().expect("Should have success output");
    assert!(success_data["id"].is_number());
    assert_eq!(success_data["name"], "test_output");
    assert!(success_data["path"].is_string());
    assert_eq!(success_data["effect_type"], "reverb");
}

// =============================================================================
// Interactive Output Tests
// =============================================================================

#[tokio::test]
async fn test_p1_effect_list_interactive_output() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let effects_dir = project_path.join("sources").join("effects");
    std::fs::create_dir_all(&effects_dir).unwrap();

    create_effect_file(&effects_dir, "reverb_hall", 111, Some("reverb")).unwrap();
    create_effect_file(&effects_dir, "eq_bass", 222, Some("eq")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    let result = handle_effect_command(&EffectCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok());

    // Verify table was output
    let tables = output.all_tables();
    assert!(!tables.is_empty(), "Should have table output");

    // Verify progress message with count
    let progress = output.all_progress();
    assert!(
        progress.iter().any(|p| p.contains("2 effect(s) found")),
        "Should show count in progress: {:?}",
        progress
    );
}

// =============================================================================
// Validation Tests
// =============================================================================

#[tokio::test]
async fn test_p1_effect_create_empty_name_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("effects")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Create {
            name: "".to_string(),
            effect_type: Some("reverb".to_string()),
            param: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_err(), "Should fail with empty name");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("name") && err_msg.contains("required"),
        "Error should mention name is required: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_p1_effect_create_whitespace_name_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("effects")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_effect_command(
        &EffectCommands::Create {
            name: "   ".to_string(),
            effect_type: Some("reverb".to_string()),
            param: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_err(), "Should fail with whitespace-only name");
}
