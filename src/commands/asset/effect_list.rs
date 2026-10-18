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

//! Effect list and delete commands.

use std::env;
use std::fs;

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, Effect, ProjectValidator},
    common::{
        errors::{CliError, asset_not_found, codes},
        utils::read_amproject_file,
    },
    forms::in_use::assert_not_in_use,
    input::Input,
    presentation::{Output, OutputMode},
};

use super::find_json_files_recursive;

/// The name of the current asset.
const ASSET_NAME: &str = "Effect";

// =============================================================================
// List
// =============================================================================

/// List all effect assets in the current project.
pub(super) async fn list_effects(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Scan effects directory using sources_dir from config
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let effects_dir = sources_base.join("effects");

    // Step 3: Handle missing directory
    if !effects_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "effects": [],
                        "count": 0,
                        "warnings": ["No effects directory found. Create effects with 'am asset effect create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No effects directory found.");
                output.progress(&format!(
                    "Create effects with '{}'.",
                    "am asset effect create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 4: Read and parse all .json files recursively
    let mut effects: Vec<Effect> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let json_files = match find_json_files_recursive(&effects_dir) {
        Ok(files) => files,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read effects directory",
                format!("Permission denied on {}", effects_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for path in json_files {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Effect>(&content) {
                    Ok(effect) => {
                        effects.push(effect);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        output
                            .warning(&format!("Skipping invalid effect file: {}", path.display()));
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
                    output.warning(&format!("Failed to read effect file: {}", path.display()));
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 5: Sort by name
    effects.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 6: Handle empty
    if effects.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "effects": [],
                        "count": 0,
                        "warnings": warnings
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                for warning in &warnings {
                    output.progress(&format!("{} {}", "Warning:".yellow(), warning));
                }
                if !warnings.is_empty() {
                    output.progress("");
                }
                output.progress("No effects found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset effect create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 7: Output
    match output.mode() {
        OutputMode::Json => {
            let effect_data: Vec<serde_json::Value> = effects
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id,
                        "name": e.name(),
                        "effect_type": e.effect,
                        "parameter_count": e.parameters.as_ref().map(|p| p.len()).unwrap_or(0)
                    })
                })
                .collect();

            output.success(
                json!({
                    "effects": effect_data,
                    "count": effects.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = effects
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id,
                        "name": e.name(),
                        "effect_type": e.effect.as_deref().unwrap_or("")
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} effect(s) found", effects.len()));
        }
    }

    Ok(())
}

// =============================================================================
// Delete
// =============================================================================

/// Delete an effect asset.
pub(super) async fn delete_effect(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Locate effect file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let effects_dir = sources_base.join("effects");
    let effect_file_path = effects_dir.join(format!("{}.json", name));

    if !effect_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset effect list' to see available effects")
            .into());
    }

    // Step 3: Read effect for response data
    let content = fs::read_to_string(&effect_file_path).context(format!(
        "Failed to read effect file: {}",
        effect_file_path.display()
    ))?;
    let effect: Effect = serde_json::from_str(&content).context(format!(
        "Failed to parse effect file: {}",
        effect_file_path.display()
    ))?;

    // Step 4: Refuse while anything still references the effect (--force does not override)
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    assert_not_in_use(&validator, AssetType::Effect, effect.id, name)?;

    // Step 5: Confirm deletion
    let confirmed = if force {
        true
    } else {
        match input.confirm(
            &format!("Delete effect '{}'? This cannot be undone.", name),
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
                    "Use 'am asset effect delete {} --force' to delete without prompting",
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

    // Step 5: Remove file
    fs::remove_file(&effect_file_path).context(format!(
        "Failed to delete effect file: {}",
        effect_file_path.display()
    ))?;

    // Step 6: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": effect.id,
                    "name": effect.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Effect '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}
