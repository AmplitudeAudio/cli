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
//! Implements CRUD operations for Event assets in Amplitude projects.
//! Events are triggerable audio actions that can be called from game code.

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
        Asset, AssetType, Event, ProjectContext, ProjectValidator, Scope,
        generated::{EventActionDefinition, EventActionRunningMode, EventActionType},
    },
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        utils::{read_amproject_file, truncate_string},
    },
    database::Database,
    input::{Input, select_index},
    presentation::{Output, OutputMode},
};

use super::find_json_files_recursive;

/// The name of the current asset.
const ASSET_NAME: &str = "Event";

/// Event asset subcommands.
#[derive(Subcommand, Debug)]
pub enum EventCommands {
    /// Create a new event asset
    #[command(
        after_help = "Examples:\n  am asset event create play_music\n  am asset event create play_music --action play:12345\n"
    )]
    Create {
        /// Name of the event asset
        name: String,

        /// Run mode for actions (Parallel, Sequential)
        #[arg(short, long)]
        run_mode: Option<String>,

        /// Action in format "type:target_id[,target_id,...]" (repeatable)
        #[arg(short, long)]
        action: Vec<String>,

        /// Fade time in milliseconds for applicable actions
        #[arg(long)]
        fade: Option<u64>,
    },

    /// List all event assets in the project
    #[command(after_help = "Examples:\n  am asset event list\n")]
    List {},

    /// Update an existing event asset
    #[command(
        after_help = "Examples:\n  am asset event update play_music\n  am asset event update play_music --run-mode sequential\n"
    )]
    Update {
        /// Name of the event asset to update
        name: String,

        /// New run mode for actions
        #[arg(short, long)]
        run_mode: Option<String>,

        /// Add a new action in format "type:target_id[,target_id,...]"
        #[arg(short, long)]
        add_action: Vec<String>,

        /// Remove action at the given index (0-based)
        #[arg(long)]
        remove_action: Vec<usize>,

        /// Clear all existing actions (requires adding new ones)
        #[arg(long)]
        clear_actions: bool,
    },

    /// Delete an event asset
    #[command(
        after_help = "Examples:\n  am asset event delete play_music --yes\n  am asset event delete play_music --yes --force\n"
    )]
    Delete {
        /// Name of the event asset to delete
        name: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,

        /// Force deletion even if referenced by soundbanks
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
            fade,
        } => create_event(name, run_mode.clone(), action.clone(), *fade, input, output).await,
        EventCommands::List {} => list_events(output).await,
        EventCommands::Update {
            name,
            run_mode,
            add_action,
            remove_action,
            clear_actions,
        } => {
            update_event(
                name,
                run_mode.clone(),
                add_action.clone(),
                remove_action.clone(),
                *clear_actions,
                input,
                output,
            )
            .await
        }
        EventCommands::Delete { name, yes, force } => {
            delete_event(name, *yes, *force, input, output).await
        }
    }
}

/// Parse run mode from string.
fn parse_run_mode(s: &str) -> Result<EventActionRunningMode> {
    match s.to_lowercase().as_str() {
        "parallel" => Ok(EventActionRunningMode::Parallel),
        "sequential" => Ok(EventActionRunningMode::Sequential),
        _ => Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid run mode: '{}'", s),
            "Run mode must be one of: parallel, sequential",
        )
        .into()),
    }
}

/// Parse action type from string.
fn parse_action_type(s: &str) -> Result<EventActionType> {
    match s.to_lowercase().as_str() {
        "play" => Ok(EventActionType::Play),
        "stop" => Ok(EventActionType::Stop),
        "pause" => Ok(EventActionType::Pause),
        "resume" => Ok(EventActionType::Resume),
        "seek" => Ok(EventActionType::Seek),
        _ => Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid action type: '{}'", s),
            "Action type must be one of: play, stop, pause, resume, seek",
        )
        .into()),
    }
}

/// Parse action specification string in format "type:target_id[,target_id,...]"
fn parse_action_spec(spec: &str) -> Result<(EventActionType, Vec<u64>)> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 2 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid action format: '{}'", spec),
            "Action must be in format 'type:target_id' or 'type:target1,target2'",
        )
        .into());
    }

    let action_type = parse_action_type(parts[0])?;
    let target_ids: Result<Vec<u64>> = parts[1]
        .split(',')
        .map(|s| {
            s.trim().parse::<u64>().map_err(|_| {
                CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    format!("Invalid target ID: '{}'", s),
                    "Target IDs must be positive integers",
                )
                .into()
            })
        })
        .collect();

    Ok((action_type, target_ids?))
}

