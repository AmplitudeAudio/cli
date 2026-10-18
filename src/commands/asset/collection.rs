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

//! Collection asset commands.
//!
//! Implements CRUD operations for Collection assets in Amplitude projects. The
//! create/update handlers only gather values (flags, then prompts when the
//! session is interactive); building and merging lives in `services::collection`.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, Collection, ProjectContext, ProjectValidator},
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
    services::collection::{CollectionPatch, CollectionValues, apply_patch, build_collection},
};

use super::collection_args::{
    CollectionEntryEditArgs, CollectionExtraArgs, CollectionFlags, resolve_patch,
    validate_patch_references,
};
use super::collection_list::{delete_collection, list_collections};
use super::collection_prompts::{complete_create_patch, prompt_bus, prompt_update_patch};
use super::sound_args::{bus_warnings, with_warnings};

/// The name of the current asset.
const ASSET_NAME: &str = "Collection";

/// Collection asset subcommands.
#[derive(Subcommand, Debug)]
pub enum CollectionCommands {
    /// Create a new collection asset
    #[command(after_help = "Examples:
  am asset collection create footsteps --bus sfx --entry step1 --entry step2
  am asset collection create ambience --bus sfx --play-mode PlayAll --gain 0.7
  am asset collection create hits --bus sfx --avoid-repeat true --repeat-count 2 \\
      --entry hit1,weight=3 --entry hit2,gain=0.8,pitch-rtpc=speed
  am asset collection create stairs --bus sfx --scheduler-mode Sequence --end-behavior Reverse \\
      --entries-from-json stairs.json
")]
    Create {
        /// Name of the collection asset
        name: String,

        /// Play mode: PlayOne or PlayAll (default: PlayOne)
        #[arg(long)]
        play_mode: Option<String>,

        /// Scheduler mode: Random or Sequence (default: Random)
        #[arg(long)]
        scheduler_mode: Option<String>,

        /// Volume gain (0.0-1.0, default: 1.0)
        #[arg(short, long)]
        gain: Option<f32>,

        /// Bus to route the collection to, by name or ID (required; see 'am asset bus list')
        #[arg(short, long)]
        bus: Option<String>,

        /// Playback priority (0-255, default: 128)
        #[arg(short, long)]
        priority: Option<u8>,

        /// Spatialization mode: none, position, position_orientation, hrtf (default: none)
        #[arg(short, long)]
        spatialization: Option<String>,

        #[command(flatten)]
        extra: CollectionExtraArgs,
    },

    /// List all collection assets in the project
    #[command(after_help = "Examples:
  am asset collection list
  am asset collection list --json
")]
    List {},

    /// Update an existing collection asset
    #[command(after_help = "Examples:
  am asset collection update footsteps --play-mode PlayAll
  am asset collection update ambience --gain 0.5 --effect echo
  am asset collection update hits --add-entry hit3,weight=2 --remove-entry hit1
  am asset collection update hits --scheduler-mode Sequence --end-behavior Reverse
")]
    Update {
        /// Name of the collection asset to update
        name: String,

        /// New play mode: PlayOne or PlayAll
        #[arg(long)]
        play_mode: Option<String>,

        /// New scheduler mode: Random or Sequence (re-tags the entries and drops the old scheduler's settings)
        #[arg(long)]
        scheduler_mode: Option<String>,

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
        extra: CollectionExtraArgs,

        #[command(flatten)]
        edit: CollectionEntryEditArgs,
    },

