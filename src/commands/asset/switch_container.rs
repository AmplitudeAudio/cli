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

//! Switch Container asset commands.
//!
//! Implements CRUD operations for SwitchContainer assets in Amplitude projects.

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
        generated::SwitchContainerEntry, Asset, AssetType, ProjectContext, ProjectValidator,
        SwitchContainer,
    },
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        utils::read_amproject_file,
    },
    database::Database,
    input::Input,
    presentation::{Output, OutputMode},
};

/// The name of the current asset.
const ASSET_NAME: &str = "Switch Container";

/// Maximum number of ID generation retries before giving up.
const MAX_ID_RETRIES: u32 = 3;

/// ID value meaning "no reference" in the SDK.
const NO_REFERENCE: u64 = 0;

/// Switch container asset subcommands.
#[derive(Subcommand, Debug)]
pub enum SwitchContainerCommands {
    /// Create a new switch container asset
    #[command(after_help = "Examples:\n  am asset switch-container create footsteps\n  am asset switch-container create footsteps --switch surface_type --map wood=wood_step\n")]
    Create {
        /// Name of the switch container asset
        name: String,

        /// Controlling switch name (required in non-interactive mode)
        #[arg(long)]
        switch: Option<String>,

        /// State-to-sound mappings in format state_name=sound_name (repeatable)
        #[arg(long = "map", value_name = "STATE=SOUND")]
        mappings: Option<Vec<String>>,
    },

    /// List all switch container assets in the project
    #[command(after_help = "Examples:\n  am asset switch-container list\n  am asset switch-container list --json\n")]
    List {},

    /// Update an existing switch container asset
    #[command(after_help = "Examples:\n  am asset switch-container update footsteps\n  am asset switch-container update footsteps --map stone=stone_step\n")]
    Update {
        /// Name of the switch container asset to update
        name: String,

        /// State-to-sound mappings to add/update in format state_name=sound_name (repeatable)
        #[arg(long = "map", value_name = "STATE=SOUND")]
        mappings: Option<Vec<String>>,
    },

