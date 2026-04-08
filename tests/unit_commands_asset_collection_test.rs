//! Unit tests for collection asset command.

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, CollectionCommands};
use clap::Parser;

// =============================================================================
// Collection Create Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_collection_create_command_parses_with_name_only() {
    let args = ["am", "asset", "collection", "create", "footsteps"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command:
                        CollectionCommands::Create {
                            name,
                            play_mode,
                            scheduler_mode,
                            gain,
                            ..
                        },
                },
        } => {
            assert_eq!(name, "footsteps");
            assert!(play_mode.is_none());
            assert!(scheduler_mode.is_none());
            assert!(gain.is_none());
        }
        _ => panic!("Expected Asset Collection Create command"),
    }
}

#[test]
fn test_p0_collection_create_command_parses_with_all_flags() {
    let args = [
        "am",
        "asset",
        "collection",
        "create",
        "footsteps",
        "--play-mode",
        "PlayAll",
        "--scheduler-mode",
        "Sequence",
        "--gain",
        "0.8",
        "--bus",
        "100",
        "--priority",
        "200",
        "--spatialization",
        "position",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command:
                        CollectionCommands::Create {
                            name,
                            play_mode,
                            scheduler_mode,
                            gain,
                            bus,
                            priority,
                            spatialization,
                        },
                },
        } => {
            assert_eq!(name, "footsteps");
            assert_eq!(play_mode, Some("PlayAll".to_string()));
            assert_eq!(scheduler_mode, Some("Sequence".to_string()));
            assert_eq!(gain, Some(0.8));
            assert_eq!(bus, Some(100));
            assert_eq!(priority, Some(200));
            assert_eq!(spatialization, Some("position".to_string()));
        }
        _ => panic!("Expected Asset Collection Create command"),
    }
}

#[test]
fn test_p1_collection_create_command_short_flags() {
    let args = [
        "am",
        "asset",
        "collection",
        "create",
        "footsteps",
        "-g",
        "0.75",
        "-b",
        "50",
        "-p",
        "100",
        "-s",
        "hrtf",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command:
                        CollectionCommands::Create {
                            name,
                            gain,
                            bus,
                            priority,
                            spatialization,
                            ..
                        },
                },
        } => {
            assert_eq!(name, "footsteps");
            assert_eq!(gain, Some(0.75));
            assert_eq!(bus, Some(50));
            assert_eq!(priority, Some(100));
            assert_eq!(spatialization, Some("hrtf".to_string()));
        }
        _ => panic!("Expected Asset Collection Create command"),
    }
}

#[test]
fn test_p1_collection_create_command_requires_name() {
    let args = ["am", "asset", "collection", "create"];
    let result = App::try_parse_from(args);
    assert!(result.is_err(), "Should fail without name argument");
}

// =============================================================================
// Collection List Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_collection_list_command_parses() {
    let args = ["am", "asset", "collection", "list"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command: CollectionCommands::List {},
                },
        } => {} // Success - it parsed correctly
        _ => panic!("Expected Asset Collection List command"),
    }
}

// =============================================================================
// Collection Update Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_collection_update_command_parses_with_name_only() {
    let args = ["am", "asset", "collection", "update", "footsteps"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command:
                        CollectionCommands::Update {
                            name,
                            play_mode,
                            gain,
                            ..
                        },
                },
        } => {
            assert_eq!(name, "footsteps");
            assert!(play_mode.is_none());
            assert!(gain.is_none());
        }
        _ => panic!("Expected Asset Collection Update command"),
    }
}

#[test]
fn test_p0_collection_update_command_parses_with_all_flags() {
    let args = [
        "am",
        "asset",
        "collection",
        "update",
        "footsteps",
        "--play-mode",
        "PlayAll",
        "--scheduler-mode",
        "Sequence",
        "--gain",
        "0.5",
        "--bus",
        "42",
        "--priority",
        "255",
        "--spatialization",
        "hrtf",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command:
                        CollectionCommands::Update {
                            name,
                            play_mode,
                            scheduler_mode,
                            gain,
                            bus,
                            priority,
                            spatialization,
                        },
                },
        } => {
            assert_eq!(name, "footsteps");
            assert_eq!(play_mode, Some("PlayAll".to_string()));
            assert_eq!(scheduler_mode, Some("Sequence".to_string()));
            assert_eq!(gain, Some(0.5));
            assert_eq!(bus, Some(42));
            assert_eq!(priority, Some(255));
            assert_eq!(spatialization, Some("hrtf".to_string()));
        }
        _ => panic!("Expected Asset Collection Update command"),
    }
}