/// Format action type for display.
fn format_action_type(action_type: &EventActionType) -> &'static str {
    match action_type {
        EventActionType::Play => "Play",
        EventActionType::Stop => "Stop",
        EventActionType::Pause => "Pause",
        EventActionType::Resume => "Resume",
        EventActionType::Seek => "Seek",
        EventActionType::MuteBus => "MuteBus",
        EventActionType::UnmuteBus => "UnmuteBus",
        EventActionType::Wait => "Wait",
        EventActionType::None => "None",
    }
}

/// Format run mode for display.
fn format_run_mode(mode: &EventActionRunningMode) -> &'static str {
    match mode {
        EventActionRunningMode::Parallel => "Parallel",
        EventActionRunningMode::Sequential => "Sequential",
    }
}

/// Create a new event asset.
async fn create_event(
    name: &str,
    run_mode: Option<String>,
    actions: Vec<String>,
    _fade: Option<u64>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating event '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Validate event name doesn't already exist
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let events_dir = sources_base.join("events");
    let event_file_path = events_dir.join(format!("{}.json", name));

    if event_file_path.exists() {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset event update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Build populated ProjectContext for validation
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Check name uniqueness
    if context.has_name(AssetType::Event, name) {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset event update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Step 3: Get run mode (prompt if not provided)
    let run_mode_value = if let Some(rm) = run_mode {
        parse_run_mode(&rm)?
    } else {
        prompt_run_mode(input)?
    };

    // Step 4: Get actions (from flags or prompt)
    let actions_list = if actions.is_empty() {
        prompt_actions_interactive(input, &context)?
    } else {
        parse_actions_from_flags(&actions, &context)?
    };

    // Validate at least one action
    if actions_list.is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Event has no actions",
            "Events must have at least one action defined",
        )
        .with_suggestion("Add at least one action using --action flag or in interactive mode")
        .into());
    }

    // Step 5: Generate unique ID
    let mut id = generate_unique_id(name);
    let mut retries = 0;
    while context.has_id(id) && retries < 3 {
        id = generate_unique_id(&format!("{}{}", name, retries));
        retries += 1;
    }
    if context.has_id(id) {
        return Err(CliError::new(
            codes::ERR_ASSET_ALREADY_EXISTS,
            format!("Generated ID {} collides with an existing asset", id),
            "All generated ID attempts collided with existing assets in the project",
        )
        .with_suggestion("Try a different name or wait a moment and retry")
        .into());
    }

    // Step 6: Build the Event asset
    let mut event = Event::builder(id, name).run_mode(run_mode_value).build();

    // Add actions to the event
    event.actions = Some(actions_list);

    // Step 7: Validate the event
    event.validate_rules(&context)?;

    // Step 8: Serialize to JSON
    let json_content =
        serde_json::to_string_pretty(&event).context("Failed to serialize event to JSON")?;

    // Step 9: Write using atomic write pattern
    fs::create_dir_all(&events_dir)?;
    atomic_write(&event_file_path, json_content.as_bytes())?;

    // Step 10: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": event.id,
                    "name": event.name(),
                    "path": event_file_path.to_string_lossy(),
                    "action_count": event.actions.as_ref().map(|a| a.len()).unwrap_or(0),
                    "run_mode": format_run_mode(&event.run_mode),
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Event '{}' created successfully at {}\n  Actions: {}",
                    name,
                    event_file_path.display(),
                    event.actions.as_ref().map(|a| a.len()).unwrap_or(0)
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Parse actions from command-line flags.
fn parse_actions_from_flags(
    action_specs: &[String],
    context: &ProjectContext,
) -> Result<Vec<EventActionDefinition>> {
    let mut actions = Vec::new();

    for spec in action_specs {
        let (action_type, target_ids) = parse_action_spec(spec)?;

        // Validate targets exist for actions that need them
        match action_type {
            EventActionType::Play
            | EventActionType::Stop
            | EventActionType::Pause
            | EventActionType::Resume
            | EventActionType::Seek => {
                if target_ids.is_empty() {
                    return Err(CliError::new(
                        codes::ERR_VALIDATION_FIELD,
                        format!("{} action has no targets", format_action_type(&action_type)),
                        format!(
                            "{:?} actions must have at least one target asset",
                            action_type
                        ),
                    )
                    .into());
                }

                // Validate playable assets for Play/Resume
                if let Some(ref validator) = context.validator {
                    for target_id in &target_ids {
                        if action_type == EventActionType::Play
                            || action_type == EventActionType::Resume
                        {
                            if !validator.is_playable_asset(*target_id) {
                                return Err(CliError::new(
                                    codes::ERR_VALIDATION_REFERENCE,
                                    format!(
                                        "Target {} is not a playable asset",
                                        target_id
                                    ),
                                    "Play and Resume actions must target sounds, collections, or switch containers",
                                )
                                .with_suggestion("Use the ID of a valid playable asset (sound, collection, or switch container)")
                                .into());
                            }
                        } else {
                            // For other action types, just verify the asset exists
                            let exists = validator
                                .asset_ids
                                .values()
                                .any(|ids| ids.contains(target_id));
                            if !exists && *target_id != 0 {
                                return Err(CliError::new(
                                    codes::ERR_VALIDATION_REFERENCE,
                                    format!("Target asset with ID {} not found", target_id),
                                    "The referenced asset does not exist in the project",
                                )
                                .with_suggestion("Check the asset ID or create the asset first")
                                .into());
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        let action = EventActionDefinition {
            type_: action_type,
            active: true,
            scope: Scope::Entity,
            targets: Some(target_ids),
        };
        actions.push(action);
    }

    Ok(actions)
}

/// Prompt for run mode in interactive mode.
fn prompt_run_mode(input: &dyn Input) -> Result<EventActionRunningMode> {
    let options = vec![
        "Parallel (all actions run simultaneously)".to_string(),
        "Sequential (actions run one after another)".to_string(),
    ];

    let modes = [
        EventActionRunningMode::Parallel,
        EventActionRunningMode::Sequential,
    ];

    match select_index(input, "Action run mode:", &options) {
        Ok(idx) => Ok(modes[idx]),
        Err(_) => Ok(EventActionRunningMode::Parallel), // Default in non-interactive mode
    }
}

/// Prompt for actions in interactive mode.
fn prompt_actions_interactive(
    input: &dyn Input,
    context: &ProjectContext,
) -> Result<Vec<EventActionDefinition>> {
    let mut actions: Vec<EventActionDefinition> = Vec::new();
    let action_types = vec!["Play", "Stop", "Pause", "Resume", "Seek"];

    loop {
        // Show current actions
        if !actions.is_empty() {
            output_simple("\nCurrent actions:");
            for (idx, action) in actions.iter().enumerate() {
                let targets = action
                    .targets
                    .as_ref()
                    .map(|t| {
                        t.iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "none".to_string());
                output_simple(&format!(
                    "  {}. {} -> {}",
                    idx + 1,
                    format_action_type(&action.type_),
                    targets
                ));
            }
            output_simple("");
        }

        let should_add = match input.confirm("Add an action?", Some(true)) {
            Ok(val) => val,
            Err(_) => break, // Non-interactive mode - stop adding actions
        };

        if !should_add {
            break;
        }

        // Select action type
        let type_idx = match select_index(
            input,
            "Action type:",
            &action_types
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        ) {
            Ok(idx) => idx,
            Err(_) => break,
        };
        let action_type = match action_types[type_idx] {
            "Play" => EventActionType::Play,
            "Stop" => EventActionType::Stop,
            "Pause" => EventActionType::Pause,
            "Resume" => EventActionType::Resume,
            "Seek" => EventActionType::Seek,
            _ => EventActionType::Play,
        };

        // Get target IDs
        let target_input = match input.prompt_text(
            "Target asset IDs (comma-separated)",
            None,
            None,
            Some(&|value: &str| {
                if value.trim().is_empty() {
                    return Ok(Validation::Invalid(
                        "At least one target ID is required".into(),
                    ));
                }
                for part in value.split(',') {
                    if part.trim().parse::<u64>().is_err() {
                        return Ok(Validation::Invalid(
                            format!("'{}' is not a valid ID", part).into(),
                        ));
                    }
                }
                Ok(Validation::Valid)
            }),
        ) {
            Ok(val) => val,
            Err(_) => break,
        };

        let target_ids: Vec<u64> = target_input
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect();

        // Validate targets
        if let Some(ref validator) = context.validator {
            if action_type == EventActionType::Play || action_type == EventActionType::Resume {
                for target_id in &target_ids {
                    if !validator.is_playable_asset(*target_id) {
                        output_simple(&format!(
                            "{} Warning: Target {} is not a playable asset (sound, collection, or switch container){}",
                            "⚠".yellow(),
                            target_id,
                            "⚠".yellow()
                        ));
                    }
                }
            }
        }

        let action = EventActionDefinition {
            type_: action_type,
            active: true,
            scope: Scope::Entity,
            targets: Some(target_ids),
        };
        actions.push(action);
    }

    Ok(actions)
}

/// Simple output helper for interactive prompts.
fn output_simple(message: &str) {
    println!("{}", message);
}

/// Maximum character length for paths before truncation in table display.
const PATH_MAX_LENGTH: usize = 40;

/// List all event assets in the current project.
async fn list_events(output: &dyn Output) -> Result<()> {
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
                                format!("Invalid JSON in {}: {}. First line: {}", filename, e, &line[..line.len().min(200)])
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

/// Update an existing event asset.
async fn update_event(
    name: &str,
    run_mode: Option<String>,
    add_actions: Vec<String>,
    remove_actions: Vec<usize>,
    clear_actions: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating event '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Locate existing event file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let events_dir = sources_base.join("events");
    let event_file_path = events_dir.join(format!("{}.json", name));

    if !event_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset event list' to see available events, or 'am asset event create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing event
    let content = fs::read_to_string(&event_file_path).context(format!(
        "Failed to read event file: {}",
        event_file_path.display()
    ))?;
    let mut event: Event = serde_json::from_str(&content).context(format!(
        "Failed to parse event file: {}",
        event_file_path.display()
    ))?;

    // Step 4: Build context for validation
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Step 5: Track updated fields
    let mut updated_fields: Vec<String> = Vec::new();

    // Get mutable actions list
    let mut actions = event.actions.take().unwrap_or_default();

    // Interactive mode: prompt for additional modifications if no flags provided
    let has_any_flag = run_mode.is_some()
        || !add_actions.is_empty()
        || !remove_actions.is_empty()
        || clear_actions;

    // Apply run mode update if provided
    if let Some(rm) = run_mode {
        let new_mode = parse_run_mode(&rm)?;
        if event.run_mode != new_mode {
            event.run_mode = new_mode;
            updated_fields.push("run_mode".to_string());
        }
    }

    if !has_any_flag {
        // Interactive mode - prompt for changes
        if let Some(new_mode) = prompt_update_run_mode(input, &event.run_mode)? {
            event.run_mode = new_mode;
            updated_fields.push("run_mode".to_string());
        }

        // Prompt to modify actions
        let modified_actions = prompt_modify_actions(input, &actions, &context)?;
        if modified_actions.len() != actions.len()
            || modified_actions
                .iter()
                .zip(actions.iter())
                .any(|(a, b)| a.type_ != b.type_ || a.targets != b.targets)
        {
            updated_fields.push("actions".to_string());
        }
        actions = modified_actions;
    }

    // Validate at least one action remains
    if actions.is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Event has no actions",
            "Events must have at least one action defined",
        )
        .with_suggestion("Add at least one action before saving")
        .into());
    }

    // Update event actions
    event.actions = Some(actions);

    // Step 6: Validate the updated event
    event.validate_rules(&context)?;

    // Step 7: Serialize and write atomically
    let json_content =
        serde_json::to_string_pretty(&event).context("Failed to serialize event to JSON")?;
    atomic_write(&event_file_path, json_content.as_bytes())?;

    // Step 8: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": event.id,
                    "name": event.name(),
                    "path": event_file_path.to_string_lossy(),
                    "action_count": event.actions.as_ref().map(|a| a.len()).unwrap_or(0),
                    "updated_fields": updated_fields,
                }),
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

/// Prompt to update run mode in interactive mode.
fn prompt_update_run_mode(
    input: &dyn Input,
    current: &EventActionRunningMode,
) -> Result<Option<EventActionRunningMode>> {
    let options = vec![
        "Parallel (all actions run simultaneously)".to_string(),
        "Sequential (actions run one after another)".to_string(),
    ];

    let modes = [
        EventActionRunningMode::Parallel,
        EventActionRunningMode::Sequential,
    ];

    let current_idx = modes.iter().position(|m| m == current).unwrap_or(0);
    let prompt = format!("Action run mode (current: {}):", format_run_mode(current));

    match select_index(input, &prompt, &options) {
        Ok(idx) if idx == current_idx => Ok(None), // No change
        Ok(idx) => Ok(Some(modes[idx])),
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}

/// Prompt to modify actions in interactive mode.
fn prompt_modify_actions(
    input: &dyn Input,
    current_actions: &[EventActionDefinition],
    _context: &ProjectContext,
) -> Result<Vec<EventActionDefinition>> {
    let mut actions = current_actions.to_vec();

    loop {
        // Show current actions
        output_simple("\nCurrent actions:");
        if actions.is_empty() {
            output_simple("  (none)");
        } else {
            for (idx, action) in actions.iter().enumerate() {
                let targets = action
                    .targets
                    .as_ref()
                    .map(|t| {
                        t.iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "none".to_string());
                output_simple(&format!(
                    "  {}. {} -> {}",
                    idx,
                    format_action_type(&action.type_),
                    targets
                ));
            }
        }

        let options = vec![
            "Add new action".to_string(),
            "Remove action".to_string(),
            "Done".to_string(),
        ];

        let choice = match select_index(input, "What would you like to do?", &options) {
            Ok(idx) => idx,
            Err(_) => break, // Non-interactive mode
        };

        match choice {
            0 => {
                // Add new action
                let action_types = vec!["Play", "Stop", "Pause", "Resume", "Seek"];
                let type_idx = match select_index(
                    input,
                    "Action type:",
                    &action_types
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>(),
                ) {
                    Ok(idx) => idx,
                    Err(_) => continue,
                };

                let action_type = match action_types[type_idx] {
                    "Play" => EventActionType::Play,
                    "Stop" => EventActionType::Stop,
                    "Pause" => EventActionType::Pause,
                    "Resume" => EventActionType::Resume,
                    "Seek" => EventActionType::Seek,
                    _ => EventActionType::Play,
                };

                let target_input = match input.prompt_text(
                    "Target asset IDs (comma-separated)",
                    None,
                    None,
                    Some(&|value: &str| {
                        if value.trim().is_empty() {
                            return Ok(Validation::Invalid(
                                "At least one target ID is required".into(),
                            ));
                        }
                        for part in value.split(',') {
                            if part.trim().parse::<u64>().is_err() {
                                return Ok(Validation::Invalid(
                                    format!("'{}' is not a valid ID", part).into(),
                                ));
                            }
                        }
                        Ok(Validation::Valid)
                    }),
                ) {
                    Ok(val) => val,
                    Err(_) => continue,
                };

                let target_ids: Vec<u64> = target_input
                    .split(',')
                    .map(|s| s.trim().parse().unwrap())
                    .collect();

                let action = EventActionDefinition {
                    type_: action_type,
                    active: true,
                    scope: Scope::Entity,
                    targets: Some(target_ids),
                };
                actions.push(action);
            }
            1 => {
                // Remove action
                if actions.is_empty() {
                    output_simple("No actions to remove.");
                    continue;
                }

                let remove_options: Vec<String> = actions
                    .iter()
                    .enumerate()
                    .map(|(idx, a)| {
                        let targets = a
                            .targets
                            .as_ref()
                            .map(|t| {
                                t.iter()
                                    .map(|id| id.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_else(|| "none".to_string());
                        format!("{}: {} -> {}", idx, format_action_type(&a.type_), targets)
                    })
                    .collect();

                match select_index(input, "Select action to remove:", &remove_options) {
                    Ok(idx) => {
                        actions.remove(idx);
                    }
                    Err(_) => {}
                }
            }
            _ => break,
        }
    }

    Ok(actions)
}

/// Delete an event asset.
async fn delete_event(
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
        output_simple(&format!(
            "\n{} You are about to delete the following event:",
            "⚠".yellow()
        ));
        output_simple(&format!("  Name: {}", event.name()));
        output_simple(&format!("  ID: {}", event.id));
        output_simple(&format!(
            "  Actions: {}",
            event.actions.as_ref().map(|a| a.len()).unwrap_or(0)
        ));
        output_simple(&format!("  File: {}\n", event_file_path.display()));

        if !dependent_soundbanks.is_empty() && !force {
            output_simple(&format!(
                "{} This event is included in {} soundbank(s).",
                "⚠".yellow(),
                dependent_soundbanks.len()
            ));
            output_simple("  Use --force to delete anyway.\n");
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
