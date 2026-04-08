//! Sound asset commands.
//!
//! Implements CRUD operations for Sound assets in Amplitude projects.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use inquire::validator::Validation;
use serde_json::json;

use crate::common::utils::generate_unique_id;
use crate::{
    assets::{
        Asset, ProjectContext, ProjectValidator, RtpcCompatibleValue, Sound, SoundLoopConfig,
        Spatialization,
    },
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        utils::{read_amproject_file, truncate_string},
    },
    database::Database,
    input::{Input, select_index},
    presentation::{Output, OutputMode},
};

use super::parse_spatialization;

/// The name of the current asset.
///
/// Used in error messages and other outputs.
const ASSET_NAME: &str = "Sound";

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

        /// Volume gain (0.0-1.0, omit for default: 1.0; invalid values are rejected)
        #[arg(short, long)]
        gain: Option<f32>,

        /// Bus ID for audio routing (omit for default: 0 = master)
        #[arg(short, long)]
        bus: Option<u64>,

        /// Playback priority (0-255, omit for default: 128; invalid values are rejected)
        #[arg(short, long)]
        priority: Option<u8>,

        /// Stream from disk instead of loading into memory (default: false)
        #[arg(long)]
        stream: bool,

        /// Enable looping (default: disabled)
        #[arg(long = "loop")]
        loop_enabled: bool,

        /// Number of times to loop (0 = infinite, requires --loop)
        #[arg(long)]
        loop_count: Option<u32>,

        /// Spatialization mode: none, position, position_orientation, hrtf (default: none)
        #[arg(short, long)]
        spatialization: Option<String>,
    },

    /// List all sound assets in the project
    List {},

    /// Update an existing sound asset
    Update {
        /// Name of the sound asset to update
        name: String,

        /// New path to audio file relative to data/ directory
        #[arg(short, long)]
        file: Option<String>,

        /// New volume gain (0.0-1.0)
        #[arg(short, long)]
        gain: Option<f32>,

        /// New bus ID for audio routing
        #[arg(short, long)]
        bus: Option<u64>,

        /// New playback priority (0-255)
        #[arg(short, long)]
        priority: Option<u8>,

        /// Set streaming mode (true/false)
        #[arg(long)]
        stream: Option<bool>,

        /// Enable or disable looping (true/false)
        #[arg(long = "loop")]
        loop_enabled: Option<bool>,

        /// Number of times to loop (0 = infinite, requires --loop=true)
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
        SoundCommands::List {} => list_sounds(output).await,
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
        } => {
            update_sound(
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

    output.progress(&format!(
        "Creating sound '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Validate sound name doesn't already exist (filesystem + registry)
    let sounds_dir = current_dir.join("sources").join("sounds");
    let sound_file_path = sounds_dir.join(format!("{}.json", name));

    if sound_file_path.exists() {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset sound update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Build populated ProjectContext for validation (used throughout)
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Check name uniqueness via ProjectContext registry
    if context.has_name(crate::assets::AssetType::Sound, name) {
        return Err(asset_already_exists(ASSET_NAME, name)
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
    let stream_value = if stream { true } else { prompt_stream(input)? };

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

    // Step 11: Generate unique ID and check for collisions against all project assets
    let mut id = generate_unique_id(name);
    let mut retries = 0;
    while context.has_id(id) && retries < 3 {
        id = generate_unique_id(&format!("{}{}", name, retries));
        retries += 1;
    }
    if context.has_id(id) {
        return Err(CliError::new(
            codes::ERR_ASSET_ALREADY_EXISTS,
            format!("Generated ID {} collides with an existing asset", id),
            "All generated ID attempts collided with existing assets in the project",
        )
        .with_suggestion("Try a different name or wait a moment and retry")
        .into());
    }

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

    // Step 13: Validate the sound (type rules) with populated context (built in step 2)
    sound.validate_rules(&context)?;

    // Step 14: Serialize to JSON
    let json_content =
        serde_json::to_string_pretty(&sound).context("Failed to serialize sound to JSON")?;

    // Step 15: Write using atomic write pattern
    atomic_write(&sound_file_path, json_content.as_bytes())?;

    // Step 16: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": sound.id,
                    "name": sound.name(),
                    "path": sound_file_path.to_string_lossy(),
                    "audio_file": sound.path.as_deref().unwrap_or(""),
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

/// Maximum character length for paths before truncation in table display.
const PATH_MAX_LENGTH: usize = 40;

/// Format gain value for display in interactive mode.
///
/// Returns the static value formatted to 1 decimal place, "RTPC" if RTPC-controlled,
/// or "N/A" if not set.
fn format_gain(gain: &Option<RtpcCompatibleValue>) -> String {
    match gain {
        Some(g) => match g.as_static() {
            Some(v) => format!("{:.1}", v),
            None => "RTPC".to_string(),
        },
        None => "N/A".to_string(),
    }
}

/// Format gain value for JSON output.
///
/// Returns a numeric value when static, "RTPC" when RTPC-controlled, or null when not set.
fn gain_to_json(gain: &Option<RtpcCompatibleValue>) -> serde_json::Value {
    match gain {
        Some(g) => match g.as_static() {
            Some(v) => json!(v),
            None => json!("RTPC"),
        },
        None => serde_json::Value::Null,
    }
}

/// Format spatialization mode as a stable string for JSON output.
fn spatialization_to_string(spatialization: &Spatialization) -> &'static str {
    match spatialization {
        Spatialization::None => "None",
        Spatialization::Position => "Position",
        Spatialization::PositionOrientation => "PositionOrientation",
        Spatialization::HRTF => "HRTF",
    }
}

/// List all sound assets in the current project.
async fn list_sounds(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project (validates we're in a project directory)
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Scan sounds directory
    let sounds_dir = current_dir.join("sources").join("sounds");

    // Step 3: Handle missing or unreadable directory
    if !sounds_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "sounds": [],
                        "count": 0,
                        "warnings": ["No sounds directory found. Create sounds with 'am asset sound create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No sounds directory found.");
                output.progress(&format!(
                    "Create sounds with '{}'.",
                    "am asset sound create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 4: Read and parse all .json files
    let mut sounds: Vec<Sound> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let entries = match fs::read_dir(&sounds_dir) {
        Ok(entries) => entries,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read sounds directory",
                format!("Permission denied on {}", sounds_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Sound>(&content) {
                    Ok(sound) => {
                        // Check if referenced audio file exists
                        let path_str = sound.path.as_deref().unwrap_or("");
                        if !path_str.is_empty() {
                            let audio_path = current_dir.join("data").join(path_str);
                            if !audio_path.exists() {
                                warnings.push(format!(
                                    "Warning: Sound '{}' references missing audio file: {}. Re-add the file or update the sound.",
                                    sound.name(),
                                    path_str
                                ));
                            }
                        }
                        sounds.push(sound);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        log::warn!("Skipping invalid sound file: {}", path.display());
                        warnings.push(format!("Invalid JSON in {}: {}", filename, e));
                    }
                },
                Err(e) => {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    log::warn!("Failed to read sound file: {}", path.display());
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 4: Sort by name for consistent output
    sounds.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 5: Handle empty directory
    if sounds.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "sounds": [],
                        "count": 0,
                        "warnings": warnings
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                // Print warnings first so the user knows why no sounds were found
                for warning in &warnings {
                    output.progress(&format!("{} {}", "Warning:".yellow(), warning));
                }
                if !warnings.is_empty() {
                    output.progress("");
                }
                output.progress("No sounds found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset sound create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 6: Output based on mode
    match output.mode() {
        OutputMode::Json => {
            let sound_data: Vec<serde_json::Value> = sounds
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "name": s.name(),
                        "path": s.path.as_deref().unwrap_or(""),
                        "gain": gain_to_json(&s.gain),
                        "loop_enabled": s.loop_.as_ref().map(|l| l.enabled).unwrap_or(false),
                        "spatialization": spatialization_to_string(&s.spatialization)
                    })
                })
                .collect();

            output.success(
                json!({
                    "sounds": sound_data,
                    "count": sounds.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            // Print warnings if any
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            // Build table data
            let table_data: Vec<serde_json::Value> = sounds
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "name": s.name(),
                        "audio_file": truncate_string(s.path.as_deref().unwrap_or(""), PATH_MAX_LENGTH),
                        "gain": format_gain(&s.gain)
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} sound(s) found", sounds.len()));
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
///
/// Only called when --gain flag was NOT provided (field omitted).
/// Uses default of 1.0 in non-interactive mode, which is correct
/// because omitted fields should get defaults (AC #5).
fn prompt_gain(input: &dyn Input) -> Result<f32> {
    let result = input.prompt_text(
        "Volume gain [0.0-1.0]",
        Some("1.0"),
        None,
        Some(&|value: &str| match value.trim().parse::<f32>() {
            Ok(g) if (0.0..=1.0).contains(&g) => Ok(Validation::Valid),
            Ok(g) => Ok(Validation::Invalid(
                format!("Gain must be between 0.0 and 1.0, got {}", g).into(),
            )),
            Err(_) => Ok(Validation::Invalid("Must be a number".into())),
        }),
    );

    match result {
        Ok(value) => Ok(value.trim().parse().unwrap_or(1.0)),
        Err(_) => Ok(1.0), // Default to 1.0 when field omitted in non-interactive mode
    }
}

/// Prompt for priority value.
///
/// Only called when --priority flag was NOT provided (field omitted).
/// Uses default of 128 in non-interactive mode, which is correct
/// because omitted fields should get defaults (AC #5).
fn prompt_priority(input: &dyn Input) -> Result<u8> {
    let result = input.prompt_text(
        "Playback priority [0-255]",
        Some("128"),
        None,
        Some(&|value: &str| match value.trim().parse::<u8>() {
            Ok(_) => Ok(Validation::Valid),
            Err(_) => Ok(Validation::Invalid(
                "Must be a number between 0 and 255".into(),
            )),
        }),
    );

    match result {
        Ok(value) => Ok(value.trim().parse().unwrap_or(128)),
        Err(_) => Ok(128), // Default to 128 when field omitted in non-interactive mode
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
        Some(&|value: &str| match value.trim().parse::<u32>() {
            Ok(_) => Ok(Validation::Valid),
            Err(_) => Ok(Validation::Invalid("Must be a non-negative number".into())),
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
        Spatialization::HRTF,
    ];

    match select_index(input, "Spatialization mode:", &options) {
        Ok(idx) => Ok(modes[idx]),
        Err(_) => Ok(Spatialization::None), // Default in non-interactive mode
    }
}

/// Update an existing sound asset.
#[allow(clippy::too_many_arguments)]
async fn update_sound(
    name: &str,
    file: Option<String>,
    gain: Option<f32>,
    bus: Option<u64>,
    priority: Option<u8>,
    stream: Option<bool>,
    loop_enabled: Option<bool>,
    loop_count: Option<u32>,
    spatialization: Option<String>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating sound '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Locate existing sound file
    let sounds_dir = current_dir.join("sources").join("sounds");
    let sound_file_path = sounds_dir.join(format!("{}.json", name));

    if !sound_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset sound list' to see available sounds, or 'am asset sound create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing sound
    let content = fs::read_to_string(&sound_file_path).context(format!(
        "Failed to read sound file: {}",
        sound_file_path.display()
    ))?;
    let mut sound: Sound = serde_json::from_str(&content).context(format!(
        "Failed to parse sound file: {}",
        sound_file_path.display()
    ))?;

    // Step 4: Determine if we have any flag values (non-interactive mode)
    let has_any_flag = file.is_some()
        || gain.is_some()
        || bus.is_some()
        || priority.is_some()
        || stream.is_some()
        || loop_enabled.is_some()
        || loop_count.is_some()
        || spatialization.is_some();

    // Step 5: Apply updates based on mode, tracking which fields changed
    let updated_fields: Vec<String> = if has_any_flag {
        // Non-interactive mode: only update fields provided via flags
        apply_flag_updates(
            &mut sound,
            file,
            gain,
            bus,
            priority,
            stream,
            loop_enabled,
            loop_count,
            spatialization,
        )?
    } else {
        // Interactive mode: prompt for each field with current values as defaults
        prompt_sound_updates(&mut sound, input)?
    };

    // Step 6: Validate the updated sound with populated context
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    sound.validate_rules(&context)?;

    // Step 7: Serialize and write atomically
    let json_content =
        serde_json::to_string_pretty(&sound).context("Failed to serialize sound to JSON")?;
    atomic_write(&sound_file_path, json_content.as_bytes())?;

    // Step 8: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": sound.id,
                    "name": sound.name(),
                    "path": sound_file_path.to_string_lossy(),
                    "audio_file": sound.path.as_deref().unwrap_or(""),
                    "updated_fields": updated_fields,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Sound '{}' updated successfully at {}",
                    name,
                    sound_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Apply flag updates to a sound (non-interactive mode).
/// Returns a list of field names that were updated.
fn apply_flag_updates(
    sound: &mut Sound,
    file: Option<String>,
    gain: Option<f32>,
    bus: Option<u64>,
    priority: Option<u8>,
    stream: Option<bool>,
    loop_enabled: Option<bool>,
    loop_count: Option<u32>,
    spatialization: Option<String>,
) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    if let Some(f) = file {
        sound.path = Some(f);
        updated_fields.push("audio_file".to_string());
    }

    if let Some(g) = gain {
        validate_gain(g)?;
        sound.gain = Some(RtpcCompatibleValue::static_value(g));
        updated_fields.push("gain".to_string());
    }

    if let Some(b) = bus {
        sound.bus = b;
        updated_fields.push("bus".to_string());
    }

    if let Some(p) = priority {
        sound.priority = Some(RtpcCompatibleValue::static_value(p as f32));
        updated_fields.push("priority".to_string());
    }

    if let Some(s) = stream {
        sound.stream = s;
        updated_fields.push("stream".to_string());
    }

    if let Some(enabled) = loop_enabled {
        let loop_cfg = sound.loop_.get_or_insert(SoundLoopConfig::disabled());
        loop_cfg.enabled = enabled;
        updated_fields.push("loop_enabled".to_string());
    }

    if let Some(count) = loop_count {
        let loop_cfg = sound.loop_.get_or_insert(SoundLoopConfig::disabled());
        loop_cfg.loop_count = count;
        // Auto-enable looping if count is specified
        if count > 0 && !loop_cfg.enabled {
            loop_cfg.enabled = true;
        }
        updated_fields.push("loop_count".to_string());
    }

    if let Some(s) = spatialization {
        sound.spatialization = parse_spatialization(&s)?;
        updated_fields.push("spatialization".to_string());
    }

    Ok(updated_fields)
}

/// Prompt for sound updates in interactive mode.
/// Returns a list of field names that were updated.
fn prompt_sound_updates(sound: &mut Sound, input: &dyn Input) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    // Prompt for audio file path
    let current_path = sound.path.as_deref().unwrap_or("");
    if let Some(new_file) = prompt_update_text(input, "Audio file path", current_path)? {
        sound.path = Some(new_file);
        updated_fields.push("audio_file".to_string());
    }

    // Prompt for gain
    let current_gain = sound
        .gain
        .as_ref()
        .and_then(|g| g.as_static())
        .unwrap_or(1.0);
    if let Some(new_gain) = prompt_update_number(
        input,
        "Volume gain [0.0-1.0]",
        current_gain,
        |v| (0.0..=1.0).contains(&v),
        "Gain must be between 0.0 and 1.0",
    )? {
        sound.gain = Some(RtpcCompatibleValue::static_value(new_gain));
        updated_fields.push("gain".to_string());
    }

    // Prompt for priority
    let current_priority = sound
        .priority
        .as_ref()
        .and_then(|p| p.as_static())
        .unwrap_or(128.0) as u8;
    if let Some(new_priority) = prompt_update_number(
        input,
        "Playback priority [0-255]",
        current_priority as f32,
        |_| true, // u8 parse already validates range
        "Priority must be between 0 and 255",
    )? {
        sound.priority = Some(RtpcCompatibleValue::static_value(new_priority));
        updated_fields.push("priority".to_string());
    }

    // Prompt for streaming
    if let Some(new_stream) = prompt_update_bool(input, "Stream from disk", sound.stream)? {
        sound.stream = new_stream;
        updated_fields.push("stream".to_string());
    }

    // Prompt for looping
    let loop_cfg = sound.loop_.get_or_insert(SoundLoopConfig::disabled());
    let current_loop_enabled = loop_cfg.enabled;
    let current_loop_count = loop_cfg.loop_count;
    if let Some(new_loop) = prompt_update_bool(input, "Enable looping", current_loop_enabled)? {
        let loop_cfg = sound.loop_.get_or_insert(SoundLoopConfig::disabled());
        loop_cfg.enabled = new_loop;
        updated_fields.push("loop_enabled".to_string());
        if new_loop {
            // Ask for loop count
            if let Some(new_count) = prompt_update_number(
                input,
                "Loop count (0=infinite)",
                current_loop_count as f32,
                |_| true,
                "",
            )? {
                let loop_cfg = sound.loop_.get_or_insert(SoundLoopConfig::disabled());
                if new_count < 0.0 {
                    return Err(CliError::new(
                        crate::common::errors::codes::ERR_VALIDATION_FIELD,
                        "Loop count cannot be negative",
                        "Loop count must be a non-negative integer (0 = infinite)",
                    )
                    .into());
                }
                if new_count > u32::MAX as f32 {
                    return Err(CliError::new(
                        crate::common::errors::codes::ERR_VALIDATION_FIELD,
                        format!("Loop count too large: maximum is {}", u32::MAX),
                        "Loop count exceeds maximum allowed value",
                    )
                    .into());
                }
                loop_cfg.loop_count = new_count as u32;
                updated_fields.push("loop_count".to_string());
            }
        }
    }

    // Prompt for spatialization
    if let Some(new_spat) = prompt_update_spatialization(input, &sound.spatialization)? {
        sound.spatialization = new_spat;
        updated_fields.push("spatialization".to_string());
    }

    Ok(updated_fields)
}

/// Prompt for an optional text field update.
/// Returns Some(new_value) if user provided a new value, None to keep current.
fn prompt_update_text(
    input: &dyn Input,
    label: &str,
    current_value: &str,
) -> Result<Option<String>> {
    let prompt = format!("{} (current: {}, Enter to keep)", label, current_value);
    match input.prompt_text(&prompt, Some(current_value), None, None) {
        Ok(value) if value == current_value => Ok(None), // No change
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}

/// Prompt for an optional numeric field update.
fn prompt_update_number(
    input: &dyn Input,
    label: &str,
    current_value: f32,
    validator: impl Fn(f32) -> bool,
    error_msg: &str,
) -> Result<Option<f32>> {
    let current_str = format!("{}", current_value);
    let prompt = format!("{} (current: {}, Enter to keep)", label, current_value);

    let error_msg = error_msg.to_string();
    let result = input.prompt_text(
        &prompt,
        Some(&current_str),
        None,
        Some(&move |value: &str| match value.trim().parse::<f32>() {
            Ok(v) if validator(v) => Ok(Validation::Valid),
            Ok(_) => Ok(Validation::Invalid(error_msg.clone().into())),
            Err(_) => Ok(Validation::Invalid("Must be a number".into())),
        }),
    );

    match result {
        Ok(value) => {
            let parsed: f32 = value.trim().parse().unwrap_or(current_value);
            if (parsed - current_value).abs() < f32::EPSILON {
                Ok(None) // No change
            } else {
                Ok(Some(parsed))
            }
        }
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}

/// Prompt for an optional boolean field update.
fn prompt_update_bool(input: &dyn Input, label: &str, current_value: bool) -> Result<Option<bool>> {
    let prompt = format!(
        "{} (current: {})",
        label,
        if current_value { "yes" } else { "no" }
    );
    match input.confirm(&prompt, Some(current_value)) {
        Ok(value) if value == current_value => Ok(None), // No change
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}

/// Prompt for spatialization update.
fn prompt_update_spatialization(
    input: &dyn Input,
    current: &Spatialization,
) -> Result<Option<Spatialization>> {
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
        Spatialization::HRTF,
    ];

    let current_idx = modes.iter().position(|m| m == current).unwrap_or(0);
    let prompt = format!("Spatialization mode (current: {}):", options[current_idx]);

    match select_index(input, &prompt, &options) {
        Ok(idx) if idx == current_idx => Ok(None), // No change
        Ok(idx) => Ok(Some(modes[idx])),
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}
