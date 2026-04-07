//! Unit tests for sound asset command.

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, SoundCommands};
use clap::Parser;

// =============================================================================
// Sound Create Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_sound_create_command_parses_with_name_only() {
    let args = ["am", "asset", "sound", "create", "explosion"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command:
                        SoundCommands::Create {
                            name, file, gain, ..
                        },
                },
        } => {
            assert_eq!(name, "explosion");
            assert!(file.is_none());
            assert!(gain.is_none());
        }
        _ => panic!("Expected Asset Sound Create command"),
    }
}

#[test]
fn test_p0_sound_create_command_parses_with_file_flag() {
    let args = [
        "am",
        "asset",
        "sound",
        "create",
        "explosion",
        "--file",
        "sfx/explosion.wav",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command: SoundCommands::Create { name, file, .. },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(file, Some("sfx/explosion.wav".to_string()));
        }
        _ => panic!("Expected Asset Sound Create command"),
    }
}

#[test]
fn test_p0_sound_create_command_parses_with_gain_flag() {
    let args = [
        "am",
        "asset",
        "sound",
        "create",
        "explosion",
        "--gain",
        "0.8",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command: SoundCommands::Create { name, gain, .. },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(gain, Some(0.8));
        }
        _ => panic!("Expected Asset Sound Create command"),
    }
}

#[test]
fn test_p0_sound_create_command_parses_with_all_flags() {
    let args = [
        "am",
        "asset",
        "sound",
        "create",
        "explosion",
        "--file",
        "sfx/explosion.wav",
        "--gain",
        "0.8",
        "--bus",
        "100",
        "--priority",
        "200",
        "--stream",
        "--loop",
        "--loop-count",
        "3",
        "--spatialization",
        "position",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command:
                        SoundCommands::Create {
                            name,
                            file,
                            gain,
                            bus,
                            priority,
                            stream,
                            loop_enabled,
                            loop_count,
                            spatialization,
                        },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(file, Some("sfx/explosion.wav".to_string()));
            assert_eq!(gain, Some(0.8));
            assert_eq!(bus, Some(100));
            assert_eq!(priority, Some(200));
            assert!(stream);
            assert!(loop_enabled);
            assert_eq!(loop_count, Some(3));
            assert_eq!(spatialization, Some("position".to_string()));
        }
        _ => panic!("Expected Asset Sound Create command"),
    }
}

#[test]
fn test_p1_sound_create_command_short_flags() {
    let args = [
        "am",
        "asset",
        "sound",
        "create",
        "explosion",
        "-f",
        "sfx/explosion.wav",
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
                AssetCommands::Sound {
                    command:
                        SoundCommands::Create {
                            name,
                            file,
                            gain,
                            bus,
                            priority,
                            spatialization,
                            ..
                        },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(file, Some("sfx/explosion.wav".to_string()));
            assert_eq!(gain, Some(0.75));
            assert_eq!(bus, Some(50));
            assert_eq!(priority, Some(100));
            assert_eq!(spatialization, Some("hrtf".to_string()));
        }
        _ => panic!("Expected Asset Sound Create command"),
    }
}

#[test]
fn test_p1_sound_create_command_requires_name() {
    let args = ["am", "asset", "sound", "create"];
    let result = App::try_parse_from(args);
    assert!(result.is_err(), "Should fail without name argument");
}

// =============================================================================
// Non-Interactive Mode Tests
// =============================================================================

#[test]
fn test_p1_sound_create_with_non_interactive_flag() {
    let args = [
        "am",
        "--non-interactive",
        "asset",
        "sound",
        "create",
        "explosion",
        "--file",
        "sfx/explosion.wav",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.non_interactive);
    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command: SoundCommands::Create { name, file, .. },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(file, Some("sfx/explosion.wav".to_string()));
        }
        _ => panic!("Expected Asset Sound Create command"),
    }
}

#[test]
fn test_p1_sound_create_with_json_flag() {
    let args = [
        "am",
        "--json",
        "asset",
        "sound",
        "create",
        "explosion",
        "--file",
        "sfx/explosion.wav",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.json);
    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command: SoundCommands::Create { name, file, .. },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(file, Some("sfx/explosion.wav".to_string()));
        }
        _ => panic!("Expected Asset Sound Create command"),
    }
}

// =============================================================================
// Sound Asset Struct Tests
// =============================================================================

use am::assets::{Sound, SoundLoopConfig, Spatialization};