    /// Delete a switch container asset
    #[command(after_help = "Examples:\n  am asset switch-container delete footsteps\n  am asset switch-container delete footsteps --force\n")]
    Delete {
        /// Name of the switch container asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle switch container commands by routing to the appropriate handler.
pub async fn handler(
    command: &SwitchContainerCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        SwitchContainerCommands::Create {
            name,
            switch,
            mappings,
        } => {
            create_switch_container(name, switch.clone(), mappings.clone(), input, output).await
        }
        SwitchContainerCommands::List {} => list_switch_containers(output).await,
        SwitchContainerCommands::Update { name, mappings } => {
            update_switch_container(name, mappings.clone(), input, output).await
        }
        SwitchContainerCommands::Delete { name, force } => {
            delete_switch_container(name, *force, input, output).await
        }
    }
}

// =============================================================================
// Create
// =============================================================================

/// Create a new switch container asset.
async fn create_switch_container(
    name: &str,
    switch: Option<String>,
    mappings: Option<Vec<String>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Validate name is not empty
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Switch container name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset switchcontainer create <name>'")
        .into());
    }

    // Step 2: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating switch container '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 3: Validate switch container name doesn't already exist
    let switch_containers_dir = current_dir.join("sources").join("switch_containers");
    let container_file_path = switch_containers_dir.join(format!("{}.json", name));

    if container_file_path.exists() {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset switchcontainer update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Build populated ProjectContext for validation
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Check name uniqueness via ProjectContext registry
    if context.has_name(AssetType::SwitchContainer, name) {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset switchcontainer update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Step 4: Get the controlling switch
    let switch_info = if let Some(switch_name) = switch {
        // Non-interactive mode: validate the switch exists
        find_switch_by_name(&context, &switch_name)?
            .ok_or_else(|| {
                CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    format!("Switch '{}' does not exist", switch_name),
                    "The controlling switch must be an existing switch asset",
                )
                .with_suggestion("Use 'am asset switch list' to see available switches")
            })?
    } else {
        // Interactive mode: prompt for switch selection
        prompt_switch_selection(input, &context)?
    };

    // Step 5: Get state-to-sound mappings
    let entries = if let Some(mapping_list) = mappings {
        // Non-interactive mode: parse mappings
        parse_mappings(&mapping_list, &switch_info, &context)?
    } else {
        // Interactive mode: prompt for mappings
        prompt_mappings(input, output, &switch_info, &context)?
    };

    // Step 6: Generate unique ID for switch container
    let mut container_id = generate_unique_id(name);
    let mut retries = 0;
    while context.has_id(container_id) && retries < MAX_ID_RETRIES {
        container_id = generate_unique_id(&format!("{}{}", name, retries));
        retries += 1;
    }
    if context.has_id(container_id) {
        return Err(CliError::new(
            codes::ERR_ASSET_ALREADY_EXISTS,
            format!("Generated ID {} collides with an existing asset", container_id),
            "All generated ID attempts collided with existing assets in the project",
        )
        .with_suggestion("Try a different name or wait a moment and retry")
        .into());
    }

    // Step 7: Build the SwitchContainer asset
    // Use the first state as default if available, otherwise 0
    let default_state = switch_info
        .states
        .first()
        .map(|s| s.id)
        .unwrap_or(NO_REFERENCE);

    let container = SwitchContainer::builder(container_id, name)
        .switch_group(switch_info.id)
        .default_state(default_state)
        .entries(entries)
        .build();

    // Step 8: Validate
    container.validate_rules(&context)?;

    // Step 9: Serialize to JSON
    let json_content = serde_json::to_string_pretty(&container)
        .context("Failed to serialize switch container to JSON")?;

    // Step 10: Ensure directory exists and write atomically
    fs::create_dir_all(&switch_containers_dir)?;
    atomic_write(&container_file_path, json_content.as_bytes())?;

    // Step 11: Output success
    let entry_count = container.entries.as_ref().map(|e| e.len()).unwrap_or(0);
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": container.id,
                    "name": container.name(),
                    "path": container_file_path.to_string_lossy(),
                    "switch_group": switch_info.name,
                    "mapping_count": entry_count,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Switch container '{}' created successfully at {}\nControlling switch: {} ({} mappings)",
                    name,
                    container_file_path.display(),
                    switch_info.name,
                    entry_count
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Information about a switch for building switch containers.
struct SwitchInfo {
    id: u64,
    name: String,
    states: Vec<SwitchStateInfo>,
}

/// Information about a switch state.
struct SwitchStateInfo {
    id: u64,
    name: String,
}

/// Find a switch by name and return its info.
fn find_switch_by_name(context: &ProjectContext, name: &str) -> Result<Option<SwitchInfo>> {
    let switches_dir = context.project_root.join("sources").join("switches");
    let switch_file = switches_dir.join(format!("{}.json", name));

    if !switch_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&switch_file)
        .with_context(|| format!("Failed to read switch file: {}", switch_file.display()))?;
    let switch: crate::assets::Switch = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse switch file: {}", switch_file.display()))?;

    let states: Vec<SwitchStateInfo> = switch
        .states
        .as_ref()
        .map(|states| {
            states
                .iter()
                .filter_map(|s| {
                    Some(SwitchStateInfo {
                        id: s.id,
                        name: s.name.as_deref()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Some(SwitchInfo {
        id: switch.id,
        name: switch.name().to_string(),
        states,
    }))
}

/// Get all available switches in the project.
fn get_available_switches(context: &ProjectContext) -> Result<Vec<SwitchInfo>> {
    let switches_dir = context.project_root.join("sources").join("switches");

    if !switches_dir.exists() {
        return Ok(Vec::new());
    }

    let mut switches = Vec::new();

    for entry in fs::read_dir(&switches_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<crate::assets::Switch>(&content) {
                    Ok(switch) => {
                        let states: Vec<SwitchStateInfo> = switch
                            .states
                            .as_ref()
                            .map(|states| {
                                states
                                    .iter()
                                    .filter_map(|s| {
                                        Some(SwitchStateInfo {
                                            id: s.id,
                                            name: s.name.as_deref()?.to_string(),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();

                        switches.push(SwitchInfo {
                            id: switch.id,
                            name: switch.name().to_string(),
                            states,
                        });
                    }
                    Err(e) => {
                        log::warn!("Failed to parse switch file: {} - {}", path.display(), e);
                    }
                },
                Err(e) => {
                    log::warn!("Failed to read switch file: {} - {}", path.display(), e);
                }
            }
        }
    }

    switches.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(switches)
}

/// Prompt user to select a switch in interactive mode.
fn prompt_switch_selection(input: &dyn Input, context: &ProjectContext) -> Result<SwitchInfo> {
    let switches = get_available_switches(context)?;

    if switches.is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "No switches available",
            "Switch containers require a controlling switch, but no switches exist in this project",
        )
        .with_suggestion("Create a switch first with 'am asset switch create <name>'")
        .into());
    }

    let switch_names: Vec<String> = switches.iter().map(|s| s.name.clone()).collect();

    let selected_idx = crate::input::select_index(
        input,
        "Select the controlling switch for this container:",
        &switch_names,
    )?;

    Ok(switches.into_iter().nth(selected_idx).unwrap())
}

/// Parse state-to-sound mappings from command line arguments.
fn parse_mappings(
    mappings: &[String],
    switch_info: &SwitchInfo,
    context: &ProjectContext,
) -> Result<Vec<SwitchContainerEntry>> {
    let mut entries: Vec<SwitchContainerEntry> = Vec::new();

    for mapping in mappings {
        let parts: Vec<&str> = mapping.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("Invalid mapping format: '{}'", mapping),
                "Mappings must be in format 'state_name=sound_name'",
            )
            .with_suggestion("Use --map state_name=sound_name (e.g., --map wood=footstep_wood)")
            .into());
        }

        let state_name = parts[0].trim();
        let sound_name = parts[1].trim();

        // Find the state ID
        let state_id = switch_info
            .states
            .iter()
            .find(|s| s.name == state_name)
            .map(|s| s.id)
            .ok_or_else(|| {
                let valid_states: Vec<String> =
                    switch_info.states.iter().map(|s| s.name.clone()).collect();
                CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    format!("Invalid state name: '{}'", state_name),
                    format!(
                        "State '{}' does not exist in switch '{}'",
                        state_name, switch_info.name
                    ),
                )
                .with_suggestion(format!(
                    "Valid states for this switch: {}",
                    valid_states.join(", ")
                ))
            })?;

        // Find the sound/collection ID
        let sound_id = find_asset_id_by_name(context, sound_name)?
            .ok_or_else(|| {
                CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    format!("Sound/Collection '{}' not found", sound_name),
                    "The mapped sound or collection does not exist in the project",
                )
                .with_suggestion(
                    "Use 'am asset sound list' or 'am asset collection list' to see available assets",
                )
            })?;

        // Check if we already have an entry for this sound
        if let Some(existing) = entries.iter_mut().find(|e| e.object == sound_id) {
            // Add this state to the existing entry
            if !existing.switch_states.contains(&state_id) {
                existing.switch_states.push(state_id);
            }
        } else {
            // Create a new entry
            entries.push(SwitchContainerEntry {
                object: sound_id,
                switch_states: vec![state_id],
                continue_between_states: false,
                fade_in: None,
                fade_out: None,
                gain: None,
                pitch: None,
            });
        }
    }

    Ok(entries)
}

/// Prompt user for state-to-sound mappings in interactive mode.
fn prompt_mappings(
    input: &dyn Input,
    output: &dyn Output,
    switch_info: &SwitchInfo,
    context: &ProjectContext,
) -> Result<Vec<SwitchContainerEntry>> {
    let mut entries: Vec<SwitchContainerEntry> = Vec::new();

    output.progress(&format!(
        "\n{} '{}' has the following states:",
        "Switch".cyan(),
        switch_info.name
    ));

    for state in &switch_info.states {
        output.progress(&format!("  • {} (ID: {})", state.name, state.id));
    }

    output.progress("\nMap each state to a sound or collection:");

    // Get available sounds and collections for selection
    let sounds = get_available_sounds(context)?;
    let collections = get_available_collections(context)?;

    let mut available_targets: Vec<(String, u64, &'static str)> = Vec::new();
    for (name, id) in &sounds {
        available_targets.push((name.clone(), *id, "sound"));
    }
    for (name, id) in &collections {
        available_targets.push((name.clone(), *id, "collection"));
    }

    available_targets.sort_by(|a, b| a.0.cmp(&b.0));

    if available_targets.is_empty() {
        output.progress(&format!(
            "{} No sounds or collections available. You can create the switch container without mappings and update it later.",
            "Warning:".yellow()
        ));
        return Ok(entries);
    }

    for state in &switch_info.states {
        let target_names: Vec<String> = available_targets
            .iter()
            .map(|(name, _, kind)| format!("{} ({})", name, kind))
            .collect();

        let prompt = format!(
            "Select sound/collection for state '{}' (or skip):",
            state.name
        );

        // Add "Skip" option
        let mut options_with_skip = vec!["[Skip this state]".to_string()];
        options_with_skip.extend(target_names);

        match crate::input::select_index(input, &prompt, &options_with_skip) {
            Ok(idx) => {
                if idx == 0 {
                    // User chose to skip
                    continue;
                }

                let (target_name, target_id, _) = &available_targets[idx - 1];

                // Check if we already have an entry for this target
                if let Some(existing) = entries.iter_mut().find(|e| e.object == *target_id) {
                    if !existing.switch_states.contains(&state.id) {
                        existing.switch_states.push(state.id);
                        output.progress(&format!(
                            "  {} Mapped state '{}' to '{}'",
                            "✓".green(),
                            state.name,
                            target_name
                        ));
                    }
                } else {
                    entries.push(SwitchContainerEntry {
                        object: *target_id,
                        switch_states: vec![state.id],
                        continue_between_states: false,
                        fade_in: None,
                        fade_out: None,
                        gain: None,
                        pitch: None,
                    });
                    output.progress(&format!(
                        "  {} Mapped state '{}' to '{}'",
                        "✓".green(),
                        state.name,
                        target_name
                    ));
                }
            }
            Err(_) => {
                // Non-interactive mode or error - skip this state
                continue;
            }
        }
    }

    Ok(entries)
}

/// Find an asset (sound or collection) ID by name.
fn find_asset_id_by_name(context: &ProjectContext, name: &str) -> Result<Option<u64>> {
    // Try sounds first
    let sounds_dir = context.project_root.join("sources").join("sounds");
    let sound_file = sounds_dir.join(format!("{}.json", name));

    if sound_file.exists() {
        let content = fs::read_to_string(&sound_file)?;
        let sound: crate::assets::Sound = serde_json::from_str(&content)?;
        return Ok(Some(sound.id));
    }

    // Try collections
    let collections_dir = context.project_root.join("sources").join("collections");
    let collection_file = collections_dir.join(format!("{}.json", name));

    if collection_file.exists() {
        let content = fs::read_to_string(&collection_file)?;
        let collection: crate::assets::Collection = serde_json::from_str(&content)?;
        return Ok(Some(collection.id));
    }

    Ok(None)
}

/// Get all available sounds in the project.
fn get_available_sounds(context: &ProjectContext) -> Result<Vec<(String, u64)>> {
    let sounds_dir = context.project_root.join("sources").join("sounds");

    if !sounds_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sounds = Vec::new();

    for entry in fs::read_dir(&sounds_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(sound) = serde_json::from_str::<crate::assets::Sound>(&content) {
                    sounds.push((sound.name().to_string(), sound.id));
                }
            }
        }
    }

    sounds.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(sounds)
}

