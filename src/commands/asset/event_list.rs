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

//! Event list and delete commands.

use std::env;
use std::fs;

use anyhow::Result;
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, Event, ProjectValidator},
    common::{
        errors::{CliError, asset_not_found, codes},
        utils::{read_amproject_file, truncate_string},
    },
    input::Input,
    presentation::{Output, OutputMode},
};

use super::find_json_files_recursive;

/// The name of the current asset.
const ASSET_NAME: &str = "Event";

/// Format run mode for display.
pub(super) fn format_run_mode(mode: &crate::assets::generated::EventActionRunningMode) -> &'static str {
    match mode {
        crate::assets::generated::EventActionRunningMode::Parallel => "Parallel",
        crate::assets::generated::EventActionRunningMode::Sequential => "Sequential",
    }
}

/// Maximum character length for paths before truncation in table display.
const PATH_MAX_LENGTH: usize = 40;

/// List all event assets in the current project.
pub(super) async fn list_events(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Scan events directory
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let events_dir = sources_base.join("events");

    // Step 3: Handle missing or unreadable directory
    if !events_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "events": [],
                        "count": 0,
                        "warnings": ["No events directory found. Create events with 'am asset event create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No events directory found.");
                output.progress(&format!(
                    "Create events with '{}'.",
                    "am asset event create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 4: Read and parse all .json files recursively
    let mut events: Vec<Event> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let json_files = match find_json_files_recursive(&events_dir) {
        Ok(files) => files,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read events directory",
                format!("Permission denied on {}", events_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for path in json_files {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Event>(&content) {
                    Ok(event) => {
                        events.push(event);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        output.warning(&format!("Skipping invalid event file: {}", path.display()));
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
                    output.warning(&format!("Failed to read event file: {}", path.display()));
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 5: Sort by name for consistent output
    events.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 6: Handle empty directory
    if events.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "events": [],
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
                output.progress("No events found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset event create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Build validator for resolving target names
    let validator = ProjectValidator::new(current_dir, output).ok();

    // Step 7: Output based on mode
    match output.mode() {
        OutputMode::Json => {
            let event_data: Vec<serde_json::Value> = events
                .iter()
                .map(|e| {
                    let primary_target = e
                        .actions
                        .as_ref()
                        .and_then(|a| a.first())
                        .and_then(|a| a.targets.as_ref())
                        .and_then(|t| t.first())
                        .copied()
                        .unwrap_or(0);

                    json!({
                        "id": e.id,
                        "name": e.name(),
                        "action_count": e.actions.as_ref().map(|a| a.len()).unwrap_or(0),
                        "run_mode": format_run_mode(&e.run_mode),
                        "primary_target": primary_target,
                    })
                })
                .collect();

            output.success(
                json!({
                    "events": event_data,
                    "count": events.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            // Build table data
            let table_data: Vec<serde_json::Value> = events
                .iter()
                .map(|e| {
                    let action_count = e.actions.as_ref().map(|a| a.len()).unwrap_or(0);
                    let primary_action = e.actions.as_ref().and_then(|a| a.first());

                    let primary_target_str = primary_action
                        .and_then(|a| {
                            let target_count = a.targets.as_ref()?.len();
                            if target_count == 0 {
                                Some("none".to_string())
                            } else if target_count == 1 {
                                a.targets.as_ref().map(|t| t[0].to_string())
                            } else {
                                let first_target = a.targets.as_ref().unwrap()[0];
                                Some(format!("{} (+{})", first_target, target_count - 1))
                            }
                        })
                        .unwrap_or_else(|| "none".to_string());

                    json!({
                        "id": e.id,
                        "name": e.name(),
                        "actions": action_count,
                        "target": truncate_string(&primary_target_str, PATH_MAX_LENGTH),
                        "mode": format_run_mode(&e.run_mode),
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} event(s) found", events.len()));
        }
    }

    Ok(())
}

/// Delete an event asset.
pub(super) async fn delete_event(
    name: &str,
    yes: bool,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Locate event file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let events_dir = sources_base.join("events");
    let event_file_path = events_dir.join(format!("{}.json", name));

    if !event_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset event list' to see available events")
            .into());
    }

    // Step 3: Parse event to get details
    let content = fs::read_to_string(&event_file_path)?;
    let event: Event = serde_json::from_str(&content)?;

    // Step 4: Check for dependencies (soundbanks that include this event)
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let dependent_soundbanks: Vec<String> = validator
        .asset_ids
        .get(&AssetType::Soundbank)
        .map(|ids| ids.iter().map(|id| id.to_string()).collect())
        .unwrap_or_default();

    // Step 5: Confirmation prompt
    if !yes {
        output.print(&format!(
            "\n{} You are about to delete the following event:",
            "⚠".yellow()
        ));
        output.print(&format!("  Name: {}", event.name()));
        output.print(&format!("  ID: {}", event.id));
        output.print(&format!(
            "  Actions: {}",
            event.actions.as_ref().map(|a| a.len()).unwrap_or(0)
        ));
        output.print(&format!("  File: {}\n", event_file_path.display()));

        if !dependent_soundbanks.is_empty() && !force {
            output.print(&format!(
                "{} This event is included in {} soundbank(s).",
                "⚠".yellow(),
                dependent_soundbanks.len()
            ));
            output.print("  Use --force to delete anyway.\n");
        }

        let confirmed =
            match input.confirm("Are you sure you want to delete this event?", Some(false)) {
                Ok(val) => val,
                Err(_) => {
                    return Err(CliError::new(
                        codes::ERR_VALIDATION_FIELD,
                        "Deletion requires confirmation",
                        "The --yes flag is required in non-interactive mode",
                    )
                    .with_suggestion("Use --yes to confirm deletion")
                    .into());
                }
            };

        if !confirmed {
            output.progress("Deletion cancelled.");
            return Ok(());
        }
    }

    // Step 6: Delete the file
    fs::remove_file(&event_file_path)?;

    // Step 7: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": event.id,
                    "name": event.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Event '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}