#[test]
fn test_p0_sound_builder_creates_valid_sound() {
    let sound = Sound::builder(12345, "explosion")
        .path("sfx/explosion.wav")
        .gain(0.8)
        .priority(200)
        .stream(true)
        .loop_config(SoundLoopConfig::infinite())
        .spatialization(Spatialization::Position)
        .build();

    assert_eq!(sound.id, 12345);
    assert_eq!(sound.name.as_deref(), Some("explosion"));
    assert_eq!(sound.path.as_deref(), Some("sfx/explosion.wav"));
    assert_eq!(sound.gain.as_ref().and_then(|g| g.as_static()), Some(0.8));
    assert_eq!(
        sound.priority.as_ref().and_then(|p| p.as_static()),
        Some(200.0)
    );
    assert!(sound.stream);
    assert!(sound.loop_.as_ref().unwrap().enabled);
    assert_eq!(sound.loop_.as_ref().unwrap().loop_count, 0);
    assert_eq!(sound.spatialization, Spatialization::Position);
}

#[test]
fn test_p0_sound_default_values() {
    let sound = Sound::builder(1, "test").path("test.wav").build();

    assert_eq!(sound.gain.as_ref().and_then(|g| g.as_static()), Some(1.0));
    assert_eq!(
        sound.priority.as_ref().and_then(|p| p.as_static()),
        Some(128.0)
    );
    assert!(!sound.stream);
    assert!(!sound.loop_.as_ref().unwrap().enabled);
    assert_eq!(sound.spatialization, Spatialization::None);
}

#[test]
fn test_p1_sound_loop_config_disabled() {
    let config = SoundLoopConfig::disabled();
    assert!(!config.enabled);
    assert_eq!(config.loop_count, 0);
}

#[test]
fn test_p1_sound_loop_config_infinite() {
    let config = SoundLoopConfig::infinite();
    assert!(config.enabled);
    assert_eq!(config.loop_count, 0);
}

#[test]
fn test_p1_sound_loop_config_count() {
    let config = SoundLoopConfig::count(5);
    assert!(config.enabled);
    assert_eq!(config.loop_count, 5);
}

#[test]
fn test_p1_spatialization_modes() {
    assert_eq!(Spatialization::None, Spatialization::None);
    assert_eq!(Spatialization::Position, Spatialization::Position);
    assert_eq!(
        Spatialization::PositionOrientation,
        Spatialization::PositionOrientation
    );
    assert_eq!(Spatialization::HRTF, Spatialization::HRTF);
}

// =============================================================================
// Sound Serialization Tests
// =============================================================================

#[test]
fn test_p0_sound_serializes_to_json() {
    let sound = Sound::builder(12345, "explosion")
        .path("sfx/explosion.wav")
        .gain(0.8)
        .priority(200)
        .build();

    let json = serde_json::to_string_pretty(&sound).expect("Should serialize");

    assert!(json.contains("\"id\": 12345"));
    assert!(json.contains("\"name\": \"explosion\""));
    assert!(json.contains("\"path\": \"sfx/explosion.wav\""));
}

#[test]
fn test_p0_sound_deserializes_from_json() {
    let json = r#"{
        "id": 54321,
        "name": "footstep",
        "path": "sfx/footstep.wav",
        "bus": 0,
        "gain": { "kind": "Static", "value": 0.9 },
        "priority": { "kind": "Static", "value": 100.0 },
        "stream": false,
        "loop": { "enabled": false, "loop_count": 0 },
        "spatialization": "Position",
        "attenuation": 0,
        "scope": "World",
        "fader": "Linear",
        "effect": 0
    }"#;

    let sound: Sound = serde_json::from_str(json).expect("Should deserialize");

    assert_eq!(sound.id, 54321);
    assert_eq!(sound.name.as_deref(), Some("footstep"));
    assert_eq!(sound.gain.as_ref().and_then(|g| g.as_static()), Some(0.9));
    assert_eq!(sound.spatialization, Spatialization::Position);
}

// =============================================================================
// Validation Tests
// =============================================================================

use am::assets::ProjectContext;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_p1_sound_validate_rules_passes_with_valid_audio_file() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data").join("sfx");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    let context = ProjectContext::new(dir.path().to_path_buf());
    let sound = Sound::builder(1, "explosion")
        .path("sfx/explosion.wav")
        .gain(0.8)
        .build();

    use am::assets::Asset;
    assert!(sound.validate_rules(&context).is_ok());
}