/// Get all available collections in the project.
fn get_available_collections(context: &ProjectContext) -> Result<Vec<(String, u64)>> {
    let collections_dir = context.project_root.join("sources").join("collections");

    if !collections_dir.exists() {
        return Ok(Vec::new());
    }

    let mut collections = Vec::new();

    for entry in fs::read_dir(&collections_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(collection) = serde_json::from_str::<crate::assets::Collection>(&content)
                {
                    collections.push((collection.name().to_string(), collection.id));
                }
            }
        }
    }

    collections.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(collections)
}

// =============================================================================
// List
// =============================================================================

/// List all switch container assets in the current project.
async fn list_switch_containers(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Scan switch_containers directory
    let containers_dir = current_dir.join("sources").join("switch_containers");

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
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Step 4: Read and parse all .json files
    let mut containers: Vec<SwitchContainer> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let entries = match fs::read_dir(&containers_dir) {
        Ok(entries) => entries,
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

    let canonical_dir = containers_dir
        .canonicalize()
        .unwrap_or_else(|_| containers_dir.clone());

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Skip symlinks that resolve outside the directory
        if path.is_symlink() {
            match path.canonicalize() {
                Ok(resolved) => {
                    if !resolved.starts_with(&canonical_dir) {
                        log::warn!(
                            "Skipping symlink outside directory: {}",
                            path.display()
                        );
                        continue;
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Skipping broken symlink: {} (error: {})",
                        path.display(),
                        e
                    );
                    continue;
                }
            }
        }

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<SwitchContainer>(&content) {
                    Ok(container) => {
                        containers.push(container);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        log::warn!("Skipping invalid switch container file: {}", path.display());
                        warnings.push(format!("Invalid JSON in {}: {}", filename, e));
                    }
                },
                Err(e) => {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    log::warn!("Failed to read switch container file: {}", path.display());
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

    // Try to find the switch by ID using the validator
    if let Some(validator) = &context.validator {
        if let Some(names) = validator.asset_names.get(&AssetType::Switch) {
            for name in names {
                if let Ok(Some(info)) = find_switch_by_name(context, name) {
                    if info.id == switch_id {
                        return name.clone();
                    }
                }
            }
        }
    }

    format!("ID:{}", switch_id)
}

// =============================================================================
// Update
// =============================================================================

/// Update an existing switch container asset.
async fn update_switch_container(
    name: &str,
    mappings: Option<Vec<String>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating switch container '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Locate existing container file
    let containers_dir = current_dir.join("sources").join("switch_containers");
    let container_file_path = containers_dir.join(format!("{}.json", name));

    if !container_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset switchcontainer list' to see available containers, or 'am asset switchcontainer create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing container
    let content = fs::read_to_string(&container_file_path).context(format!(
        "Failed to read switch container file: {}",
        container_file_path.display()
    ))?;
    let mut container: SwitchContainer = serde_json::from_str(&content).context(format!(
        "Failed to parse switch container file: {}",
        container_file_path.display()
    ))?;

    // Step 4: Build context and get switch info
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    let switch_info = find_switch_by_name(&context, &get_switch_name(&context, container.switch_group))
        .ok()
        .flatten()
        .ok_or_else(|| {
            CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Controlling switch not found",
                "The switch referenced by this container no longer exists",
            )
            .with_suggestion("Delete and recreate the switch container with a valid switch")
        })?;

    // Step 5: Apply updates
    let has_any_flag = mappings.is_some();
    let updated_fields: Vec<String>;

    if has_any_flag {
        // Non-interactive mode
        updated_fields = apply_mapping_updates(&mut container, mappings, &switch_info, &context)?;
    } else {
        // Interactive mode
        updated_fields = prompt_container_updates(&mut container, input, output, &switch_info, &context)?;
    }

    // Validate that we're not trying to change the switch group
    // (This is enforced by not having a --switch flag on update)

    // Step 6: Validate
    container.validate_rules(&context)?;

    // Step 7: Serialize and write atomically
    let json_content = serde_json::to_string_pretty(&container)
        .context("Failed to serialize switch container to JSON")?;
    atomic_write(&container_file_path, json_content.as_bytes())?;

    // Step 8: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": container.id,
                    "name": container.name(),
                    "path": container_file_path.to_string_lossy(),
                    "updated_fields": updated_fields,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Switch container '{}' updated successfully at {}",
                    name,
                    container_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Apply mapping updates to a container (non-interactive mode).
fn apply_mapping_updates(
    container: &mut SwitchContainer,
    mappings: Option<Vec<String>>,
    switch_info: &SwitchInfo,
    context: &ProjectContext,
) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    if let Some(mapping_list) = mappings {
        // Parse new mappings
        let new_entries = parse_mappings(&mapping_list, switch_info, context)?;

        // Merge with existing entries
        let mut entries = container.entries.take().unwrap_or_default();

        for new_entry in new_entries {
            if let Some(existing) = entries.iter_mut().find(|e| e.object == new_entry.object) {
                // Merge states
                for state_id in &new_entry.switch_states {
                    if !existing.switch_states.contains(state_id) {
                        existing.switch_states.push(*state_id);
                    }
                }
            } else {
                entries.push(new_entry);
            }
        }

        container.entries = Some(entries);
        updated_fields.push("entries".to_string());
    }

    Ok(updated_fields)
}

/// Prompt for container updates in interactive mode.
fn prompt_container_updates(
    container: &mut SwitchContainer,
    input: &dyn Input,
    output: &dyn Output,
    switch_info: &SwitchInfo,
    context: &ProjectContext,
) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    // Get current entries for display
    let current_entries = container.entries.as_ref().map(|e| e.len()).unwrap_or(0);

    // Prompt to modify mappings
    match input.confirm(
        &format!("Modify state mappings? (current: {} entries)", current_entries),
        Some(false),
    ) {
        Ok(true) => {
            let new_entries = prompt_mappings_for_update(input, output, switch_info, context, container)?;
            container.entries = Some(new_entries);
            updated_fields.push("entries".to_string());
        }
        _ => {}
    }

    Ok(updated_fields)
}

