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

//! Sound asset commands.
//!
//! Implements CRUD operations for Sound assets in Amplitude projects. The
//! create/update handlers only gather values (flags, then prompts when the
//! session is interactive); building and merging lives in `services::sound`.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, ProjectContext, ProjectValidator, Sound},
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        json_merge::merge_update_unknown_only,
        utils::read_amproject_file,
    },
    database::Database,
    forms::coverage::known_field_names,
    input::Input,
    presentation::{Output, OutputMode},
    services::sound::{SoundPatch, SoundValues, apply_patch, build_sound},
};

use super::sound_args::{
    SoundExtraArgs, SoundFlags, bus_warnings, resolve_patch, validate_patch_references,
    with_warnings,
};
use super::sound_list::{delete_sound, list_sounds};
use super::sound_prompts::{
    complete_create_patch, prompt_audio_file, prompt_bus, prompt_update_patch,
};

/// The name of the current asset.
///
/// Used in error messages and other outputs.
const ASSET_NAME: &str = "Sound";

/// Sound asset subcommands.
#[derive(Subcommand, Debug)]
pub enum SoundCommands {
    /// Create a new sound asset
    #[command(after_help = "Examples:
  am asset sound create explosion --file audio/explosion.wav --bus sfx
  am asset sound create footstep --file audio/step.wav --bus sfx --gain 0.8 --loop
  am asset sound create ambient --bus sfx --stream --spatialization position
  am asset sound create hit --bus sfx --attenuation short --scope entity --pitch 1.2
")]
    Create {
        /// Name of the sound asset
        name: String,

        /// Path to audio file relative to data/ directory
        #[arg(short, long)]
        file: Option<String>,

        /// Volume gain (0.0-1.0, omit for default: 1.0; invalid values are rejected)
        #[arg(short, long)]
        gain: Option<f32>,

        /// Bus to route the sound to, by name or ID (required; see 'am asset bus list')
        #[arg(short, long)]
        bus: Option<String>,

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

        /// Stereo/multichannel image width in degrees (0-360); 0 collapses channels to the sound's direction
        #[arg(long)]
        spread: Option<f32>,

        #[command(flatten)]
        extra: SoundExtraArgs,
    },

    /// List all sound assets in the project
    #[command(after_help = "Examples:
  am asset sound list
  am asset sound list --json
")]
    List {},

    /// Update an existing sound asset
    #[command(after_help = "Examples:
  am asset sound update explosion --gain 0.5
  am asset sound update ambient --file audio/new_ambient.wav --stream
  am asset sound update hit --effect echo --scope entity
")]
    Update {
        /// Name of the sound asset to update
        name: String,

        /// New path to audio file relative to data/ directory
        #[arg(short, long)]
        file: Option<String>,

        /// New volume gain (0.0-1.0)
        #[arg(short, long)]
        gain: Option<f32>,

        /// New bus, by name or ID
        #[arg(short, long)]
        bus: Option<String>,

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

        /// Stereo/multichannel image width in degrees (0-360); 0 collapses channels to the sound's direction
        #[arg(long)]
        spread: Option<f32>,

        #[command(flatten)]
        extra: SoundExtraArgs,
    },

    /// Delete a sound asset (refused while collections, switch containers, events or soundbanks use it)
    #[command(after_help = "Examples:
  am asset sound delete explosion
  am asset sound delete explosion --force
")]
    Delete {
        /// Name of the sound asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode; alias: --yes)
        #[arg(long, visible_alias = "yes")]
        force: bool,
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
            spread,
            extra,
        } => {
            let flags = SoundFlags {
                file: file.clone(),
                gain: *gain,
                bus: bus.clone(),
                priority: *priority,
                stream: stream.then_some(true),
                loop_enabled: loop_enabled.then_some(true),
                loop_count: *loop_count,
                spatialization: spatialization.clone(),
                spread: *spread,
                extra: extra.clone(),
            };
            create_sound(name, flags, input, output).await
        }
        SoundCommands::List {} => list_sounds(output).await,
        SoundCommands::Delete { name, force } => delete_sound(name, *force, input, output).await,
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
            spread,
            extra,
        } => {
            let flags = SoundFlags {
                file: file.clone(),
                gain: *gain,
                bus: bus.clone(),
                priority: *priority,
                stream: *stream,
                loop_enabled: *loop_enabled,
                loop_count: *loop_count,
                spatialization: spatialization.clone(),
                spread: *spread,
                extra: extra.clone(),
            };
            update_sound(name, flags, input, output).await
        }
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

