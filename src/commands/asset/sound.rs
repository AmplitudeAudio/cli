//! Sound asset commands.
//!
//! Implements CRUD operations for Sound assets in Amplitude projects.

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::Subcommand;
use inquire::validator::Validation;
use serde_json::json;

use crate::{
    assets::{Asset, ProjectContext, Sound, SoundLoopConfig, Spatialization},
    common::{
        errors::{asset_already_exists, codes, CliError},
        files::atomic_write,
        utils::read_amproject_file,
    },
    database::Database,
    input::{select_index, Input},
    presentation::{Output, OutputMode},
};

/// Sound asset subcommands.
#[derive(Subcommand, Debug)]
pub enum SoundCommands {
    /// Create a new sound asset
    Create {
        /// Name of the sound asset
        name: String,

        /// Path to audio file relative to data/ directory
        #[arg(short, long)]
        file: Option<String>,

        /// Volume gain (0.0-1.0, default: 1.0)
        #[arg(short, long)]
        gain: Option<f32>,

        /// Bus ID for audio routing (default: 0 = master)
        #[arg(short, long)]
        bus: Option<u64>,

        /// Playback priority (0-255, default: 128)
        #[arg(short, long)]
        priority: Option<u8>,

        /// Stream from disk instead of loading into memory
        #[arg(long)]
        stream: bool,

        /// Enable looping
        #[arg(long = "loop")]
        loop_enabled: bool,

        /// Number of times to loop (0 = infinite, requires --loop)
        #[arg(long)]
        loop_count: Option<u32>,

        /// Spatialization mode: none, position, position_orientation, hrtf
        #[arg(short, long)]
        spatialization: Option<String>,
    },
}

/// Handle sound commands by routing to the appropriate handler.
pub async fn handler(
    command: &SoundCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
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
        } => {
            create_sound(
                name,
                file.clone(),
                *gain,
                *bus,
                *priority,
                *stream,
                *loop_enabled,
                *loop_count,
                spatialization.clone(),
                input,
                output,
            )
            .await
        }
    }
}

/// Generate a unique ID for a sound asset.
///
/// Uses a combination of the sound name and current timestamp to generate
/// a unique u64 identifier.
fn generate_unique_id(name: &str) -> u64 {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    timestamp.hash(&mut hasher);
    hasher.finish()
}

/// Parse spatialization mode from string.
fn parse_spatialization(s: &str) -> Result<Spatialization> {
    match s.to_lowercase().as_str() {
        "none" => Ok(Spatialization::None),
        "position" => Ok(Spatialization::Position),
        "position_orientation" | "positionorientation" => Ok(Spatialization::PositionOrientation),
        "hrtf" => Ok(Spatialization::Hrtf),
        _ => Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid spatialization mode: '{}'", s),
            "Spatialization must be one of: none, position, position_orientation, hrtf",
        )
        .into()),
    }
}

/// Create a new sound asset.
#[allow(clippy::too_many_arguments)]
async fn create_sound(
    name: &str,
    file: Option<String>,
    gain: Option<f32>,
    bus: Option<u64>,
    priority: Option<u8>,
    stream: bool,
    loop_enabled: bool,
    loop_count: Option<u32>,
    spatialization: Option<String>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!("Creating sound '{}' in project '{}'...", name, project_config.name));

    // Step 2: Validate sound name doesn't already exist
    let sounds_dir = current_dir.join("sources").join("sounds");
    let sound_file_path = sounds_dir.join(format!("{}.json", name));

    if sound_file_path.exists() {
        return Err(asset_already_exists("Sound", name)
            .with_suggestion(format!(
                "Use 'am asset sound update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Step 3: Get audio file path (prompt if not provided)
    let audio_file = if let Some(f) = file {
        f
    } else {
        prompt_audio_file(input)?
    };

    // Step 4: Validate audio file exists
    let audio_full_path = current_dir.join("data").join(&audio_file);
    if !audio_full_path.exists() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Audio file not found: {}", audio_file),
            "The audio file must exist in the project's data/ directory",
        )
        .with_suggestion(format!(
            "Add the audio file at: {}",
            audio_full_path.display()
        ))
        .into());
    }

    // Step 5: Get gain value (prompt if not provided)
    let gain_value = if let Some(g) = gain {
        validate_gain(g)?;
        g
    } else {
        prompt_gain(input)?
    };

    // Step 6: Get priority value (prompt if not provided)
    let priority_value = if let Some(p) = priority {
        p
    } else {
        prompt_priority(input)?
    };

    // Step 7: Get stream value (already from flag, or prompt)
    let stream_value = if stream {
        true
    } else {
        prompt_stream(input)?
    };

    // Step 8: Get loop configuration
    let loop_config = if loop_enabled {
        let count = loop_count.unwrap_or(0);
        if count == 0 {
            SoundLoopConfig::infinite()
        } else {
            SoundLoopConfig::count(count)
        }
    } else {
        prompt_loop_config(input)?
    };

    // Step 9: Get spatialization mode
    let spatialization_mode = if let Some(s) = spatialization {
        parse_spatialization(&s)?
    } else {
        prompt_spatialization(input)?
    };

    // Step 10: Get bus ID (use default or provided)
    let bus_id = bus.unwrap_or(0);

    // Step 11: Generate unique ID
    let id = generate_unique_id(name);

    // Step 12: Build the Sound asset
    let sound = Sound::builder(id, name)
        .path(&audio_file)
        .bus(bus_id)
        .gain(gain_value)
        .priority(priority_value)
        .stream(stream_value)
        .loop_config(loop_config)
        .spatialization(spatialization_mode)
        .build();

    // Step 13: Validate the sound (type rules)
    let context = ProjectContext::new(current_dir.clone());
    sound.validate_rules(&context)?;

    // Step 14: Serialize to JSON
    let json_content = serde_json::to_string_pretty(&sound)
        .context("Failed to serialize sound to JSON")?;

    // Step 15: Write using atomic write pattern
    atomic_write(&sound_file_path, json_content.as_bytes())?;

    // Step 16: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": sound.id,
                    "name": sound.name,
                    "path": sound_file_path.to_string_lossy(),
                    "audio_file": sound.path.to_string_lossy(),
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Sound '{}' created successfully at {}",
                    name,
                    sound_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Validate gain is in valid range.
fn validate_gain(gain: f32) -> Result<()> {
    if !(0.0..=1.0).contains(&gain) {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid gain value: {}", gain),
            "Gain must be between 0.0 and 1.0",
        )
        .with_suggestion("Set gain to a value between 0.0 (silent) and 1.0 (full volume)")
        .into());
    }
    Ok(())
}

