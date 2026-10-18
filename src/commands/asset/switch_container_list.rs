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

//! Switch container list and delete handlers.

use std::env;
use std::fs;

use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, ProjectContext, ProjectValidator, SwitchContainer},
    common::{
        errors::{CliError, asset_not_found, codes},
        utils::read_amproject_file,
    },
    forms::references::RefTarget,
    input::Input,
    presentation::{Output, OutputMode},
};

use super::find_json_files_recursive;

/// The name of the current asset.
const ASSET_NAME: &str = "Switch Container";

/// ID value meaning "no reference" in the SDK.
const NO_REFERENCE: u64 = 0;

/// List all switch container assets in the current project.
pub(super) async fn list_switch_containers(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Scan switch_containers directory
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let containers_dir = sources_base.join("switch_containers");

    // Step 3: Handle missing directory
    if !containers_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "switch_containers": [],
                        "count": 0,
                        "warnings": ["No switch containers directory found. Create switch containers with 'am asset switchcontainer create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No switch containers directory found.");
                output.progress(&format!(
                    "Create switch containers with '{}'.",
                    "am asset switchcontainer create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Build context for resolving references
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Step 4: Read and parse all .json files recursively
    let mut containers: Vec<SwitchContainer> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let json_files = match find_json_files_recursive(&containers_dir) {
        Ok(files) => files,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read switch containers directory",
                format!("Permission denied on {}", containers_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for path in json_files {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<SwitchContainer>(&content) {
                    Ok(container) => {
                        containers.push(container);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        output.warning(&format!(
                            "Skipping invalid switch container file: {}",
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
                        "Failed to read switch container file: {}",
                        path.display()
                    ));
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 5: Sort by name
    containers.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 6: Handle empty
    if containers.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "switch_containers": [],
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
                output.progress("No switch containers found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset switchcontainer create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 7: Output
    match output.mode() {
        OutputMode::Json => {
            let container_data: Vec<serde_json::Value> = containers
                .iter()
                .map(|c| {
                    let entry_count = c.entries.as_ref().map(|v| v.len()).unwrap_or(0);
                    let switch_name = get_switch_name(&context, c.switch_group);
                    json!({
                        "id": c.id,
                        "name": c.name(),
                        "switch_group_id": c.switch_group,
                        "switch_group_name": switch_name,
                        "mapping_count": entry_count,
                        "default_state": c.default_switch_state,
                    })
                })
                .collect();

            output.success(
                json!({
                    "switch_containers": container_data,
                    "count": containers.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = containers
                .iter()
                .map(|c| {
                    let entry_count = c.entries.as_ref().map(|v| v.len()).unwrap_or(0);
                    let switch_name = get_switch_name(&context, c.switch_group);
                    json!({
                        "id": c.id,
                        "name": c.name(),
                        "switch": switch_name,
                        "mappings": entry_count,
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} switch container(s) found", containers.len()));
        }
    }

    Ok(())
}


/// Get switch name by ID for display purposes.
fn get_switch_name(context: &ProjectContext, switch_id: u64) -> String {
    if switch_id == NO_REFERENCE {
        return "<none>".to_string();
    }
    context
        .validator
        .as_ref()
        .and_then(|v| v.resolve(RefTarget::Asset(AssetType::Switch), switch_id))
        .map_or_else(|| format!("ID:{}", switch_id), |s| s.name)
}

// =============================================================================
// Delete
// =============================================================================

/// Delete a switch container asset.
pub(super) async fn delete_switch_container(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Locate container file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let containers_dir = sources_base.join("switch_containers");
    let container_file_path = containers_dir.join(format!("{}.json", name));

    if !container_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset switchcontainer list' to see available containers")
            .into());
    }

    // Step 3: Read container for response data
    let content = fs::read_to_string(&container_file_path).context(format!(
        "Failed to read switch container file: {}",
        container_file_path.display()
    ))?;
    let container: SwitchContainer = serde_json::from_str(&content).context(format!(
        "Failed to parse switch container file: {}",
        container_file_path.display()
    ))?;

    // Step 4: Check for references (events and soundbanks - placeholder for now)
    let has_references = check_container_references(&container);

    // Step 5: Confirm deletion
    let confirmed = if force {
        true
    } else {
        let mut prompt = format!("Delete switch container '{}'? This cannot be undone.", name);
        if has_references {
            prompt.push_str("\n⚠️  Warning: This container is referenced by events or soundbanks.");
        }

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
                    "Use 'am asset switchcontainer delete {} --force' to delete without prompting",
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
    fs::remove_file(&container_file_path).context(format!(
        "Failed to delete switch container file: {}",
        container_file_path.display()
    ))?;

    // Step 7: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": container.id,
                    "name": container.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Switch container '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}

/// Check if a switch container is referenced by events or soundbanks.
fn check_container_references(_container: &SwitchContainer) -> bool {
    // Placeholder - will be implemented when events and soundbanks are available
    // For now, return false to not block deletion
    false
}