fn missing_bus_error() -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_EMPTY_REFERENCE,
        "Missing required --bus",
        "Sounds must be routed to a bus and cannot prompt in non-interactive or JSON mode",
    )
    .with_suggestion(
        "Pass --bus <name|id> (see 'am asset bus list'); if the project has no bus yet, create a bus first",
    )
}

fn missing_file_error() -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        "Audio file path is required",
        "The --file flag must be provided in non-interactive mode",
    )
    .with_suggestion("Use --file <path> to specify the audio file path")
}

/// Create a new sound asset.
async fn create_sound(
    name: &str,
    flags: SoundFlags,
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
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let sounds_dir = sources_base.join("sounds");
    let sound_file_path = sounds_dir.join(format!("{}.json", name));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset sound update {}' to modify it, or choose a different name",
            name
        ))
    };

    if sound_file_path.exists() {
        return Err(already_exists().into());
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;
    if context.has_name(crate::assets::AssetType::Sound, name) {
        return Err(already_exists().into());
    }

    // Step 3: Resolve flags; unknown names and dangling ids fail before any prompt
    let patch = resolve_patch(&flags, validator)?;
    validate_patch_references(&patch, validator)?;
    let interactive = can_prompt(input, output);

    // Step 4: Required fields, bus first so a missing bus fails fast
    let bus = match (patch.bus, interactive) {
        (Some(bus), _) => bus,
        (None, true) => prompt_bus(input, validator)?,
        (None, false) => return Err(missing_bus_error().into()),
    };
    let audio_file = match (&patch.audio_file, interactive) {
        (Some(file), _) => file.clone(),
        (None, true) => prompt_audio_file(input)?,
        (None, false) => return Err(missing_file_error().into()),
    };

    let data_base = if project_config.data_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.data_dir)
    };
    let audio_full_path = data_base.join(&audio_file);
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
    let patch = SoundPatch {
        bus: Some(bus),
        audio_file: Some(audio_file),
        ..patch
    };

    // Step 5: Optional fields, interactive only
    let patch = if interactive {
        complete_create_patch(input, validator, patch)?
    } else {
        patch
    };

    // Step 6: Build and validate through the service, then write atomically
    let sound = build_sound(SoundValues::from_patch(name, &patch)?, &context)?;
    let json_content =
        serde_json::to_string_pretty(&sound).context("Failed to serialize sound to JSON")?;
    atomic_write(&sound_file_path, json_content.as_bytes())?;

    // Step 7: Output success
    let warnings = bus_warnings(validator, Some(sound.bus), output);
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": sound.id,
                        "name": sound.name(),
                        "path": sound_file_path.to_string_lossy(),
                        "audio_file": sound.path.as_deref().unwrap_or(""),
                    }),
                    &warnings,
                ),
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

/// Update an existing sound asset.
async fn update_sound(
    name: &str,
    flags: SoundFlags,
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
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let sounds_dir = sources_base.join("sounds");
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
    let existing: Sound = serde_json::from_str(&content).context(format!(
        "Failed to parse sound file: {}",
        sound_file_path.display()
    ))?;
    let original: serde_json::Value = serde_json::from_str(&content).context(format!(
        "Failed to parse sound file: {}",
        sound_file_path.display()
    ))?;

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;

    // Step 4: Flags win; with none, an interactive session is offered every field
    let mut patch = resolve_patch(&flags, validator)?;
    if patch.is_empty() && can_prompt(input, output) {
        patch = prompt_update_patch(input, validator, &existing)?;
    }

    // Step 5: Merge into a copy and validate the new state
    let sound = apply_patch(&existing, &patch);
    sound.validate_rules(&context).map_err(|e| {
        if e.field.as_deref() == Some("bus") {
            let suggestion = format!(
                "Run 'am asset sound update {name} --bus <name|id>' with an existing bus (see 'am asset bus list')"
            );
            e.with_suggestion(suggestion)
        } else {
            e
        }
    })?;

    // Step 6: Serialize over the original file, keeping keys the CLI does not model
    let known = known_field_names(AssetType::Sound);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let updated = serde_json::to_value(&sound).context("Failed to serialize sound to JSON")?;
    let merged = merge_update_unknown_only(&original, &updated, &known);
    let json_content =
        serde_json::to_string_pretty(&merged).context("Failed to serialize sound to JSON")?;
    atomic_write(&sound_file_path, json_content.as_bytes())?;

    // Step 7: Output success
    let warnings = bus_warnings(validator, patch.bus, output);
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": sound.id,
                        "name": sound.name(),
                        "path": sound_file_path.to_string_lossy(),
                        "audio_file": sound.path.as_deref().unwrap_or(""),
                        "updated_fields": patch.changed_fields(),
                    }),
                    &warnings,
                ),
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