#[test]
fn test_p1_collection_update_command_requires_name() {
    let args = ["am", "asset", "collection", "update"];
    let result = App::try_parse_from(args);
    assert!(result.is_err(), "Should fail without name argument");
}

// =============================================================================
// Collection Delete Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_collection_delete_command_parses() {
    let args = ["am", "asset", "collection", "delete", "footsteps"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command: CollectionCommands::Delete { name, force },
                },
        } => {
            assert_eq!(name, "footsteps");
            assert!(!force);
        }
        _ => panic!("Expected Asset Collection Delete command"),
    }
}

#[test]
fn test_p0_collection_delete_command_parses_with_force() {
    let args = [
        "am",
        "asset",
        "collection",
        "delete",
        "footsteps",
        "--force",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command: CollectionCommands::Delete { name, force },
                },
        } => {
            assert_eq!(name, "footsteps");
            assert!(force);
        }
        _ => panic!("Expected Asset Collection Delete command"),
    }
}

#[test]
fn test_p1_collection_delete_command_requires_name() {
    let args = ["am", "asset", "collection", "delete"];
    let result = App::try_parse_from(args);
    assert!(result.is_err(), "Should fail without name argument");
}

// =============================================================================
// Non-Interactive Mode Tests
// =============================================================================

#[test]
fn test_p1_collection_create_with_non_interactive_flag() {
    let args = [
        "am",
        "--non-interactive",
        "asset",
        "collection",
        "create",
        "footsteps",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.non_interactive);
    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command: CollectionCommands::Create { name, .. },
                },
        } => {
            assert_eq!(name, "footsteps");
        }
        _ => panic!("Expected Asset Collection Create command"),
    }
}

#[test]
fn test_p1_collection_create_with_json_flag() {
    let args = ["am", "--json", "asset", "collection", "create", "footsteps"];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.json);
    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Collection {
                    command: CollectionCommands::Create { name, .. },
                },
        } => {
            assert_eq!(name, "footsteps");
        }
        _ => panic!("Expected Asset Collection Create command"),
    }
}

// =============================================================================
// Feature Tests: Collection CRUD with temp project directories
// =============================================================================

mod common;
use am::commands::asset::handle_collection_command;
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

/// Create a collection JSON file.
fn create_collection_file(
    collections_dir: &std::path::Path,
    name: &str,
    id: u64,
    play_mode: &str,
    scheduler_mode: &str,
) -> anyhow::Result<()> {
    let collection = serde_json::json!({
        "id": id,
        "name": name,
        "bus": 0,
        "attenuation": 0,
        "effect": 0,
        "gain": { "kind": "Static", "value": 1.0 },
        "priority": { "kind": "Static", "value": 128.0 },
        "fader": "Linear",
        "spatialization": "None",
        "scope": "World",
        "play_mode": play_mode,
        "scheduler": { "mode": scheduler_mode }
    });
    std::fs::write(
        collections_dir.join(format!("{}.json", name)),
        serde_json::to_string_pretty(&collection)?,
    )?;
    Ok(())
}

// =============================================================================
// Create Tests
// =============================================================================

#[tokio::test]
async fn test_p0_collection_create_with_defaults() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("collections")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Non-interactive mode: play_mode and scheduler_mode default when prompts fail
    let result = handle_collection_command(
        &CollectionCommands::Create {
            name: "footsteps".to_string(),
            play_mode: None,
            scheduler_mode: None,
            gain: None,
            bus: None,
            priority: None,
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

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify file was created
    let collection_file = project_path
        .join("sources")
        .join("collections")
        .join("footsteps.json");
    assert!(collection_file.exists(), "Collection file should exist");

    // Verify JSON content
    let content = std::fs::read_to_string(&collection_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["name"], "footsteps");
    assert_eq!(parsed["play_mode"], "PlayOne");
    assert_eq!(parsed["scheduler"]["mode"], "Random");

    // Verify output
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["name"], "footsteps");
    assert_eq!(success_data["play_mode"], "PlayOne");
    assert_eq!(success_data["scheduler_mode"], "Random");
}

#[tokio::test]
async fn test_p0_collection_create_with_all_flags() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("collections")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Create {
            name: "explosions".to_string(),
            play_mode: Some("PlayAll".to_string()),
            scheduler_mode: Some("Sequence".to_string()),
            gain: Some(0.8),
            bus: Some(42),
            priority: Some(200),
            spatialization: Some("position".to_string()),
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

    let collection_file = project_path
        .join("sources")
        .join("collections")
        .join("explosions.json");
    let content = std::fs::read_to_string(&collection_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["name"], "explosions");
    assert_eq!(parsed["play_mode"], "PlayAll");
    assert_eq!(parsed["scheduler"]["mode"], "Sequence");
    assert_eq!(parsed["bus"], 42);
    assert_eq!(parsed["spatialization"], "Position");

    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["play_mode"], "PlayAll");
    assert_eq!(success_data["scheduler_mode"], "Sequence");
}

