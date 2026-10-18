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

//! Sound list and delete handlers.

use std::env;
use std::fs;

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, ProjectValidator, RtpcCompatibleValue, Sound, Spatialization},
    common::{
        errors::{CliError, asset_not_found, codes},
        utils::{read_amproject_file, truncate_string},
    },
    forms::in_use::assert_not_in_use,
    input::Input,
    presentation::{Output, OutputMode},
};

use super::find_json_files_recursive;

/// The name of the current asset.
const ASSET_NAME: &str = "Sound";

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
pub(super) async fn list_sounds(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project (validates we're in a project directory)
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Scan sounds directory using sources_dir from config
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let sounds_dir = sources_base.join("sounds");

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

    // Step 4: Read and parse all .json files recursively
    let mut sounds: Vec<Sound> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let json_files = match find_json_files_recursive(&sounds_dir) {
        Ok(files) => files,
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

    for path in json_files {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Sound>(&content) {
                    Ok(sound) => {
                        // Check if referenced audio file exists
                        let path_str = sound.path.as_deref().unwrap_or("");
                        if !path_str.is_empty() {
                            let data_base = if project_config.data_dir.is_empty() {
                                current_dir.clone()
                            } else {
                                current_dir.join(&project_config.data_dir)
                            };
                            let audio_path = data_base.join(path_str);
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
                        output.warning(&format!("Skipping invalid sound file: {}", path.display()));
                        // Provide more context for JSON errors
                        let error_msg = if let Some(line) = content.lines().next() {
                            if e.to_string().contains("column") {
                                format!(
                                    "Invalid JSON in {}: {}. First line: {}",
                                    filename,
                                    e,
                                    &line[..line.len().min(200)]
                                )
                            } else {
                                format!("Invalid JSON in {}: {}", filename, e)
                            }
                        } else {
                            format!("Invalid JSON in {}: {}", filename, e)
                        };
                        warnings.push(error_msg);
                    }
                },
                Err(e) => {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    output.warning(&format!("Failed to read sound file: {}", path.display()));
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
                        "spatialization": spatialization_to_string(&s.spatialization),
                        "spread": gain_to_json(&s.spread)
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

/// Delete a sound asset.
///
/// The audio data file under `data/` is left alone, like the other asset
/// types: only the sound definition is removed.
pub(super) async fn delete_sound(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Locate sound file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let sound_file_path = sources_base.join("sounds").join(format!("{}.json", name));

    if !sound_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset sound list' to see available sounds")
            .into());
    }

    // Step 3: Read sound for response data
    let content = fs::read_to_string(&sound_file_path).context(format!(
        "Failed to read sound file: {}",
        sound_file_path.display()
    ))?;
    let sound: Sound = serde_json::from_str(&content).context(format!(
        "Failed to parse sound file: {}",
        sound_file_path.display()
    ))?;

    // Step 4: Refuse while anything still references the sound (--force does not override)
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    assert_not_in_use(&validator, AssetType::Sound, sound.id, name)?;

    // Step 5: Confirm deletion
    let confirmed = if force {
        true
    } else {
        match input.confirm(
            &format!("Delete sound '{}'? This cannot be undone.", name),
            Some(false),
        ) {
            Ok(value) => value,
            Err(_) => {
                // Non-interactive mode without --force
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Deletion requires confirmation",
                    "In non-interactive mode, use --force to confirm deletion",
                )
                .with_suggestion(format!(
                    "Use 'am asset sound delete {} --force' to delete without prompting",
                    name
                ))
                .into());
            }
        }
    };

    if !confirmed {
        output.progress("Deletion cancelled.");
        return Ok(());
    }

    // Step 6: Remove file
    fs::remove_file(&sound_file_path).context(format!(
        "Failed to delete sound file: {}",
        sound_file_path.display()
    ))?;

    // Step 7: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": sound.id,
                    "name": sound.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Sound '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}
