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

//! Soundbank list and delete commands.

use std::env;
use std::fs;

use anyhow::Result;
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, Soundbank, SoundbankList},
    common::{
        errors::{CliError, asset_not_found, codes},
        utils::read_amproject_file,
    },
    input::Input,
    presentation::{Output, OutputMode},
};

use super::find_json_files_recursive;

/// The name of the current asset.
const ASSET_NAME: &str = "Soundbank";

/// Get all assets currently in a soundbank as (type_key, entry) pairs.
fn get_soundbank_assets(soundbank: &Soundbank) -> Vec<(String, String)> {
    SoundbankList::ALL
        .into_iter()
        .flat_map(|list| {
            soundbank
                .list(list)
                .iter()
                .map(move |entry| (list.type_key().to_string(), entry.clone()))
        })
        .collect()
}

/// Count assets by list field in a soundbank (empty lists are left out).
pub(super) fn count_assets_by_type(soundbank: &Soundbank) -> Vec<(String, usize)> {
    SoundbankList::ALL
        .into_iter()
        .filter(|list| !soundbank.list(*list).is_empty())
        .map(|list| (list.field().to_string(), soundbank.list(list).len()))
        .collect()
}

// =============================================================================
// List
// =============================================================================

/// List all soundbank assets in the current project.
pub(super) async fn list_soundbanks(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Scan soundbanks directory
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let soundbanks_dir = sources_base.join("soundbanks");

    if !soundbanks_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "soundbanks": [],
                        "count": 0,
                        "warnings": ["No soundbanks directory found. Create soundbanks with 'am asset soundbank create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No soundbanks directory found.");
                output.progress(&format!(
                    "Create soundbanks with '{}'.",
                    "am asset soundbank create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 3: Read and parse all .json files recursively
    let mut soundbanks: Vec<Soundbank> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let json_files = match find_json_files_recursive(&soundbanks_dir) {
        Ok(files) => files,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read soundbanks directory",
                format!("Permission denied on {}", soundbanks_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for path in json_files {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Soundbank>(&content) {
                    Ok(soundbank) => {
                        soundbanks.push(soundbank);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        output.warning(&format!(
                            "Skipping invalid soundbank file: {}",
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
                        "Failed to read soundbank file: {}",
                        path.display()
                    ));
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 4: Sort by name
    soundbanks.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 5: Handle empty directory
    if soundbanks.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "soundbanks": [],
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
                output.progress("No soundbanks found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset soundbank create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 6: Output based on mode
    match output.mode() {
        OutputMode::Json => {
            let soundbank_data: Vec<serde_json::Value> = soundbanks
                .iter()
                .map(|sb| {
                    let counts = count_assets_by_type(sb);
                    let type_counts: serde_json::Value = counts
                        .iter()
                        .map(|(k, v)| (k.clone(), json!(v)))
                        .collect::<serde_json::Map<String, serde_json::Value>>()
                        .into();

                    json!({
                        "id": sb.id,
                        "name": sb.name(),
                        "asset_count": sb.asset_count(),
                        "assets_by_type": type_counts,
                    })
                })
                .collect();

            output.success(
                json!({
                    "soundbanks": soundbank_data,
                    "count": soundbanks.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = soundbanks
                .iter()
                .map(|sb| {
                    let counts = count_assets_by_type(sb);
                    let type_summary = counts
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k, v))
                        .collect::<Vec<_>>()
                        .join(", ");

                    json!({
                        "id": sb.id,
                        "name": sb.name(),
                        "total": sb.asset_count(),
                        "breakdown": type_summary,
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} soundbank(s) found", soundbanks.len()));
        }
    }

    Ok(())
}

// =============================================================================
// Delete
// =============================================================================

/// Delete a soundbank asset.
pub(super) async fn delete_soundbank(
    name: &str,
    yes: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Locate soundbank file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let soundbanks_dir = sources_base.join("soundbanks");
    let soundbank_file_path = soundbanks_dir.join(format!("{}.json", name));

    if !soundbank_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset soundbank list' to see available soundbanks")
            .into());
    }

    // Step 3: Parse soundbank for details
    let content = fs::read_to_string(&soundbank_file_path)?;
    let soundbank: Soundbank = serde_json::from_str(&content)?;

    // Step 4: Check for orphaned assets (informational only)
    // An asset is "orphaned" if this is the only soundbank containing it.
    // This is informational, not a blocker.
    let orphan_info = check_orphaned_assets(&soundbank, &current_dir);

    // Step 5: Confirmation prompt
    if !yes {
        output.print(&format!(
            "\n{} You are about to delete the following soundbank:",
            "⚠".yellow()
        ));
        output.print(&format!("  Name: {}", soundbank.name()));
        output.print(&format!("  ID: {}", soundbank.id));
        output.print(&format!("  Assets: {}", soundbank.asset_count()));
        output.print(&format!("  File: {}\n", soundbank_file_path.display()));

        if !orphan_info.is_empty() {
            output.print(&format!(
                "{} The following assets will no longer be in any soundbank after deletion:",
                "ℹ".blue()
            ));
            for orphan in &orphan_info {
                output.print(&format!("  - {}", orphan));
            }
            output.print("");
        }

        let confirmed = match input.confirm(
            "Are you sure you want to delete this soundbank?",
            Some(false),
        ) {
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
    fs::remove_file(&soundbank_file_path)?;

    // Step 7: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": soundbank.id,
                    "name": soundbank.name(),
                    "deleted": true,
                    "orphaned_assets": orphan_info,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Soundbank '{}' deleted successfully", name)),
                None,
            );
            if !orphan_info.is_empty() {
                output.progress(&format!(
                    "{} {} asset(s) are no longer in any soundbank",
                    "ℹ".blue(),
                    orphan_info.len()
                ));
            }
        }
    }

    Ok(())
}

/// Check which assets would become orphaned (not in any other soundbank) after deletion.
fn check_orphaned_assets(soundbank: &Soundbank, project_root: &std::path::Path) -> Vec<String> {
    let sources_dir = match crate::common::utils::read_amproject_file(project_root) {
        Ok(config) => {
            if config.sources_dir.is_empty() {
                project_root.to_path_buf()
            } else {
                project_root.join(&config.sources_dir)
            }
        }
        Err(_) => project_root.join("sources"),
    };
    let soundbanks_dir = sources_dir.join("soundbanks");

    // Load all other soundbanks
    let mut other_assets: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Ok(entries) = fs::read_dir(&soundbanks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json")
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(other) = serde_json::from_str::<Soundbank>(&content)
            {
                // Skip self
                if other.id == soundbank.id {
                    continue;
                }
                // Collect all asset paths from other soundbanks
                for (_, asset_path) in get_soundbank_assets(&other) {
                    other_assets.insert(asset_path);
                }
            }
        }
    }

    // Find assets in this soundbank that aren't in any other
    let our_assets = get_soundbank_assets(soundbank);
    our_assets
        .into_iter()
        .filter(|(_, path)| !other_assets.contains(path))
        .map(|(type_key, path)| format!("[{}] {}", type_key, path))
        .collect()
}
