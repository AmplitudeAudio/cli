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
//! Implements CRUD operations for SwitchContainer assets in Amplitude projects. The
//! create/update handlers only gather values (flags, then prompts when the
//! session is interactive); building and merging lives in `services::switch_container`.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, ProjectContext, ProjectValidator, SwitchContainer},
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        json_merge::merge_update_unknown_only,
        utils::read_amproject_file,
    },
    database::Database,
    forms::coverage::known_field_names,
    forms::references::RefTarget,
    input::Input,
    presentation::{Output, OutputMode},
    services::switch_container::{
        SwitchContainerPatch, SwitchContainerValues, apply_patch, build_switch_container,
    },
};

use super::sound_args::{bus_warnings, with_warnings};
use super::switch_container_args::{
    SwitchContainerEntryEditArgs, SwitchContainerExtraArgs, SwitchContainerFlags, bind_states,
    resolve_flags, validate_flag_references,
};
use super::switch_container_list::{delete_switch_container, list_switch_containers};
use super::switch_container_prompts::{
    complete_create_patch, prompt_bus, prompt_switch_group, prompt_update_patch,
};

/// The name of the current asset.
const ASSET_NAME: &str = "Switch Container";

/// Switch container asset subcommands.
#[derive(Subcommand, Debug)]
pub enum SwitchContainerCommands {
    /// Create a new switch container asset
    #[command(after_help = "Examples:
  am asset switch-container create footsteps --bus sfx --switch-group surface \\
      --entry wood_step,states=wood --entry stone_step,states=stone|gravel,fade-in=0.2:EaseIn
  am asset switch-container create footsteps --bus sfx --switch-group surface --map wood=wood_step
  am asset switch-container create music --bus music --switch-group mood --default-switch-state calm \\
      --update-behavior UpdateOnChange --entries-from-json music_entries.json
")]
    Create {
        /// Name of the switch container asset
        name: String,

        /// Switch that drives the container, by name or ID (required; see 'am asset switch list')
        #[arg(long, alias = "switch")]
        switch_group: Option<String>,

        /// Volume gain (0.0-1.0, default: 1.0)
        #[arg(short, long)]
        gain: Option<f32>,

        /// Bus to route the container to, by name or ID (required; see 'am asset bus list')
        #[arg(short, long)]
        bus: Option<String>,

        /// Playback priority (0-255, default: 128)
        #[arg(short, long)]
        priority: Option<u8>,

        /// Spatialization mode: none, position, position_orientation, hrtf (default: none)
        #[arg(short, long)]
        spatialization: Option<String>,

        #[command(flatten)]
        extra: SwitchContainerExtraArgs,
    },

    /// List all switch container assets in the project
    #[command(after_help = "Examples:
  am asset switch-container list
  am asset switch-container list --json
")]
    List {},

    /// Update an existing switch container asset
    #[command(after_help = "Examples:
  am asset switch-container update footsteps --bus sfx
  am asset switch-container update footsteps --map gravel=stone_step
  am asset switch-container update footsteps --add-entry grass_step,states=grass,fade-out=0.3 --remove-entry old_step
  am asset switch-container update footsteps --default-switch-state stone --update-behavior UpdateOnChange
")]
    Update {
        /// Name of the switch container asset to update
        name: String,

        /// New switch, by name or ID (its states replace the old ones: redo the default state and entries)
        #[arg(long, alias = "switch")]
        switch_group: Option<String>,

        /// New volume gain (0.0-1.0)
        #[arg(short, long)]
        gain: Option<f32>,

        /// New bus, by name or ID
        #[arg(short, long)]
        bus: Option<String>,

        /// New playback priority (0-255)
        #[arg(short, long)]
        priority: Option<u8>,

        /// New spatialization mode: none, position, position_orientation, hrtf
        #[arg(short, long)]
        spatialization: Option<String>,

        #[command(flatten)]
        extra: SwitchContainerExtraArgs,

        #[command(flatten)]
        edit: SwitchContainerEntryEditArgs,
    },