#[test]
fn test_p1_sound_validate_rules_fails_missing_audio_file() {
    let dir = tempdir().unwrap();
    let context = ProjectContext::new(dir.path().to_path_buf());

    let sound = Sound::builder(1, "explosion")
        .path("sfx/missing.wav")
        .gain(0.8)
        .build();

    use am::assets::Asset;
    let result = sound.validate_rules(&context);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.what().contains("Audio file not found"));
}

#[test]
fn test_p1_sound_validate_rules_fails_invalid_gain_above_one() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("test.wav"), b"fake audio").unwrap();
    let context = ProjectContext::new(dir.path().to_path_buf());

    let sound = Sound::builder(1, "test")
        .path("sfx/test.wav")
        .gain(1.5)
        .build();

    use am::assets::Asset;
    let result = sound.validate_rules(&context);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.what().contains("Invalid gain value"));
}

#[test]
fn test_p1_sound_validate_rules_fails_invalid_gain_below_zero() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("test.wav"), b"fake audio").unwrap();
    let context = ProjectContext::new(dir.path().to_path_buf());

    let sound = Sound::builder(1, "test")
        .path("sfx/test.wav")
        .gain(-0.5)
        .build();

    use am::assets::Asset;
    let result = sound.validate_rules(&context);
    assert!(result.is_err());
}

// =============================================================================
// Atomic Write Tests
// =============================================================================

use am::common::files::atomic_write;

#[test]
fn test_p1_atomic_write_creates_file() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.json");

    atomic_write(&file_path, b"test content").unwrap();

    assert!(file_path.exists());
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "test content");
}

#[test]
fn test_p1_atomic_write_creates_parent_directories() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("sources").join("sounds").join("test.json");

    atomic_write(&file_path, b"nested content").unwrap();

    assert!(file_path.exists());
}

#[test]
fn test_p1_atomic_write_overwrites_existing_file() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.json");

    fs::write(&file_path, "old content").unwrap();
    atomic_write(&file_path, b"new content").unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "new content");
}

#[test]
fn test_p1_atomic_write_no_tmp_file_remains() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.json");
    let tmp_path = file_path.with_extension("tmp");

    atomic_write(&file_path, b"content").unwrap();

    assert!(!tmp_path.exists());
}

// =============================================================================
// Sound List Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_sound_list_command_parses() {
    let args = ["am", "asset", "sound", "list"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command: SoundCommands::List {},
                },
        } => {
            // Success - command parsed correctly
        }
        _ => panic!("Expected Asset Sound List command"),
    }
}

#[test]
fn test_p1_sound_list_command_with_json_flag() {
    let args = ["am", "--json", "asset", "sound", "list"];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.json);
    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command: SoundCommands::List {},
                },
        } => {
            // Success - command parsed correctly
        }
        _ => panic!("Expected Asset Sound List command"),
    }
}

// =============================================================================
// Sound List Handler Tests
// =============================================================================

mod common;
use am::commands::asset::handle_sound_command;
use am::input::NonInteractiveInput;
use am::presentation::{Output, OutputMode};
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

/// Create a sound JSON file.
fn create_sound_file(
    sounds_dir: &std::path::Path,
    name: &str,
    id: u64,
    gain: f32,
) -> anyhow::Result<()> {
    let sound = serde_json::json!({
        "id": id,
        "name": name,
        "path": format!("sfx/{}.wav", name),
        "bus": 0,
        "gain": { "kind": "Static", "value": gain },
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
        sounds_dir.join(format!("{}.json", name)),
        serde_json::to_string_pretty(&sound)?,
    )?;
    Ok(())
}

#[tokio::test]
async fn test_p0_sound_list_multiple_sounds_displays_all() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory and sound files
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();
    create_sound_file(&sounds_dir, "explosion", 12345, 0.8).unwrap();
    create_sound_file(&sounds_dir, "footstep", 67890, 0.5).unwrap();
    create_sound_file(&sounds_dir, "ambient", 11111, 1.0).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Run the list command
    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    // Restore original directory if we had one
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify output content
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(
        success_data["count"].as_u64().unwrap(),
        3,
        "Should list 3 sounds"
    );

    let sounds = success_data["sounds"]
        .as_array()
        .expect("Should have sounds array");
    assert_eq!(sounds.len(), 3, "Should have 3 sounds in array");

    // Verify sounds are sorted alphabetically by name
    assert_eq!(sounds[0]["name"].as_str().unwrap(), "ambient");
    assert_eq!(sounds[1]["name"].as_str().unwrap(), "explosion");
    assert_eq!(sounds[2]["name"].as_str().unwrap(), "footstep");

    // Verify gain is numeric in JSON output
    assert!(
        sounds[0]["gain"].is_number(),
        "Gain should be numeric in JSON"
    );
}

