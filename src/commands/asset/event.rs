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

//! Event asset commands.
//!
//! Implements CRUD operations for Event assets in Amplitude projects. The
//! create/update handlers only gather values (flags, then prompts when the
//! session is interactive); building and merging lives in `services::event`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, Event, ProjectContext, ProjectValidator},
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        json_merge::merge_update_unknown_only,
        utils::read_amproject_file,
    },
    database::Database,
    forms::coverage::known_field_names,
    input::Input,
    presentation::{Output, OutputMode},
    services::event::{EventPatch, EventValues, apply_patch, build_event},
};

use super::event_args::{
    EventFlags, bus_warnings, patch_actions, resolve_patch, validate_patch_references,
};
use super::event_list::{delete_event, format_run_mode, list_events};
use super::event_prompts::{complete_create_patch, prompt_update_patch};
use super::sound_args::with_warnings;

/// The name of the current asset.
const ASSET_NAME: &str = "Event";

/// Syntax help shared by the flags that take actions.
const ACTION_HELP: &str = "TYPE:TARGET[,TARGET...][,active=BOOL][,scope=world|entity]";

/// Event asset subcommands.
#[derive(Subcommand, Debug)]
pub enum EventCommands {
    /// Create a new event asset
    #[command(
        after_help = "Action syntax: TYPE:TARGET[,TARGET...][,active=BOOL][,scope=world|entity]
  TYPE     none, play, pause, resume, stop, seek, mute-bus, unmute-bus, wait
  TARGET   a name or ID. Play/pause/resume/stop/seek act on sounds, collections or switch
           containers; mute-bus/unmute-bus on buses. Write kind:name (sound:hit,
           collection:hit, switch-container:music, bus:sfx) when a name exists in several kinds.
           wait takes its duration in milliseconds instead (wait:500).
  active   whether the action runs (default: true)
  scope    world or entity (default: entity)

Examples:
  am asset event create play_music --action play:music_loop
  am asset event create ui_click --action play:click,scope=world --action wait:200
  am asset event create duck_sfx --run-mode sequential --action mute-bus:sfx --action wait:1000 \\
      --action unmute-bus:sfx
  am asset event create stop_all --action stop:sound:hit,collection:hit
  am asset event create from_file --actions-from-json actions.json
"
    )]
    Create {
        /// Name of the event asset
        name: String,

        /// Run mode for actions: parallel or sequential (default: parallel)
        #[arg(short, long)]
        run_mode: Option<String>,

        /// An action: TYPE:TARGET[,TARGET...][,active=BOOL][,scope=world|entity] (repeatable)
        #[arg(short, long, value_name = "TYPE:TARGET[,KEY=VALUE...]")]
        action: Vec<String>,

        /// Read the actions from a JSON file instead of --action: a list of
        /// {"type": "Play", "targets": [NAME|ID...], "active": true, "scope": "World"} objects,
        /// or an SDK event object with an "actions" list
        #[arg(long, value_name = "FILE")]
        actions_from_json: Option<PathBuf>,
    },

    /// List all event assets in the project
    #[command(after_help = "Examples:\n  am asset event list\n")]
    List {},

    /// Update an existing event asset
    #[command(after_help = "Examples:
  am asset event update play_music --run-mode sequential
  am asset event update play_music --add-action stop:ambience --remove-action 0
  am asset event update play_music --action play:music_loop --action wait:500
  am asset event update play_music --clear-actions --add-action play:music_loop
")]
    Update {
        /// Name of the event asset to update
        name: String,

        /// New run mode for actions: parallel or sequential
        #[arg(short, long)]
        run_mode: Option<String>,

        /// Replace all actions with these: TYPE:TARGET[,TARGET...][,active=BOOL][,scope=world|entity]
        #[arg(long, value_name = "TYPE:TARGET[,KEY=VALUE...]")]
        action: Vec<String>,

        /// Replace all actions with the ones of a JSON file (same format as on create)
        #[arg(long, value_name = "FILE")]
        actions_from_json: Option<PathBuf>,

        /// Append an action (same syntax as --action); repeatable
        #[arg(short, long, value_name = "TYPE:TARGET[,KEY=VALUE...]")]
        add_action: Vec<String>,

        /// Remove the action at the given index (0-based); repeatable
        #[arg(long)]
        remove_action: Vec<usize>,

        /// Remove every action (the event needs new ones: combine with --add-action)
        #[arg(long)]
        clear_actions: bool,
    },

    /// Delete an event asset
    #[command(after_help = "Examples:\n  am asset event delete play_music --yes\n")]
    Delete {
        /// Name of the event asset to delete
        name: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,

        /// Accepted for compatibility; it no longer overrides the in-use check
        #[arg(short, long)]
        force: bool,
    },
}