/// Prompt for mapping updates in interactive mode.
fn prompt_mappings_for_update(
    input: &dyn Input,
    output: &dyn Output,
    switch_info: &SwitchInfo,
    context: &ProjectContext,
    container: &SwitchContainer,
) -> Result<Vec<SwitchContainerEntry>> {
    use colored::Colorize;

    let mut entries = container.entries.clone().unwrap_or_default();

    // Build current mapping display
    let current_mappings = build_mapping_display(&entries, switch_info, context);

    output.progress(&format!("\nCurrent mappings:\n{}", current_mappings));

    output.progress(&format!(
        "\n{} '{}' has the following states:",
        "Switch".cyan(),
        switch_info.name
    ));

    for state in &switch_info.states {
        output.progress(&format!("  • {} (ID: {})", state.name, state.id));
    }

    // Get available targets
    let sounds = get_available_sounds(context)?;
    let collections = get_available_collections(context)?;

    let mut available_targets: Vec<(String, u64, &'static str)> = Vec::new();
    for (name, id) in &sounds {
        available_targets.push((name.clone(), *id, "sound"));
    }
    for (name, id) in &collections {
        available_targets.push((name.clone(), *id, "collection"));
    }
    available_targets.sort_by(|a, b| a.0.cmp(&b.0));

    loop {
        let prompt = "Options: (a)dd mapping, (r)emove mapping, (d)one";

        let choice = input.prompt_text(
            prompt,
            Some("d"),
            None,
            Some(&|value: &str| {
                let trimmed = value.trim().to_lowercase();
                if trimmed.is_empty() || ["a", "add", "r", "remove", "d", "done"].contains(&trimmed.as_str()) {
                    Ok(Validation::Valid)
                } else {
                    Ok(Validation::Invalid("Enter 'a' to add, 'r' to remove, or 'd' to done".into()))
                }
            }),
        )?;

        match choice.trim().to_lowercase().as_str() {
            "a" | "add" => {
                // Select state
                let state_names: Vec<String> = switch_info.states.iter().map(|s| s.name.clone()).collect();
                let state_idx = crate::input::select_index(input, "Select state to map:", &state_names)?;
                let state = &switch_info.states[state_idx];

                // Select target
                let target_names: Vec<String> = available_targets
                    .iter()
                    .map(|(name, _, kind)| format!("{} ({})", name, kind))
                    .collect();
                let target_idx = crate::input::select_index(input, "Select sound/collection:", &target_names)?;
                let (target_name, target_id, _) = &available_targets[target_idx];

                // Add or update entry
                if let Some(existing) = entries.iter_mut().find(|e| e.object == *target_id) {
                    if !existing.switch_states.contains(&state.id) {
                        existing.switch_states.push(state.id);
                        output.progress(&format!("  {} Added mapping: {} -> {}", "✓".green(), state.name, target_name));
                    } else {
                        output.progress(&format!("  {} State '{}' is already mapped to '{}'", "ℹ".blue(), state.name, target_name));
                    }
                } else {
                    entries.push(SwitchContainerEntry {
                        object: *target_id,
                        switch_states: vec![state.id],
                        continue_between_states: false,
                        fade_in: None,
                        fade_out: None,
                        gain: None,
                        pitch: None,
                    });
                    output.progress(&format!("  {} Added mapping: {} -> {}", "✓".green(), state.name, target_name));
                }
            }
            "r" | "remove" => {
                if entries.is_empty() {
                    output.progress(&format!("  {} No mappings to remove", "ℹ".blue()));
                    continue;
                }

                // Build list of current mappings for selection
                let mapping_strings: Vec<String> = entries
                    .iter()
                    .flat_map(|e| {
                        let target_name = find_name_by_id(context, e.object).unwrap_or_else(|| format!("ID:{}", e.object));
                        e.switch_states.iter().map(move |&state_id| {
                            let state_name = switch_info.states.iter()
                                .find(|s| s.id == state_id)
                                .map(|s| s.name.as_str())
                                .unwrap_or("?");
                            format!("{} -> {}", state_name, target_name)
                        })
                    })
                    .collect();

                if mapping_strings.is_empty() {
                    output.progress(&format!("  {} No individual mappings to remove", "ℹ".blue()));
                    continue;
                }

                let mapping_idx = crate::input::select_index(input, "Select mapping to remove:", &mapping_strings)?;

                // Find and remove the mapping
                let mut current_idx = 0;
                let mut removed = false;
                for entry in entries.iter_mut() {
                    for (state_idx, &state_id) in entry.switch_states.iter().enumerate() {
                        if current_idx == mapping_idx {
                            entry.switch_states.remove(state_idx);
                            removed = true;
                            break;
                        }
                        current_idx += 1;
                    }
                    if removed {
                        break;
                    }
                }

                // Remove entries with no states
                entries.retain(|e| !e.switch_states.is_empty());

                output.progress(&format!("  {} Removed mapping", "✓".green()));
            }
            _ => break,
        }
    }

    Ok(entries)
}

/// Build a display string for current mappings.
fn build_mapping_display(
    entries: &[SwitchContainerEntry],
    switch_info: &SwitchInfo,
    context: &ProjectContext,
) -> String {
    if entries.is_empty() {
        return "  (none)\n".to_string();
    }

    let mut result = String::new();
    for entry in entries {
        let target_name = find_name_by_id(context, entry.object)
            .unwrap_or_else(|| format!("ID:{}", entry.object));
        for state_id in &entry.switch_states {
            let state_name = switch_info
                .states
                .iter()
                .find(|s| s.id == *state_id)
                .map(|s| s.name.as_str())
                .unwrap_or("?");
            result.push_str(&format!("  {} -> {}\n", state_name, target_name));
        }
    }
    result
}

/// Find asset name by ID.
fn find_name_by_id(context: &ProjectContext, id: u64) -> Option<String> {
    // Search in sounds
    if let Some(validator) = &context.validator {
        if let Some(names) = validator.asset_names.get(&AssetType::Sound) {
            for name in names {
                if let Ok(Some(asset_id)) = find_asset_id_by_name(context, name) {
                    if asset_id == id {
                        return Some(name.clone());
                    }
                }
            }
        }

        // Search in collections
        if let Some(names) = validator.asset_names.get(&AssetType::Collection) {
            for name in names {
                if let Ok(Some(asset_id)) = find_asset_id_by_name(context, name) {
                    if asset_id == id {
                        return Some(name.clone());
                    }
                }
            }
        }
    }

    None
}

// =============================================================================
// Delete
// =============================================================================

/// Delete a switch container asset.
async fn delete_switch_container(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Locate container file
    let containers_dir = current_dir.join("sources").join("switch_containers");
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