    /// Delete a switch container asset
    #[command(after_help = "Examples:
  am asset switch-container delete footsteps
  am asset switch-container delete footsteps --force
")]
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
            switch_group,
            gain,
            bus,
            priority,
            spatialization,
            extra,
        } => {
            let flags = SwitchContainerFlags {
                switch_group: switch_group.clone(),
                gain: *gain,
                bus: bus.clone(),
                priority: *priority,
                spatialization: spatialization.clone(),
                extra: extra.clone(),
                edit: SwitchContainerEntryEditArgs::default(),
            };
            create_switch_container(name, flags, input, output).await
        }
        SwitchContainerCommands::List {} => list_switch_containers(output).await,
        SwitchContainerCommands::Update {
            name,
            switch_group,
            gain,
            bus,
            priority,
            spatialization,
            extra,
            edit,
        } => {
            let flags = SwitchContainerFlags {
                switch_group: switch_group.clone(),
                gain: *gain,
                bus: bus.clone(),
                priority: *priority,
                spatialization: spatialization.clone(),
                extra: extra.clone(),
                edit: edit.clone(),
            };
            update_switch_container(name, flags, input, output).await
        }
        SwitchContainerCommands::Delete { name, force } => {
            delete_switch_container(name, *force, input, output).await
        }
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

fn missing_flag_error(flag: &str, what: &str, hint: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_EMPTY_REFERENCE,
        format!("Missing required {flag}"),
        format!(
            "Switch containers must have {what} and cannot prompt in non-interactive or JSON mode"
        ),
    )
    .with_suggestion(hint.to_string())
}

fn missing_bus_error() -> CliError {
    missing_flag_error(
        "--bus",
        "a bus",
        "Pass --bus <name|id> (see 'am asset bus list'); if the project has no bus yet, create a bus first",
    )
}

fn missing_switch_group_error() -> CliError {
    missing_flag_error(
        "--switch-group",
        "a switch",
        "Pass --switch-group <name|id> (see 'am asset switch list'); if the project has no switch yet, create one with 'am asset switch create'",
    )
}

// =============================================================================
// Create
// =============================================================================

/// Create a new switch container asset.
async fn create_switch_container(
    name: &str,
    flags: SwitchContainerFlags,
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
        .with_suggestion("Provide a name: 'am asset switch-container create <name>'")
        .into());
    }

    // Step 2: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating switch container '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 3: Validate the name is free (filesystem + registry)
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let containers_dir = sources_base.join("switch_containers");
    let container_file_path = containers_dir.join(format!("{}.json", name));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset switch-container update {}' to modify it, or choose a different name",
            name
        ))
    };

    if container_file_path.exists() {
        return Err(already_exists().into());
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;
    if context.has_name(AssetType::SwitchContainer, name) {
        return Err(already_exists().into());
    }

    // Step 4: Resolve flags; unknown names and dangling ids fail before any prompt
    let resolved = resolve_flags(&flags, validator)?;
    validate_flag_references(&resolved, validator)?;
    let interactive = can_prompt(input, output);

    // Step 5: The bus and the switch are the required fields
    let bus = match (resolved.patch.bus, interactive) {
        (Some(bus), _) => bus,
        (None, true) => prompt_bus(input, validator)?,
        (None, false) => return Err(missing_bus_error().into()),
    };
    let group = match (resolved.patch.switch_group, interactive) {
        (Some(group), _) => group,
        (None, true) => prompt_switch_group(input, validator)?,
        (None, false) => return Err(missing_switch_group_error().into()),
    };
    let patch = SwitchContainerPatch {
        bus: Some(bus),
        switch_group: Some(group),
        ..bind_states(resolved, group, validator)?
    };

    // Step 6: Optional fields, interactive only
    let patch = if interactive {
        complete_create_patch(input, validator, patch, group)?
    } else {
        patch
    };

    // Step 7: Build and validate through the service, then write atomically
    let values = SwitchContainerValues::from_patch(name, &patch, &context)?;
    let container = build_switch_container(values, &context)?;
    let json_content = serde_json::to_string_pretty(&container)
        .context("Failed to serialize switch container to JSON")?;
    fs::create_dir_all(&containers_dir)?;
    atomic_write(&container_file_path, json_content.as_bytes())?;

    // Step 8: Output success
    let warnings = bus_warnings(validator, Some(container.bus), output);
    let entry_count = container.entries.as_ref().map_or(0, |e| e.len());
    let switch_name = validator
        .resolve(RefTarget::Asset(AssetType::Switch), container.switch_group)
        .map_or_else(String::new, |s| s.name);
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": container.id,
                        "name": container.name(),
                        "path": container_file_path.to_string_lossy(),
                        "switch_group": switch_name,
                        "default_switch_state": container.default_switch_state,
                        "mapping_count": entry_count,
                    }),
                    &warnings,
                ),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Switch container '{}' created successfully at {}\nControlling switch: {} ({} entries)",
                    name,
                    container_file_path.display(),
                    switch_name,
                    entry_count
                )),
                None,
            );
        }
    }

    Ok(())
}