#[tokio::test]
async fn test_p1_collection_create_duplicate_name_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let collections_dir = project_path.join("sources").join("collections");
    std::fs::create_dir_all(&collections_dir).unwrap();

    // Create an existing collection
    create_collection_file(&collections_dir, "footsteps", 12345, "PlayOne", "Random").unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Create {
            name: "footsteps".to_string(),
            play_mode: Some("PlayOne".to_string()),
            scheduler_mode: Some("Random".to_string()),
            gain: None,
            bus: None,
            priority: None,
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
async fn test_p0_collection_list_empty_directory() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let collections_dir = project_path.join("sources").join("collections");
    std::fs::create_dir_all(&collections_dir).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result =
        handle_collection_command(&CollectionCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["count"].as_u64().unwrap(), 0);
    assert!(success_data["collections"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_p0_collection_list_multiple_collections() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let collections_dir = project_path.join("sources").join("collections");
    std::fs::create_dir_all(&collections_dir).unwrap();

    create_collection_file(&collections_dir, "footsteps", 111, "PlayOne", "Random").unwrap();
    create_collection_file(&collections_dir, "explosions", 222, "PlayAll", "Sequence").unwrap();
    create_collection_file(&collections_dir, "ambient", 333, "PlayOne", "Sequence").unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result =
        handle_collection_command(&CollectionCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["count"].as_u64().unwrap(), 3);

    let collections = success_data["collections"]
        .as_array()
        .expect("Should have collections array");
    assert_eq!(collections.len(), 3);

    // Verify sorted alphabetically by name
    assert_eq!(collections[0]["name"].as_str().unwrap(), "ambient");
    assert_eq!(collections[1]["name"].as_str().unwrap(), "explosions");
    assert_eq!(collections[2]["name"].as_str().unwrap(), "footsteps");

    // Verify play_mode and scheduler_mode in JSON output
    assert_eq!(collections[1]["play_mode"].as_str().unwrap(), "PlayAll");
    assert_eq!(
        collections[1]["scheduler_mode"].as_str().unwrap(),
        "Sequence"
    );
}

// =============================================================================
// Update Tests
// =============================================================================

#[tokio::test]
async fn test_p0_collection_update_with_flags() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let collections_dir = project_path.join("sources").join("collections");
    std::fs::create_dir_all(&collections_dir).unwrap();

    create_collection_file(&collections_dir, "footsteps", 12345, "PlayOne", "Random").unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Update {
            name: "footsteps".to_string(),
            play_mode: Some("PlayAll".to_string()),
            scheduler_mode: Some("Sequence".to_string()),
            gain: Some(0.5),
            bus: None,
            priority: None,
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

    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify file was updated
    let collection_file = collections_dir.join("footsteps.json");
    let content = std::fs::read_to_string(&collection_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["play_mode"], "PlayAll");
    assert_eq!(parsed["scheduler"]["mode"], "Sequence");

    // Verify output lists updated fields
    let success_data = output.last_success().expect("Should have success output");
    let updated_fields = success_data["updated_fields"]
        .as_array()
        .expect("Should have updated_fields");
    assert!(
        updated_fields
            .iter()
            .any(|f| f.as_str() == Some("play_mode"))
    );
    assert!(
        updated_fields
            .iter()
            .any(|f| f.as_str() == Some("scheduler_mode"))
    );
    assert!(updated_fields.iter().any(|f| f.as_str() == Some("gain")));
}

#[tokio::test]
async fn test_p1_collection_update_nonexistent_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("collections")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Update {
            name: "nonexistent".to_string(),
            play_mode: Some("PlayAll".to_string()),
            scheduler_mode: None,
            gain: None,
            bus: None,
            priority: None,
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

    assert!(result.is_err(), "Should fail for non-existent collection");
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
async fn test_p0_collection_delete_with_force() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let collections_dir = project_path.join("sources").join("collections");
    std::fs::create_dir_all(&collections_dir).unwrap();

    create_collection_file(&collections_dir, "footsteps", 12345, "PlayOne", "Random").unwrap();

    let collection_file = collections_dir.join("footsteps.json");
    assert!(collection_file.exists(), "File should exist before delete");

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Delete {
            name: "footsteps".to_string(),
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
        !collection_file.exists(),
        "File should be deleted after delete command"
    );

    // Verify output
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["deleted"], true);
    assert_eq!(success_data["name"], "footsteps");
}

#[tokio::test]
async fn test_p1_collection_delete_nonexistent_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("collections")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Delete {
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

    assert!(result.is_err(), "Should fail for non-existent collection");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "Error should mention 'not found': {}",
        err_msg
    );
}