/// Handle event commands by routing to the appropriate handler.
pub async fn handler(
    command: &EventCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        EventCommands::Create {
            name,
            run_mode,
            action,
            actions_from_json,
        } => {
            let flags = EventFlags {
                run_mode: run_mode.clone(),
                action: action.clone(),
                actions_from_json: actions_from_json.clone(),
                ..Default::default()
            };
            create_event(name, flags, input, output).await
        }
        EventCommands::List {} => list_events(output).await,
        EventCommands::Update {
            name,
            run_mode,
            action,
            actions_from_json,
            add_action,
            remove_action,
            clear_actions,
        } => {
            let flags = EventFlags {
                run_mode: run_mode.clone(),
                action: action.clone(),
                actions_from_json: actions_from_json.clone(),
                add_action: add_action.clone(),
                remove_action: remove_action.clone(),
                clear_actions: *clear_actions,
            };
            update_event(name, flags, input, output).await
        }
        EventCommands::Delete { name, yes, force } => {
            delete_event(name, *yes, *force, input, output).await
        }
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

fn missing_action_error() -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        "Missing required --action",
        "Events must have at least one action and cannot prompt in non-interactive or JSON mode",
    )
    .with_suggestion(format!(
        "Pass --action {ACTION_HELP} (for example --action play:my_sound) or --actions-from-json <file>"
    ))
}

// =============================================================================
// Create
// =============================================================================

/// Create a new event asset.
async fn create_event(
    name: &str,
    flags: EventFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Event name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset event create <name>'")
        .into());
    }

    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating event '{}' in project '{}'...",
        name, project_config.name
    ));

    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let events_dir = sources_base.join("events");
    let event_file_path = events_dir.join(format!("{}.json", name));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset event update {}' to modify it, or choose a different name",
            name
        ))
    };
    if event_file_path.exists() {
        return Err(already_exists().into());
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;
    if context.has_name(AssetType::Event, name) {
        return Err(already_exists().into());
    }

    // Resolve flags; unknown names and dangling ids fail before any prompt
    let patch = resolve_patch(&flags, validator)?;
    validate_patch_references(&patch, validator)?;

    // Actions are the one required field; everything else has a default
    let patch = if can_prompt(input, output) {
        complete_create_patch(input, validator, patch)?
    } else if patch.actions.replace.is_none() {
        return Err(missing_action_error().into());
    } else {
        patch
    };

    let event = build_event(EventValues::from_patch(name, &patch)?, &context)?;
    let json_content =
        serde_json::to_string_pretty(&event).context("Failed to serialize event to JSON")?;
    fs::create_dir_all(&events_dir)?;
    atomic_write(&event_file_path, json_content.as_bytes())?;

    let warnings = bus_warnings(validator, &patch_actions(&patch), output);
    let action_count = event.actions.as_ref().map_or(0, |a| a.len());
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": event.id,
                        "name": event.name(),
                        "path": event_file_path.to_string_lossy(),
                        "action_count": action_count,
                        "run_mode": format_run_mode(&event.run_mode),
                    }),
                    &warnings,
                ),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Event '{}' created successfully at {}\n  Actions: {}",
                    name,
                    event_file_path.display(),
                    action_count
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

/// Update an existing event asset.
async fn update_event(
    name: &str,
    flags: EventFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating event '{}' in project '{}'...",
        name, project_config.name
    ));

    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let event_file_path = sources_base.join("events").join(format!("{}.json", name));

    if !event_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset event list' to see available events, or 'am asset event create {}' to create it",
                name
            ))
            .into());
    }

    // Parse the existing event (typed, and raw to keep what the CLI does not model)
    let content = fs::read_to_string(&event_file_path).context(format!(
        "Failed to read event file: {}",
        event_file_path.display()
    ))?;
    let existing: Event = serde_json::from_str(&content).context(format!(
        "Failed to parse event file: {}",
        event_file_path.display()
    ))?;
    let original: serde_json::Value = serde_json::from_str(&content).context(format!(
        "Failed to parse event file: {}",
        event_file_path.display()
    ))?;

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;

    // Flags win; with none, an interactive session is offered every field
    let mut patch: EventPatch = resolve_patch(&flags, validator)?;
    validate_patch_references(&patch, validator)?;
    if patch.is_empty() && can_prompt(input, output) {
        patch = prompt_update_patch(input, validator, &existing)?;
    }

    // Merge into a copy and validate the new state
    let event = apply_patch(&existing, &patch)?;
    event.validate_rules(&context)?;

    // Serialize over the original file, keeping keys the CLI does not model
    let known = known_field_names(AssetType::Event);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let mut updated = serde_json::to_value(&event).context("Failed to serialize event to JSON")?;
    if patch.actions.is_empty()
        && let (Some(raw), Some(map)) = (original.get("actions"), updated.as_object_mut())
    {
        // Untouched actions stay exactly as written (including keys the CLI does not model).
        map.insert("actions".to_string(), raw.clone());
    }
    let merged = merge_update_unknown_only(&original, &updated, &known);
    let json_content =
        serde_json::to_string_pretty(&merged).context("Failed to serialize event to JSON")?;
    atomic_write(&event_file_path, json_content.as_bytes())?;

    let warnings = bus_warnings(validator, &patch_actions(&patch), output);
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": event.id,
                        "name": event.name(),
                        "path": event_file_path.to_string_lossy(),
                        "action_count": event.actions.as_ref().map_or(0, |a| a.len()),
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
                    "Event '{}' updated successfully at {}",
                    name,
                    event_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}