#[tokio::test]
async fn test_p0_sound_list_empty_directory_shows_message() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure with empty sounds directory
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Run the list command
    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    // Restore original directory if we had one
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(
        result.is_ok(),
        "Handler should succeed with empty directory: {:?}",
        result
    );

    // Verify JSON output shows empty array and count=0
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(
        success_data["count"].as_u64().unwrap(),
        0,
        "Should have count 0"
    );
    assert!(
        success_data["sounds"].as_array().unwrap().is_empty(),
        "Should have empty sounds array"
    );
}

#[tokio::test]
async fn test_p1_sound_list_invalid_json_warns_but_continues() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory with one valid and one invalid sound file
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();
    create_sound_file(&sounds_dir, "explosion", 12345, 0.8).unwrap();

    // Create an invalid JSON file
    std::fs::write(sounds_dir.join("invalid.json"), "{ not valid json }").unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Run the list command
    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    // Restore original directory if we had one
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Should still succeed (invalid files are warned but don't fail the command)
    assert!(
        result.is_ok(),
        "Handler should succeed despite invalid JSON: {:?}",
        result
    );

    // Verify the valid sound is still listed
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(
        success_data["count"].as_u64().unwrap(),
        1,
        "Should list 1 valid sound"
    );

    // Verify warnings array contains the invalid file warning
    let warnings = success_data["warnings"]
        .as_array()
        .expect("Should have warnings array");
    assert!(!warnings.is_empty(), "Should have at least one warning");
    let warning_text = warnings[0].as_str().unwrap();
    assert!(
        warning_text.contains("invalid.json"),
        "Warning should mention invalid file"
    );
}

#[tokio::test]
async fn test_p1_sound_list_json_output_envelope_format() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory and sound file
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();
    create_sound_file(&sounds_dir, "explosion", 12345, 0.8).unwrap();

    // Change to project directory - save current dir before we change, ignore if fails (parallel test cleanup)
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs with JSON output
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();
    assert_eq!(output.mode(), OutputMode::Json);

    // Run the list command
    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    // Restore original directory if we had one
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify envelope format: sounds array, count, warnings
    let success_data = output.last_success().expect("Should have success output");
    assert!(
        success_data.get("sounds").is_some(),
        "Should have 'sounds' field"
    );
    assert!(
        success_data.get("count").is_some(),
        "Should have 'count' field"
    );
    assert!(
        success_data.get("warnings").is_some(),
        "Should have 'warnings' field"
    );

    // Verify sound object structure
    let sounds = success_data["sounds"].as_array().unwrap();
    assert_eq!(sounds.len(), 1);
    let sound = &sounds[0];
    assert!(sound.get("id").is_some(), "Sound should have 'id' field");
    assert!(
        sound.get("name").is_some(),
        "Sound should have 'name' field"
    );
    assert!(
        sound.get("path").is_some(),
        "Sound should have 'path' field"
    );
    assert!(
        sound.get("gain").is_some(),
        "Sound should have 'gain' field"
    );
    assert!(
        sound.get("loop_enabled").is_some(),
        "Sound should have 'loop_enabled' field"
    );
    assert!(
        sound.get("spatialization").is_some(),
        "Sound should have 'spatialization' field"
    );
}