#[tokio::test]
async fn test_p1_collection_delete_without_force_in_non_interactive_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let collections_dir = project_path.join("sources").join("collections");
    std::fs::create_dir_all(&collections_dir).unwrap();

    create_collection_file(&collections_dir, "footsteps", 12345, "PlayOne", "Random").unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Delete {
            name: "footsteps".to_string(),
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
    let collection_file = collections_dir.join("footsteps.json");
    assert!(
        collection_file.exists(),
        "File should still exist after failed delete"
    );
}

// =============================================================================
// JSON Output Format Tests
// =============================================================================

#[tokio::test]
async fn test_p1_collection_list_json_output_format() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let collections_dir = project_path.join("sources").join("collections");
    std::fs::create_dir_all(&collections_dir).unwrap();

    create_collection_file(&collections_dir, "footsteps", 111, "PlayOne", "Random").unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result =
        handle_collection_command(&CollectionCommands::List {}, None, &input, &output).await;

    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    assert!(result.is_ok());

    let success_data = output.last_success().expect("Should have success output");

    // Verify JSON structure matches expected envelope fields
    assert!(success_data["collections"].is_array());
    assert!(success_data["count"].is_number());
    assert!(success_data["warnings"].is_array());

    let collections = success_data["collections"].as_array().unwrap();
    assert_eq!(collections.len(), 1);

    let first = &collections[0];
    assert!(first["id"].is_number());
    assert!(first["name"].is_string());
    assert!(first["play_mode"].is_string());
    assert!(first["scheduler_mode"].is_string());
    assert!(first["gain"].is_number() || first["gain"].is_string() || first["gain"].is_null());
    assert!(first["spatialization"].is_string());
}

#[tokio::test]
async fn test_p1_collection_create_json_output_format() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("collections")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Create {
            name: "test_output".to_string(),
            play_mode: Some("PlayOne".to_string()),
            scheduler_mode: Some("Random".to_string()),
            gain: Some(0.9),
            bus: None,
            priority: None,
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

    assert!(result.is_ok());

    let success_data = output.last_success().expect("Should have success output");
    assert!(success_data["id"].is_number());
    assert_eq!(success_data["name"], "test_output");
    assert!(success_data["path"].is_string());
    assert_eq!(success_data["play_mode"], "PlayOne");
    assert_eq!(success_data["scheduler_mode"], "Random");
}

// =============================================================================
// Interactive Output Tests
// =============================================================================

#[tokio::test]
async fn test_p1_collection_list_interactive_output() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let collections_dir = project_path.join("sources").join("collections");
    std::fs::create_dir_all(&collections_dir).unwrap();

    create_collection_file(&collections_dir, "footsteps", 111, "PlayOne", "Random").unwrap();
    create_collection_file(&collections_dir, "explosions", 222, "PlayAll", "Sequence").unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    let result =
        handle_collection_command(&CollectionCommands::List {}, None, &input, &output).await;

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
        progress.iter().any(|p| p.contains("2 collection(s) found")),
        "Should show count in progress: {:?}",
        progress
    );
}

// =============================================================================
// Validation Tests (Review Follow-ups)
// =============================================================================

#[tokio::test]
async fn test_p1_collection_create_empty_name_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("collections")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Create {
            name: "".to_string(),
            play_mode: Some("PlayOne".to_string()),
            scheduler_mode: Some("Random".to_string()),
            gain: None,
            bus: None,
            priority: None,
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

    assert!(result.is_err(), "Should fail with empty name");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("name") && err_msg.contains("required"),
        "Error should mention name is required: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_p1_collection_create_whitespace_name_fails() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    std::fs::create_dir_all(project_path.join("sources").join("collections")).unwrap();

    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    let result = handle_collection_command(
        &CollectionCommands::Create {
            name: "   ".to_string(),
            play_mode: Some("PlayOne".to_string()),
            scheduler_mode: Some("Random".to_string()),
            gain: None,
            bus: None,
            priority: None,
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

    assert!(result.is_err(), "Should fail with whitespace-only name");
}
