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
            command: AssetCommands::Sound {
                command: SoundCommands::Create { name, file, gain, .. },
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
        "am", "asset", "sound", "create", "explosion",
        "--file", "sfx/explosion.wav",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command: AssetCommands::Sound {
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
        "am", "asset", "sound", "create", "explosion",
        "--gain", "0.8",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command: AssetCommands::Sound {
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
        "am", "asset", "sound", "create", "explosion",
        "--file", "sfx/explosion.wav",
        "--gain", "0.8",
        "--bus", "100",
        "--priority", "200",
        "--stream",
        "--loop",
        "--loop-count", "3",
        "--spatialization", "position",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command: AssetCommands::Sound {
                command: SoundCommands::Create {
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
        "am", "asset", "sound", "create", "explosion",
        "-f", "sfx/explosion.wav",
        "-g", "0.75",
        "-b", "50",
        "-p", "100",
        "-s", "hrtf",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command: AssetCommands::Sound {
                command: SoundCommands::Create {
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
        "am", "--non-interactive",
        "asset", "sound", "create", "explosion",
        "--file", "sfx/explosion.wav",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.non_interactive);
    match app.command {
        Commands::Asset {
            command: AssetCommands::Sound {
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
        "am", "--json",
        "asset", "sound", "create", "explosion",
        "--file", "sfx/explosion.wav",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    assert!(app.json);
    match app.command {
        Commands::Asset {
            command: AssetCommands::Sound {
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
    assert_eq!(sound.name, "explosion");
    assert_eq!(sound.path.to_string_lossy(), "sfx/explosion.wav");
    assert_eq!(sound.gain.as_static(), Some(0.8));
    assert_eq!(sound.priority.as_static(), Some(200.0));
    assert!(sound.stream);
    assert!(sound.loop_config.enabled);
    assert_eq!(sound.loop_config.loop_count, 0);
    assert_eq!(sound.spatialization, Spatialization::Position);
}

#[test]
fn test_p0_sound_default_values() {
    let sound = Sound::builder(1, "test")
        .path("test.wav")
        .build();

    assert_eq!(sound.gain.as_static(), Some(1.0));
    assert_eq!(sound.priority.as_static(), Some(128.0));
    assert!(!sound.stream);
    assert!(!sound.loop_config.enabled);
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
    assert_eq!(Spatialization::PositionOrientation, Spatialization::PositionOrientation);
    assert_eq!(Spatialization::Hrtf, Spatialization::Hrtf);
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
    assert_eq!(sound.name, "footstep");
    assert_eq!(sound.gain.as_static(), Some(0.9));
    assert_eq!(sound.spatialization, Spatialization::Position);
}

// =============================================================================
// Validation Tests
// =============================================================================

use am::assets::ProjectContext;
use tempfile::tempdir;
use std::fs;

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
    let context = ProjectContext::new(dir.path().to_path_buf());

    let sound = Sound::builder(1, "test")
        .path("")
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
    let context = ProjectContext::new(dir.path().to_path_buf());

    let sound = Sound::builder(1, "test")
        .path("")
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