#[tokio::test]
async fn test_p1_sound_list_table_format_has_correct_columns() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory and sound files
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create sounds with specific names to verify sorting
    create_sound_file(&sounds_dir, "zebra", 99999, 0.9).unwrap();
    create_sound_file(&sounds_dir, "alpha", 11111, 0.1).unwrap();
    create_sound_file(&sounds_dir, "beta", 22222, 0.5).unwrap();

    // Change to project directory - save current dir before we change, ignore if fails (parallel test cleanup)
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs with INTERACTIVE mode for table output
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    // Run the list command
    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    // Restore original directory if we had one
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify table output was called
    let table_data = output.last_table().expect("Should have table output");
    let (_, data) = table_data;
    let rows = data.as_array().expect("Table data should be array");

    // Verify we have 3 sounds
    assert_eq!(rows.len(), 3, "Should have 3 sounds in table");

    // Verify sounds are sorted alphabetically by name
    assert_eq!(rows[0]["name"].as_str().unwrap(), "alpha");
    assert_eq!(rows[1]["name"].as_str().unwrap(), "beta");
    assert_eq!(rows[2]["name"].as_str().unwrap(), "zebra");

    // Verify table columns: id, name, audio_file, gain
    let first_row = &rows[0];
    assert!(
        first_row.get("id").is_some(),
        "Table row should have 'id' column"
    );
    assert!(
        first_row.get("name").is_some(),
        "Table row should have 'name' column"
    );
    assert!(
        first_row.get("audio_file").is_some(),
        "Table row should have 'audio_file' column"
    );
    assert!(
        first_row.get("gain").is_some(),
        "Table row should have 'gain' column"
    );

    // Verify progress message shows count
    let progress = output.all_progress();
    assert!(
        progress.iter().any(|msg| msg.contains("3 sound(s) found")),
        "Should show count"
    );
}

#[tokio::test]
async fn test_p1_sound_list_no_sounds_directory() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure WITHOUT sounds directory
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    // Don't create sources/sounds directory

    // Change to project directory - save current dir before we change, ignore if fails (parallel test cleanup)
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Run the list command
    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    // Restore original directory if we had one
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Should succeed with empty result (directory doesn't exist is handled gracefully)
    assert!(
        result.is_ok(),
        "Handler should succeed with missing sounds dir: {:?}",
        result
    );

    // Verify empty result
    let success_data = output.last_success().expect("Should have success output");
    assert_eq!(success_data["count"].as_u64().unwrap(), 0);
    assert!(success_data["sounds"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_p1_sound_list_all_invalid_shows_warnings_in_interactive() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory with ONLY invalid files (no valid sounds)
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();
    std::fs::write(sounds_dir.join("broken1.json"), "{ invalid json }").unwrap();
    std::fs::write(sounds_dir.join("broken2.json"), "not json at all").unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs with INTERACTIVE mode
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    // Run the list command
    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Should succeed (invalid files don't fail the command)
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify warnings ARE displayed in interactive mode (this was the bug fix)
    let progress = output.all_progress();
    let has_warning = progress.iter().any(|msg| msg.contains("Warning:"));
    assert!(
        has_warning,
        "Should display warnings about invalid files in interactive mode. Progress messages: {:?}",
        progress
    );

    // Verify the "no sounds found" message is also present
    assert!(
        progress.iter().any(|msg| msg.contains("No sounds found")),
        "Should show 'No sounds found' message"
    );
}

// =============================================================================
// Sound Update Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_sound_update_command_parses_with_name_only() {
    let args = ["am", "asset", "sound", "update", "explosion"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command:
                        SoundCommands::Update {
                            name, file, gain, ..
                        },
                },
        } => {
            assert_eq!(name, "explosion");
            assert!(file.is_none());
            assert!(gain.is_none());
        }
        _ => panic!("Expected Asset Sound Update command"),
    }
}

#[test]
fn test_p0_sound_update_command_parses_with_gain_flag() {
    let args = [
        "am",
        "asset",
        "sound",
        "update",
        "explosion",
        "--gain",
        "0.5",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command: SoundCommands::Update { name, gain, .. },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(gain, Some(0.5));
        }
        _ => panic!("Expected Asset Sound Update command"),
    }
}

#[test]
fn test_p0_sound_update_command_parses_with_all_flags() {
    let args = [
        "am",
        "asset",
        "sound",
        "update",
        "explosion",
        "--file",
        "sfx/explosion_v2.wav",
        "--gain",
        "0.5",
        "--bus",
        "50",
        "--priority",
        "200",
        "--stream",
        "true",
        "--loop",
        "true",
        "--loop-count",
        "5",
        "--spatialization",
        "hrtf",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command:
                        SoundCommands::Update {
                            name,
                            file,
                            gain,
                            bus,
                            priority,
                            stream,
                            loop_enabled,
                            loop_count,
                            spatialization,
                        },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(file, Some("sfx/explosion_v2.wav".to_string()));
            assert_eq!(gain, Some(0.5));
            assert_eq!(bus, Some(50));
            assert_eq!(priority, Some(200));
            assert_eq!(stream, Some(true));
            assert_eq!(loop_enabled, Some(true));
            assert_eq!(loop_count, Some(5));
            assert_eq!(spatialization, Some("hrtf".to_string()));
        }
        _ => panic!("Expected Asset Sound Update command"),
    }
}

