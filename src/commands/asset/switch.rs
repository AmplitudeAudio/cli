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

//! Switch asset commands.
//!
//! Implements CRUD operations for Switch assets in Amplitude projects. The
//! create/update handlers only gather values (flags, then prompts when the
//! session is interactive); building and merging lives in `services::switch`.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, ProjectContext, ProjectValidator, Switch},
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        json_merge::merge_update_unknown_only,
        utils::read_amproject_file,
    },
    database::Database,
    forms::coverage::known_field_names,
    forms::references::switch_state_referrers,
    input::Input,
    presentation::{Output, OutputMode},
    services::switch::{SwitchPatch, SwitchValues, apply_patch, build_switch, removed_state_ids},
};

use super::switch_args::{SwitchEditArgs, SwitchExtraArgs, SwitchFlags, resolve_patch};
use super::switch_list::{delete_switch, list_switches};
use super::switch_prompts::{complete_create_patch, prompt_update_patch};

/// The name of the current asset.
const ASSET_NAME: &str = "Switch";

/// Switch asset subcommands.
#[derive(Subcommand, Debug)]
pub enum SwitchCommands {
    /// Create a new switch asset
    #[command(after_help = "Examples:
  am asset switch create surface_type --states wood,stone,metal
  am asset switch create surface_type --state wood,id=1 --state stone,id=2
  am asset switch create weather --states-from-json weather_states.json

State syntax: NAME[,id=N]
  NAME   the state name, unique in the switch
  id     the number switch containers store for the state (non-zero, unique in the switch);
         generated when omitted. Keep ids stable once containers use them.
")]
    Create {
        /// Name of the switch asset
        name: String,

        /// State names (comma-separated; ids are generated)
        #[arg(long, value_delimiter = ',')]
        states: Option<Vec<String>>,

        #[command(flatten)]
        extra: SwitchExtraArgs,
    },

    /// List all switch assets in the project
    #[command(after_help = "Examples:\n  am asset switch list\n")]
    List {},

    /// Update an existing switch asset
    #[command(after_help = "Examples:
  am asset switch update surface_type --add-state grass
  am asset switch update surface_type --rename-state stone=rock
  am asset switch update surface_type --remove-state metal
  am asset switch update surface_type --states wood,stone,grass

--states, --state and --states-from-json replace the list but keep the id of every state whose
name stays, so switch containers keep working. A state a switch container uses cannot be removed
(or given another id) until the container stops using it.
")]
    Update {
        /// Name of the switch asset to update
        name: String,

        /// New state names (comma-separated, replaces existing states; ids of kept names stay)
        #[arg(long, value_delimiter = ',')]
        states: Option<Vec<String>>,

        #[command(flatten)]
        extra: SwitchExtraArgs,

        #[command(flatten)]
        edit: SwitchEditArgs,
    },

