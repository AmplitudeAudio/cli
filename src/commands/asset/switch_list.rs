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

//! Switch list and delete commands.

use std::env;
use std::fs;

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, ProjectValidator, Switch},
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
const ASSET_NAME: &str = "Switch";

// =============================================================================
// List
// =============================================================================

/// List all switch assets in the current project.
pub(super) async fn list_switches(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Scan switches directory
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let switches_dir = sources_base.join("switches");

    // Step 3: Handle missing directory
    if !switches_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "switches": [],
                        "count": 0,
                        "warnings": ["No switches directory found. Create switches with 'am asset switch create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No switches directory found.");
                output.progress(&format!(
                    "Create switches with '{}'.",
                    "am asset switch create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 4: Read and parse all .json files recursively
    let mut switches: Vec<Switch> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let json_files = match find_json_files_recursive(&switches_dir) {
        Ok(files) => files,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read switches directory",
                format!("Permission denied on {}", switches_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for path in json_files {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Switch>(&content) {
                    Ok(switch) => {
                        switches.push(switch);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        output
                            .warning(&format!("Skipping invalid switch file: {}", path.display()));
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
                    output.warning(&format!("Failed to read switch file: {}", path.display()));
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 5: Sort by name
    switches.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 6: Handle empty
    if switches.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "switches": [],
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
                output.progress("No switches found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset switch create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 7: Output
    match output.mode() {
        OutputMode::Json => {
            let switch_data: Vec<serde_json::Value> = switches
                .iter()
                .map(|s| {
                    let state_count = s.states.as_ref().map(|v| v.len()).unwrap_or(0);
                    json!({
                        "id": s.id,
                        "name": s.name(),
                        "state_count": state_count,
                        "states": s.states.as_ref().map(|states| {
                            states.iter().map(|st| {
                                json!({
                                    "id": st.id,
                                    "name": st.name.as_deref().unwrap_or("")
                                })
                            }).collect::<Vec<_>>()
                        }).unwrap_or_default()
                    })
                })
                .collect();

            output.success(
                json!({
                    "switches": switch_data,
                    "count": switches.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = switches
                .iter()
                .map(|s| {
                    let state_count = s.states.as_ref().map(|v| v.len()).unwrap_or(0);
                    json!({
                        "id": s.id,
                        "name": s.name(),
                        "states": state_count,
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} switch(es) found", switches.len()));
        }
    }

    Ok(())
}

// =============================================================================
// Delete
// =============================================================================

/// Delete a switch asset.
pub(super) async fn delete_switch(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Locate switch file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let switches_dir = sources_base.join("switches");
    let switch_file_path = switches_dir.join(format!("{}.json", name));

    if !switch_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset switch list' to see available switches")
            .into());
    }

    // Step 3: Read switch for response data
    let content = fs::read_to_string(&switch_file_path).context(format!(
        "Failed to read switch file: {}",
        switch_file_path.display()
    ))?;
    let switch: Switch = serde_json::from_str(&content).context(format!(
        "Failed to parse switch file: {}",
        switch_file_path.display()
    ))?;

    // Step 4: Refuse while switch containers or soundbanks still reference the switch
    // (--force does not override)
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    assert_not_in_use(&validator, AssetType::Switch, switch.id, name)?;

    // Step 5: Confirm deletion
    let confirmed = if force {
        true
    } else {
        let prompt = format!("Delete switch '{}'? This cannot be undone.", name);

        match input.confirm(&prompt, Some(false)) {
            Ok(value) => value,
            Err(_) => {
                // Non-interactive mode without --force
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Deletion requires confirmation",
                    "In non-interactive mode, use --force to confirm deletion",
                )
                .with_suggestion(format!(
                    "Use 'am asset switch delete {} --force' to delete without prompting",
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
    fs::remove_file(&switch_file_path).context(format!(
        "Failed to delete switch file: {}",
        switch_file_path.display()
    ))?;

    // Step 7: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": switch.id,
                    "name": switch.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Switch '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}