#[test]
fn test_p1_sound_update_command_short_flags() {
    let args = [
        "am",
        "asset",
        "sound",
        "update",
        "explosion",
        "-f",
        "sfx/new.wav",
        "-g",
        "0.75",
        "-b",
        "25",
        "-p",
        "150",
        "-s",
        "position",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command:
                        SoundCommands::Update {
                            name,
                            file,
                            gain,
                            bus,
                            priority,
                            spatialization,
                            ..
                        },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(file, Some("sfx/new.wav".to_string()));
            assert_eq!(gain, Some(0.75));
            assert_eq!(bus, Some(25));
            assert_eq!(priority, Some(150));
            assert_eq!(spatialization, Some("position".to_string()));
        }
        _ => panic!("Expected Asset Sound Update command"),
    }
}

#[test]
fn test_p1_sound_update_command_requires_name() {
    let args = ["am", "asset", "sound", "update"];
    let result = App::try_parse_from(args);
    assert!(result.is_err(), "Should fail without name argument");
}

#[test]
fn test_p1_sound_update_with_json_flag() {
    let args = [
        "am",
        "--json",
        "asset",
        "sound",
        "update",
        "explosion",
        "--gain",
        "0.5",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.json);
    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Sound {
                    command: SoundCommands::Update { name, gain, .. },
                },
        } => {
            assert_eq!(name, "explosion");
            assert_eq!(gain, Some(0.5));
        }
        _ => panic!("Expected Asset Sound Update command"),
    }
}

// =============================================================================
// Sound Update Handler Tests
// =============================================================================

#[tokio::test]
async fn test_p0_sound_update_changes_gain_preserves_other_fields() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory and sound file
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory with audio file
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    // Create initial sound file with gain=1.0
    create_sound_file(&sounds_dir, "explosion", 12345, 1.0).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Run the update command - only update gain
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "explosion".to_string(),
            file: None,
            gain: Some(0.5),
            bus: None,
            priority: None,
            stream: None,
            loop_enabled: None,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Read back the updated sound file and verify
    let updated_content = std::fs::read_to_string(sounds_dir.join("explosion.json")).unwrap();
    let updated: Sound = serde_json::from_str(&updated_content).unwrap();

    // Gain should be updated to 0.5
    assert_eq!(
        updated.gain.as_ref().and_then(|g| g.as_static()),
        Some(0.5),
        "Gain should be updated to 0.5"
    );

    // Other fields should be preserved
    assert_eq!(updated.id, 12345, "ID should be preserved");
    assert_eq!(
        updated.name.as_deref(),
        Some("explosion"),
        "Name should be preserved"
    );
    assert_eq!(
        updated.priority.as_ref().and_then(|p| p.as_static()),
        Some(128.0),
        "Priority should be preserved"
    );
}

#[tokio::test]
async fn test_p0_sound_update_not_found_returns_error() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure WITHOUT any sound files
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Try to update a non-existent sound
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "nonexistent".to_string(),
            file: None,
            gain: Some(0.5),
            bus: None,
            priority: None,
            stream: None,
            loop_enabled: None,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify error
    assert!(
        result.is_err(),
        "Should return error for non-existent sound"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not found"),
        "Error should indicate sound not found"
    );
}

#[tokio::test]
async fn test_p1_sound_update_invalid_path_does_not_modify_original() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory with audio file
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    // Create initial sound file
    create_sound_file(&sounds_dir, "explosion", 12345, 0.8).unwrap();

    // Save original content for comparison
    let original_content = std::fs::read_to_string(sounds_dir.join("explosion.json")).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Try to update with an invalid audio file path
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "explosion".to_string(),
            file: Some("sfx/nonexistent.wav".to_string()),
            gain: None,
            bus: None,
            priority: None,
            stream: None,
            loop_enabled: None,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify error
    assert!(
        result.is_err(),
        "Should return error for invalid audio path"
    );

    // Verify original file is NOT modified
    let after_content = std::fs::read_to_string(sounds_dir.join("explosion.json")).unwrap();
    assert_eq!(
        original_content, after_content,
        "Original file should not be modified on validation failure"
    );
}