    /// Delete a switch asset
    #[command(
        after_help = "Examples:\n  am asset switch delete surface_type\n  am asset switch delete surface_type --force\n"
    )]
    Delete {
        /// Name of the switch asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle switch commands by routing to the appropriate handler.
pub async fn handler(
    command: &SwitchCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        SwitchCommands::Create {
            name,
            states,
            extra,
        } => {
            let flags = SwitchFlags {
                states: states.clone(),
                extra: extra.clone(),
                ..Default::default()
            };
            create_switch(name, flags, input, output).await
        }
        SwitchCommands::List {} => list_switches(output).await,
        SwitchCommands::Update {
            name,
            states,
            extra,
            edit,
        } => {
            let flags = SwitchFlags {
                states: states.clone(),
                extra: extra.clone(),
                edit: edit.clone(),
            };
            update_switch(name, flags, input, output).await
        }
        SwitchCommands::Delete { name, force } => delete_switch(name, *force, input, output).await,
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

// =============================================================================
// Create
// =============================================================================

/// Create a new switch asset.
async fn create_switch(
    name: &str,
    flags: SwitchFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Switch name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset switch create <name>'")
        .into());
    }

    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating switch '{}' in project '{}'...",
        name, project_config.name
    ));

    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let switches_dir = sources_base.join("switches");
    let switch_file_path = switches_dir.join(format!("{}.json", name));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset switch update {}' to modify it, or choose a different name",
            name
        ))
    };
    if switch_file_path.exists() {
        return Err(already_exists().into());
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    if context.has_name(AssetType::Switch, name) {
        return Err(already_exists().into());
    }

    // Resolve flags; bad syntax fails before any prompt
    let patch = resolve_patch(&flags)?;

    // The states are the one required field; their ids are generated unless asked for
    let patch = if can_prompt(input, output) {
        complete_create_patch(input, name, patch)?
    } else {
        patch
    };

    let switch = build_switch(SwitchValues::from_patch(name, &patch)?, &context)?;
    let json_content =
        serde_json::to_string_pretty(&switch).context("Failed to serialize switch to JSON")?;
    fs::create_dir_all(&switches_dir)?;
    atomic_write(&switch_file_path, json_content.as_bytes())?;

    let state_count = switch.states.as_ref().map_or(0, |s| s.len());
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": switch.id,
                    "name": switch.name(),
                    "path": switch_file_path.to_string_lossy(),
                    "state_count": state_count,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Switch '{}' created successfully at {}",
                    name,
                    switch_file_path.display()
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

/// Refuses a change that drops a state some switch container still uses.
fn ensure_states_unused(
    validator: &ProjectValidator,
    before: &Switch,
    after: &Switch,
) -> Result<()> {
    let old = before.states.as_deref().unwrap_or(&[]);
    let new = after.states.as_deref().unwrap_or(&[]);
    for id in removed_state_ids(old, new) {
        let users = switch_state_referrers(validator, before.id, id);
        if users.is_empty() {
            continue;
        }
        let state = old
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.name.as_deref())
            .unwrap_or("(unnamed)");
        let names = users
            .iter()
            .map(|u| format!("'{}'", u.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(CliError::new(
            codes::ERR_ASSET_IN_USE,
            format!("Switch state '{state}' (ID {id}) is still used"),
            format!("Switch containers using it: {names}"),
        )
        .with_suggestion(format!(
            "Point those containers at another state first (am asset switch-container update <name> ...), then remove or re-number '{state}'; renaming keeps the ID"
        ))
        .into());
    }
    Ok(())
}

/// Update an existing switch asset.
async fn update_switch(
    name: &str,
    flags: SwitchFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating switch '{}' in project '{}'...",
        name, project_config.name
    ));

    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let switch_file_path = sources_base.join("switches").join(format!("{}.json", name));

    if !switch_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset switch list' to see available switches, or 'am asset switch create {}' to create it",
                name
            ))
            .into());
    }

    // Parse the existing switch (typed, and raw to keep what the CLI does not model)
    let content = fs::read_to_string(&switch_file_path).context(format!(
        "Failed to read switch file: {}",
        switch_file_path.display()
    ))?;
    let existing: Switch = serde_json::from_str(&content).context(format!(
        "Failed to parse switch file: {}",
        switch_file_path.display()
    ))?;
    let original: serde_json::Value = serde_json::from_str(&content).context(format!(
        "Failed to parse switch file: {}",
        switch_file_path.display()
    ))?;

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;

    // Flags win; with none, an interactive session is offered every field
    let mut patch: SwitchPatch = resolve_patch(&flags)?;
    if patch.is_empty() && can_prompt(input, output) {
        patch = prompt_update_patch(input, &existing)?;
    }

    // Merge into a copy and validate the new state
    let switch = apply_patch(&existing, &patch)?;
    switch.validate_rules(&context)?;
    ensure_states_unused(validator, &existing, &switch)?;

    // Serialize over the original file, keeping keys the CLI does not model
    let known = known_field_names(AssetType::Switch);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let mut updated =
        serde_json::to_value(&switch).context("Failed to serialize switch to JSON")?;
    if patch.states.is_empty()
        && let (Some(raw), Some(map)) = (original.get("states"), updated.as_object_mut())
    {
        // Untouched states stay exactly as written (including keys the CLI does not model).
        map.insert("states".to_string(), raw.clone());
    }
    let merged = merge_update_unknown_only(&original, &updated, &known);
    let json_content =
        serde_json::to_string_pretty(&merged).context("Failed to serialize switch to JSON")?;
    atomic_write(&switch_file_path, json_content.as_bytes())?;

    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": switch.id,
                    "name": switch.name(),
                    "path": switch_file_path.to_string_lossy(),
                    "state_count": switch.states.as_ref().map_or(0, |s| s.len()),
                    "updated_fields": patch.changed_fields(),
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Switch '{}' updated successfully at {}",
                    name,
                    switch_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}