// =============================================================================
// Update
// =============================================================================

/// Update an existing switch container asset.
async fn update_switch_container(
    name: &str,
    flags: SwitchContainerFlags,
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
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let container_file_path = sources_base
        .join("switch_containers")
        .join(format!("{}.json", name));

    if !container_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset switch-container list' to see available containers, or 'am asset switch-container create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing container (typed, and raw to keep what the CLI does not model)
    let content = fs::read_to_string(&container_file_path).context(format!(
        "Failed to read switch container file: {}",
        container_file_path.display()
    ))?;
    let existing: SwitchContainer = serde_json::from_str(&content).context(format!(
        "Failed to parse switch container file: {}",
        container_file_path.display()
    ))?;
    let original: serde_json::Value = serde_json::from_str(&content).context(format!(
        "Failed to parse switch container file: {}",
        container_file_path.display()
    ))?;

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;

    // Step 4: Flags win; with none, an interactive session is offered every field.
    // State names are looked up in the switch the container will use.
    let resolved = resolve_flags(&flags, validator)?;
    validate_flag_references(&resolved, validator)?;
    let group = resolved.patch.switch_group.unwrap_or(existing.switch_group);
    let mut patch = bind_states(resolved, group, validator)?;
    if patch.is_empty() && can_prompt(input, output) {
        patch = prompt_update_patch(input, validator, &existing)?;
    }

    // Step 5: Merge into a copy and validate the new state
    let container = apply_patch(&existing, &patch)?;
    container.validate_rules(&context).map_err(|e| {
        let suggestion = match e.field.as_deref() {
            Some("bus") => Some(format!(
                "Run 'am asset switch-container update {name} --bus <name|id>' with an existing bus (see 'am asset bus list')"
            )),
            Some("default_switch_state") => Some(format!(
                "Run 'am asset switch-container update {name} --default-switch-state <state>' with a state of the container's switch. {}",
                validator.describe_states(container.switch_group)
            )),
            _ => None,
        };
        match suggestion {
            Some(suggestion) => e.with_suggestion(suggestion),
            None => e,
        }
    })?;

    // Step 6: Serialize over the original file, keeping keys the CLI does not model
    let known = known_field_names(AssetType::SwitchContainer);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let mut updated =
        serde_json::to_value(&container).context("Failed to serialize switch container to JSON")?;
    if patch.entries.is_empty()
        && let (Some(raw), Some(map)) = (original.get("entries"), updated.as_object_mut())
    {
        // Untouched entries stay exactly as written (including keys the CLI does not model).
        map.insert("entries".to_string(), raw.clone());
    }
    let merged = merge_update_unknown_only(&original, &updated, &known);
    let json_content = serde_json::to_string_pretty(&merged)
        .context("Failed to serialize switch container to JSON")?;
    atomic_write(&container_file_path, json_content.as_bytes())?;

    // Step 7: Output success
    let warnings = bus_warnings(validator, patch.bus, output);
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": container.id,
                        "name": container.name(),
                        "path": container_file_path.to_string_lossy(),
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