#[tokio::test]
async fn test_p1_sound_update_json_output_envelope_format() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory with audio file
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    // Create initial sound file
    create_sound_file(&sounds_dir, "explosion", 12345, 0.8).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs with JSON output
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Run the update command
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "explosion".to_string(),
            file: None,
            gain: Some(0.5),
            bus: None,
            priority: None,
            stream: None,
            loop_enabled: None,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify JSON envelope format
    let success_data = output.last_success().expect("Should have success output");
    assert!(success_data.get("id").is_some(), "Should have 'id' field");
    assert!(
        success_data.get("name").is_some(),
        "Should have 'name' field"
    );
    assert!(
        success_data.get("path").is_some(),
        "Should have 'path' field"
    );
}

#[tokio::test]
async fn test_p1_sound_update_only_updates_specified_flags() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory with audio file
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    // Create a sound with specific values
    let original_sound = serde_json::json!({
        "id": 99999,
        "name": "explosion",
        "path": "sfx/explosion.wav",
        "bus": 5,
        "gain": { "kind": "Static", "value": 0.8 },
        "priority": { "kind": "Static", "value": 200.0 },
        "stream": true,
        "loop": { "enabled": true, "loop_count": 3 },
        "spatialization": "Position",
        "attenuation": 0,
        "scope": "World",
        "fader": "Linear",
        "effect": 0
    });
    std::fs::write(
        sounds_dir.join("explosion.json"),
        serde_json::to_string_pretty(&original_sound).unwrap(),
    )
    .unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Update only gain and priority (leaving other fields unchanged)
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "explosion".to_string(),
            file: None,
            gain: Some(0.5),
            bus: None,
            priority: Some(150),
            stream: None,
            loop_enabled: None,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Read back and verify
    let updated_content = std::fs::read_to_string(sounds_dir.join("explosion.json")).unwrap();
    let updated: Sound = serde_json::from_str(&updated_content).unwrap();

    // Updated fields
    assert_eq!(
        updated.gain.as_ref().and_then(|g| g.as_static()),
        Some(0.5),
        "Gain should be updated"
    );
    assert_eq!(
        updated.priority.as_ref().and_then(|p| p.as_static()),
        Some(150.0),
        "Priority should be updated"
    );

    // Preserved fields
    assert_eq!(updated.id, 99999, "ID should be preserved");
    assert_eq!(updated.bus, 5, "Bus should be preserved");
    assert!(updated.stream, "Stream should be preserved as true");
    assert!(
        updated.loop_.as_ref().unwrap().enabled,
        "Loop enabled should be preserved"
    );
    assert_eq!(
        updated.loop_.as_ref().unwrap().loop_count,
        3,
        "Loop count should be preserved"
    );
    assert_eq!(
        updated.spatialization,
        Spatialization::Position,
        "Spatialization should be preserved"
    );
}

#[tokio::test]
async fn test_p1_sound_update_preserves_sound_id() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory with audio file
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    // Create sound with a specific ID
    let specific_id: u64 = 123456789012345678;
    let sound = serde_json::json!({
        "id": specific_id,
        "name": "explosion",
        "path": "sfx/explosion.wav",
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
        sounds_dir.join("explosion.json"),
        serde_json::to_string_pretty(&sound).unwrap(),
    )
    .unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Update sound with multiple fields
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "explosion".to_string(),
            file: None,
            gain: Some(0.5),
            bus: Some(10),
            priority: Some(200),
            stream: Some(true),
            loop_enabled: None,
            loop_count: None,
            spatialization: Some("hrtf".to_string()),
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Read back and verify ID is EXACTLY the same
    let updated_content = std::fs::read_to_string(sounds_dir.join("explosion.json")).unwrap();
    let updated: Sound = serde_json::from_str(&updated_content).unwrap();

    assert_eq!(
        updated.id, specific_id,
        "Sound ID must NEVER change during update"
    );
}

#[tokio::test]
async fn test_p1_sound_update_invalid_gain_returns_error() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory with audio file
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    // Create initial sound file
    create_sound_file(&sounds_dir, "explosion", 12345, 0.8).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Try to update with invalid gain (> 1.0)
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "explosion".to_string(),
            file: None,
            gain: Some(1.5), // Invalid: gain > 1.0
            bus: None,
            priority: None,
            stream: None,
            loop_enabled: None,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify error
    assert!(result.is_err(), "Should return error for invalid gain");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("gain") || err.to_string().contains("Gain"),
        "Error should mention gain: {}",
        err
    );
}