    /// Delete a collection asset
    #[command(after_help = "Examples:
  am asset collection delete footsteps
  am asset collection delete footsteps --force
")]
    Delete {
        /// Name of the collection asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle collection commands by routing to the appropriate handler.
pub async fn handler(
    command: &CollectionCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        CollectionCommands::Create {
            name,
            play_mode,
            scheduler_mode,
            gain,
            bus,
            priority,
            spatialization,
            extra,
        } => {
            let flags = CollectionFlags {
                play_mode: play_mode.clone(),
                scheduler_mode: scheduler_mode.clone(),
                gain: *gain,
                bus: bus.clone(),
                priority: *priority,
                spatialization: spatialization.clone(),
                extra: extra.clone(),
                edit: CollectionEntryEditArgs::default(),
            };
            create_collection(name, flags, input, output).await
        }
        CollectionCommands::List {} => list_collections(output).await,
        CollectionCommands::Update {
            name,
            play_mode,
            scheduler_mode,
            gain,
            bus,
            priority,
            spatialization,
            extra,
            edit,
        } => {
            let flags = CollectionFlags {
                play_mode: play_mode.clone(),
                scheduler_mode: scheduler_mode.clone(),
                gain: *gain,
                bus: bus.clone(),
                priority: *priority,
                spatialization: spatialization.clone(),
                extra: extra.clone(),
                edit: edit.clone(),
            };
            update_collection(name, flags, input, output).await
        }
        CollectionCommands::Delete { name, force } => {
            delete_collection(name, *force, input, output).await
        }
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

fn missing_bus_error() -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_EMPTY_REFERENCE,
        "Missing required --bus",
        "Collections must be routed to a bus and cannot prompt in non-interactive or JSON mode",
    )
    .with_suggestion(
        "Pass --bus <name|id> (see 'am asset bus list'); if the project has no bus yet, create a bus first",
    )
}

// =============================================================================
// Create
// =============================================================================

/// Create a new collection asset.
async fn create_collection(
    name: &str,
    flags: CollectionFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Validate name is not empty
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Collection name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset collection create <name>'")
        .into());
    }

    // Step 2: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating collection '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 3: Validate the name is free (filesystem + registry)
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let collections_dir = sources_base.join("collections");
    let collection_file_path = collections_dir.join(format!("{}.json", name));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset collection update {}' to modify it, or choose a different name",
            name
        ))
    };

    if collection_file_path.exists() {
        return Err(already_exists().into());
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;
    if context.has_name(AssetType::Collection, name) {
        return Err(already_exists().into());
    }

    // Step 4: Resolve flags; unknown names and dangling ids fail before any prompt
    let patch = resolve_patch(&flags, validator)?;
    validate_patch_references(&patch, validator)?;
    let interactive = can_prompt(input, output);

    // Step 5: The bus is the one required field
    let bus = match (patch.bus, interactive) {
        (Some(bus), _) => bus,
        (None, true) => prompt_bus(input, validator)?,
        (None, false) => return Err(missing_bus_error().into()),
    };
    let patch = CollectionPatch {
        bus: Some(bus),
        ..patch
    };

    // Step 6: Optional fields, interactive only
    let patch = if interactive {
        complete_create_patch(input, validator, patch)?
    } else {
        patch
    };

    // Step 7: Build and validate through the service, then write atomically
    let collection = build_collection(CollectionValues::from_patch(name, &patch)?, &context)?;
    let json_content = serde_json::to_string_pretty(&collection)
        .context("Failed to serialize collection to JSON")?;
    fs::create_dir_all(&collections_dir)?;
    atomic_write(&collection_file_path, json_content.as_bytes())?;

    // Step 8: Output success
    let warnings = bus_warnings(validator, Some(collection.bus), output);
    let scheduler_mode = collection
        .scheduler
        .as_ref()
        .map(|s| s.mode.to_string())
        .unwrap_or_default();
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": collection.id,
                        "name": collection.name(),
                        "path": collection_file_path.to_string_lossy(),
                        "play_mode": collection.play_mode.to_string(),
                        "scheduler_mode": scheduler_mode,
                    }),
                    &warnings,
                ),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Collection '{}' created successfully at {}",
                    name,
                    collection_file_path.display()
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

/// Update an existing collection asset.
async fn update_collection(
    name: &str,
    flags: CollectionFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating collection '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Locate existing collection file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let collection_file_path = sources_base
        .join("collections")
        .join(format!("{}.json", name));

    if !collection_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset collection list' to see available collections, or 'am asset collection create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing collection (typed, and raw to keep unknown keys)
    let content = fs::read_to_string(&collection_file_path).context(format!(
        "Failed to read collection file: {}",
        collection_file_path.display()
    ))?;
    let existing: Collection = serde_json::from_str(&content).context(format!(
        "Failed to parse collection file: {}",
        collection_file_path.display()
    ))?;
    let original: serde_json::Value = serde_json::from_str(&content).context(format!(
        "Failed to parse collection file: {}",
        collection_file_path.display()
    ))?;

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;

    // Step 4: Flags win; with none, an interactive session is offered every field
    let mut patch = resolve_patch(&flags, validator)?;
    validate_patch_references(&patch, validator)?;
    if patch.is_empty() && can_prompt(input, output) {
        patch = prompt_update_patch(input, validator, &existing)?;
    }

    // Step 5: Merge into a copy and validate the new state
    let collection = apply_patch(&existing, &patch)?;
    collection.validate_rules(&context).map_err(|e| {
        if e.field.as_deref() == Some("bus") {
            let suggestion = format!(
                "Run 'am asset collection update {name} --bus <name|id>' with an existing bus (see 'am asset bus list')"
            );
            e.with_suggestion(suggestion)
        } else {
            e
        }
    })?;

    // Step 6: Serialize over the original file, keeping keys the CLI does not model
    let known = known_field_names(AssetType::Collection);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let updated =
        serde_json::to_value(&collection).context("Failed to serialize collection to JSON")?;
    let merged = merge_update_unknown_only(&original, &updated, &known);
    let json_content =
        serde_json::to_string_pretty(&merged).context("Failed to serialize collection to JSON")?;
    atomic_write(&collection_file_path, json_content.as_bytes())?;

    // Step 7: Output success
    let warnings = bus_warnings(validator, patch.bus, output);
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": collection.id,
                        "name": collection.name(),
                        "path": collection_file_path.to_string_lossy(),
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
                    "Collection '{}' updated successfully at {}",
                    name,
                    collection_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}
