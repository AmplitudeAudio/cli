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

//! Collection list and delete handlers.

use std::env;
use std::fs;

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, Collection, RtpcCompatibleValue, Spatialization},
    common::{
        errors::{CliError, asset_not_found, codes},
        utils::read_amproject_file,
    },
    input::Input,
    presentation::{Output, OutputMode},
};

use super::find_json_files_recursive;

/// The name of the current asset.
const ASSET_NAME: &str = "Collection";

/// Format gain value for JSON output.
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

/// List all collection assets in the current project.
pub(super) async fn list_collections(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Scan collections directory
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let collections_dir = sources_base.join("collections");

    // Step 3: Handle missing directory
    if !collections_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "collections": [],
                        "count": 0,
                        "warnings": ["No collections directory found. Create collections with 'am asset collection create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No collections directory found.");
                output.progress(&format!(
                    "Create collections with '{}'.",
                    "am asset collection create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 4: Read and parse all .json files recursively
    let mut collections: Vec<Collection> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let json_files = match find_json_files_recursive(&collections_dir) {
        Ok(files) => files,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read collections directory",
                format!("Permission denied on {}", collections_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for path in json_files {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Collection>(&content) {
                    Ok(collection) => {
                        collections.push(collection);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        output.warning(&format!(
                            "Skipping invalid collection file: {}",
                            path.display()
                        ));
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
                    output.warning(&format!(
                        "Failed to read collection file: {}",
                        path.display()
                    ));
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 5: Sort by name
    collections.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 6: Handle empty
    if collections.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "collections": [],
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
                output.progress("No collections found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset collection create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 7: Output
    match output.mode() {
        OutputMode::Json => {
            let collection_data: Vec<serde_json::Value> = collections
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "name": c.name(),
                        "play_mode": c.play_mode.to_string(),
                        "scheduler_mode": c.scheduler.as_ref().map(|s| s.mode.to_string()).unwrap_or_default(),
                        "gain": gain_to_json(&c.gain),
                        "spatialization": spatialization_to_string(&c.spatialization),
                        "entry_count": c.sounds.as_ref().map_or(0, Vec::len)
                    })
                })
                .collect();

            output.success(
                json!({
                    "collections": collection_data,
                    "count": collections.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = collections
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "name": c.name(),
                        "play_mode": c.play_mode.to_string(),
                        "scheduler": c.scheduler.as_ref().map(|s| s.mode.to_string()).unwrap_or_default()
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} collection(s) found", collections.len()));
        }
    }

    Ok(())
}

/// Delete a collection asset.
pub(super) async fn delete_collection(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Locate collection file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let collections_dir = sources_base.join("collections");
    let collection_file_path = collections_dir.join(format!("{}.json", name));

    if !collection_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset collection list' to see available collections")
            .into());
    }

    // Step 3: Read collection for response data
    let content = fs::read_to_string(&collection_file_path).context(format!(
        "Failed to read collection file: {}",
        collection_file_path.display()
    ))?;
    let collection: Collection = serde_json::from_str(&content).context(format!(
        "Failed to parse collection file: {}",
        collection_file_path.display()
    ))?;

    // Step 4: Confirm deletion
    let confirmed = if force {
        true
    } else {
        match input.confirm(
            &format!("Delete collection '{}'? This cannot be undone.", name),
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
                    "Use 'am asset collection delete {} --force' to delete without prompting",
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
    fs::remove_file(&collection_file_path).context(format!(
        "Failed to delete collection file: {}",
        collection_file_path.display()
    ))?;

    // Step 6: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": collection.id,
                    "name": collection.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Collection '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}