/// Prompt for audio file path.
fn prompt_audio_file(input: &dyn Input) -> Result<String> {
    input
        .prompt_text(
            "Path to audio file (relative to data/)",
            Some("sfx/sound.wav"),
            None,
            Some(&|value: &str| {
                if value.trim().is_empty() {
                    return Ok(Validation::Invalid("Audio file path is required".into()));
                }
                Ok(Validation::Valid)
            }),
        )
        .map_err(|e| {
            // In non-interactive mode, provide a helpful error
            if e.to_string().contains("non-interactive") || e.to_string().contains("blocked") {
                CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Audio file path is required",
                    "The --file flag must be provided in non-interactive mode",
                )
                .with_suggestion("Use --file <path> to specify the audio file path")
                .into()
            } else {
                e
            }
        })
}

/// Prompt for gain value.
fn prompt_gain(input: &dyn Input) -> Result<f32> {
    let result = input.prompt_text(
        "Volume gain [0.0-1.0]",
        Some("1.0"),
        None,
        Some(&|value: &str| {
            match value.trim().parse::<f32>() {
                Ok(g) if (0.0..=1.0).contains(&g) => Ok(Validation::Valid),
                Ok(g) => Ok(Validation::Invalid(
                    format!("Gain must be between 0.0 and 1.0, got {}", g).into(),
                )),
                Err(_) => Ok(Validation::Invalid("Must be a number".into())),
            }
        }),
    );

    match result {
        Ok(value) => Ok(value.trim().parse().unwrap_or(1.0)),
        Err(_) => Ok(1.0), // Default to 1.0 in non-interactive mode
    }
}

/// Prompt for priority value.
fn prompt_priority(input: &dyn Input) -> Result<u8> {
    let result = input.prompt_text(
        "Playback priority [0-255]",
        Some("128"),
        None,
        Some(&|value: &str| {
            match value.trim().parse::<u8>() {
                Ok(_) => Ok(Validation::Valid),
                Err(_) => Ok(Validation::Invalid("Must be a number between 0 and 255".into())),
            }
        }),
    );

    match result {
        Ok(value) => Ok(value.trim().parse().unwrap_or(128)),
        Err(_) => Ok(128), // Default to 128 in non-interactive mode
    }
}

/// Prompt for stream preference.
fn prompt_stream(input: &dyn Input) -> Result<bool> {
    match input.confirm("Stream from disk?", Some(false)) {
        Ok(value) => Ok(value),
        Err(_) => Ok(false), // Default to false in non-interactive mode
    }
}

/// Prompt for loop configuration.
fn prompt_loop_config(input: &dyn Input) -> Result<SoundLoopConfig> {
    let loop_enabled = match input.confirm("Enable looping?", Some(false)) {
        Ok(value) => value,
        Err(_) => return Ok(SoundLoopConfig::disabled()), // Default in non-interactive mode
    };

    if !loop_enabled {
        return Ok(SoundLoopConfig::disabled());
    }

    // Ask for loop count
    let result = input.prompt_text(
        "Loop count (0=infinite)",
        Some("0"),
        None,
        Some(&|value: &str| {
            match value.trim().parse::<u32>() {
                Ok(_) => Ok(Validation::Valid),
                Err(_) => Ok(Validation::Invalid("Must be a non-negative number".into())),
            }
        }),
    );

    match result {
        Ok(value) => {
            let count: u32 = value.trim().parse().unwrap_or(0);
            if count == 0 {
                Ok(SoundLoopConfig::infinite())
            } else {
                Ok(SoundLoopConfig::count(count))
            }
        }
        Err(_) => Ok(SoundLoopConfig::infinite()), // Default to infinite in non-interactive mode
    }
}

/// Prompt for spatialization mode.
fn prompt_spatialization(input: &dyn Input) -> Result<Spatialization> {
    let options = vec![
        "None".to_string(),
        "Position".to_string(),
        "Position + Orientation".to_string(),
        "HRTF".to_string(),
    ];

    let modes = [
        Spatialization::None,
        Spatialization::Position,
        Spatialization::PositionOrientation,
        Spatialization::Hrtf,
    ];

    match select_index(input, "Spatialization mode:", &options) {
        Ok(idx) => Ok(modes[idx]),
        Err(_) => Ok(Spatialization::None), // Default in non-interactive mode
    }
}