#[tokio::test]
async fn test_p1_sound_update_bus_field_updates_correctly() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory with audio file
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    // Create initial sound file with bus=0
    create_sound_file(&sounds_dir, "explosion", 12345, 0.8).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Update only the bus field
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "explosion".to_string(),
            file: None,
            gain: None,
            bus: Some(42), // Update bus to 42
            priority: None,
            stream: None,
            loop_enabled: None,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Read back and verify bus was updated
    let updated_content = std::fs::read_to_string(sounds_dir.join("explosion.json")).unwrap();
    let updated: Sound = serde_json::from_str(&updated_content).unwrap();

    assert_eq!(updated.bus, 42, "Bus should be updated to 42");
    // Verify other fields preserved
    assert_eq!(
        updated.gain.as_ref().and_then(|g| g.as_static()),
        Some(0.8),
        "Gain should be preserved"
    );
}

#[tokio::test]
async fn test_p1_sound_update_json_includes_updated_fields() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create data directory with audio file
    let data_dir = project_path.join("data").join("sfx");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("explosion.wav"), b"fake audio").unwrap();

    // Create initial sound file
    create_sound_file(&sounds_dir, "explosion", 12345, 0.8).unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs with JSON output
    let input = NonInteractiveInput;
    let output = CaptureOutput::json();

    // Update gain and priority
    let result = handle_sound_command(
        &SoundCommands::Update {
            name: "explosion".to_string(),
            file: None,
            gain: Some(0.5),
            bus: None,
            priority: Some(200),
            stream: None,
            loop_enabled: None,
            loop_count: None,
            spatialization: None,
        },
        None,
        &input,
        &output,
    )
    .await;

    // Restore original directory
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify JSON output includes updated_fields
    let success_data = output.last_success().expect("Should have success output");
    assert!(
        success_data.get("updated_fields").is_some(),
        "Should have 'updated_fields' field"
    );

    let updated_fields = success_data["updated_fields"]
        .as_array()
        .expect("updated_fields should be array");
    assert!(
        updated_fields.iter().any(|f| f.as_str() == Some("gain")),
        "Should include 'gain' in updated_fields"
    );
    assert!(
        updated_fields
            .iter()
            .any(|f| f.as_str() == Some("priority")),
        "Should include 'priority' in updated_fields"
    );
    assert_eq!(
        updated_fields.len(),
        2,
        "Should have exactly 2 updated fields"
    );
}

#[tokio::test]
async fn test_p1_sound_list_table_truncates_long_paths() {
    let fixture = TestProjectFixture::new("test_project").unwrap();
    let project_path = fixture.project_path();

    // Set up project structure
    std::fs::create_dir_all(project_path).unwrap();
    create_amproject(project_path, "test_project").unwrap();

    // Create sounds directory
    let sounds_dir = project_path.join("sources").join("sounds");
    std::fs::create_dir_all(&sounds_dir).unwrap();

    // Create sound with a very long path (> 40 chars)
    let long_path = "very/deeply/nested/subfolder/with/many/levels/explosion_sound_effect.wav";
    let sound = serde_json::json!({
        "id": 12345,
        "name": "long_path_sound",
        "path": long_path,
        "bus": 0,
        "gain": { "kind": "Static", "value": 0.8 },
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
        sounds_dir.join("long_path_sound.json"),
        serde_json::to_string_pretty(&sound).unwrap(),
    )
    .unwrap();

    // Change to project directory
    let original_dir = std::env::current_dir().ok();
    std::env::set_current_dir(project_path).unwrap();

    // Set up handler inputs with INTERACTIVE mode (table output truncates paths)
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    // Run the list command
    let result = handle_sound_command(&SoundCommands::List {}, None, &input, &output).await;

    // Restore original directory if we had one
    if let Some(dir) = original_dir {
        let _ = std::env::set_current_dir(dir);
    }

    // Verify result
    assert!(result.is_ok(), "Handler should succeed: {:?}", result);

    // Verify table output has truncated path
    let table_data = output.last_table().expect("Should have table output");
    let (_, data) = table_data;
    let rows = data.as_array().expect("Table data should be array");
    assert_eq!(rows.len(), 1);

    let audio_file = rows[0]["audio_file"].as_str().unwrap();
    // Path should be truncated to 40 chars max and end with "..."
    assert!(
        audio_file.len() <= 40,
        "Path should be truncated to max 40 chars, got {} chars",
        audio_file.len()
    );
    assert!(
        audio_file.ends_with("..."),
        "Truncated path should end with '...'"
    );

    // Original path was longer than 40 chars
    assert!(
        long_path.len() > 40,
        "Test precondition: original path should be > 40 chars"
    );
}
