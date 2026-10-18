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

//! Soundbank asset commands.
//!
//! Implements CRUD operations for Soundbank assets in Amplitude projects. Soundbanks
//! package assets for runtime loading and name them by *runtime file name*
//! (`explosion.amsound`); the create/update handlers only gather source asset names
//! (flags, then prompts when the session is interactive). Resolving them and merging
//! lives in `services::soundbank`.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, ProjectContext, ProjectValidator, Soundbank, SoundbankList},
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
    services::soundbank::{
        Catalog, SoundbankPatch, SoundbankValues, apply_patch, build_soundbank,
    },
};

use super::soundbank_args::{SoundbankEditArgs, SoundbankFlags, SoundbankListArgs, resolve_patch};
use super::soundbank_list::{count_assets_by_type, delete_soundbank, list_soundbanks};
use super::soundbank_prompts::{complete_create_patch, prompt_update_patch};

/// The name of the current asset.
const ASSET_NAME: &str = "Soundbank";

/// Soundbank asset subcommands.
#[derive(Subcommand, Debug)]
pub enum SoundbankCommands {
    /// Create a new soundbank asset
    #[command(after_help = "Examples:
  am asset soundbank create main_bank --sound explosion,footstep --collection ambience
  am asset soundbank create main_bank --include sound:explosion,footstep --include collection:ambience
  am asset soundbank create main_bank --from-json bank_lists.json

Assets are given by name. The soundbank stores the runtime file name the SDK loads
(explosion -> explosion.amsound, a sound in sounds/fx/boom.json -> fx/boom.amsound).
Every list flag takes comma-separated names and can be repeated. A soundbank may be empty
(the SDK loads it). --from-json reads an object of lists: {\"sounds\": [\"explosion\"], \"events\": [...]}.
")]
    Create {
        /// Name of the soundbank asset
        name: String,

        /// Include assets in format "type:name[,name,...]" (repeatable)
        /// Example: --include sound:footstep,explosion --include collection:ambience
        #[arg(short, long)]
        include: Vec<String>,

        #[command(flatten)]
        lists: SoundbankListArgs,

        /// Read the lists from a JSON file: an object keyed by sounds, collections, events,
        /// switches, switch_containers, effects, attenuators, rtpc (an SDK soundbank works)
        #[arg(long, value_name = "FILE")]
        from_json: Option<std::path::PathBuf>,
    },

    /// List all soundbank assets in the project
    #[command(after_help = "Examples:\n  am asset soundbank list\n")]
    List {},

    /// Update an existing soundbank asset
    #[command(after_help = "Examples:
  am asset soundbank update main_bank --add-sound newsfx --remove-event old_event
  am asset soundbank update main_bank --add sound:newsfx
  am asset soundbank update main_bank --sound explosion,footstep
  am asset soundbank update main_bank --clear-effects

--sound, --collection, ... replace the whole list; --add-* and --remove-* edit it (names, or
entries as written in the file); --clear-* empties it. Lists you do not mention are kept.
")]
    Update {
        /// Name of the soundbank asset to update
        name: String,

        /// Add assets in format "type:name[,name,...]" (repeatable)
        #[arg(short, long)]
        add: Vec<String>,

        /// Remove assets in format "type:name[,name,...]" (repeatable)
        #[arg(short, long)]
        remove: Vec<String>,

        #[command(flatten)]
        lists: SoundbankListArgs,

        #[command(flatten)]
        edit: SoundbankEditArgs,

        /// Replace the lists found in a JSON file (see 'create --help')
        #[arg(long, value_name = "FILE")]
        from_json: Option<std::path::PathBuf>,
    },

    /// Delete a soundbank asset
    #[command(
        after_help = "Examples:\n  am asset soundbank delete main_bank\n  am asset soundbank delete main_bank --yes\n"
    )]
    Delete {
        /// Name of the soundbank asset to delete
        name: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
}

/// Handle soundbank commands by routing to the appropriate handler.
pub async fn handler(
    command: &SoundbankCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        SoundbankCommands::Create {
            name,
            include,
            lists,
            from_json,
        } => {
            let flags = SoundbankFlags {
                lists: lists.clone(),
                include: include.clone(),
                from_json: from_json.clone(),
                ..Default::default()
            };
            create_soundbank(name, flags, input, output).await
        }
        SoundbankCommands::List {} => list_soundbanks(output).await,
        SoundbankCommands::Update {
            name,
            add,
            remove,
            lists,
            edit,
            from_json,
        } => {
            let flags = SoundbankFlags {
                lists: lists.clone(),
                edit: edit.clone(),
                add: add.clone(),
                remove: remove.clone(),
                from_json: from_json.clone(),
                ..Default::default()
            };
            update_soundbank(name, flags, input, output).await
        }
        SoundbankCommands::Delete { name, yes } => {
            delete_soundbank(name, *yes, input, output).await
        }
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

fn type_counts(soundbank: &Soundbank) -> serde_json::Value {
    count_assets_by_type(soundbank)
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into()
}

// =============================================================================
// Create
// =============================================================================

/// Create a new soundbank asset.
async fn create_soundbank(
    name: &str,
    flags: SoundbankFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Soundbank name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset soundbank create <name>'")
        .into());
    }

    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating soundbank '{}' in project '{}'...",
        name, project_config.name
    ));

    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let soundbanks_dir = sources_base.join("soundbanks");
    let soundbank_file_path = soundbanks_dir.join(format!("{}.json", name));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset soundbank update {}' to modify it, or choose a different name",
            name
        ))
    };
    if soundbank_file_path.exists() {
        return Err(already_exists().into());
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let catalog = Catalog::from_validator(&validator);
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    if context.has_name(AssetType::Soundbank, name) {
        return Err(already_exists().into());
    }

    // Resolve flags; bad syntax fails before any prompt
    let patch: SoundbankPatch = resolve_patch(&flags)?;
    let patch = if can_prompt(input, output) {
        complete_create_patch(input, &catalog, patch)?
    } else {
        patch
    };

    let values = SoundbankValues::from_patch(name, &patch, &catalog)?;
    let soundbank = build_soundbank(values, &context)?;

    let json_content = serde_json::to_string_pretty(&soundbank)
        .context("Failed to serialize soundbank to JSON")?;
    fs::create_dir_all(&soundbanks_dir)?;
    atomic_write(&soundbank_file_path, json_content.as_bytes())?;

    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": soundbank.id,
                    "name": soundbank.name(),
                    "path": soundbank_file_path.to_string_lossy(),
                    "asset_count": soundbank.asset_count(),
                    "assets_by_type": type_counts(&soundbank),
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Soundbank '{}' created successfully at {}\n  Assets: {} total",
                    name,
                    soundbank_file_path.display(),
                    soundbank.asset_count()
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

/// Update an existing soundbank asset.
async fn update_soundbank(
    name: &str,
    flags: SoundbankFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating soundbank '{}' in project '{}'...",
        name, project_config.name
    ));

    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let soundbank_file_path = sources_base
        .join("soundbanks")
        .join(format!("{}.json", name));

    if !soundbank_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset soundbank list' to see available soundbanks, or 'am asset soundbank create {}' to create it",
                name
            ))
            .into());
    }

    // Parse the existing soundbank (typed, and raw to keep what the CLI does not model)
    let content = fs::read_to_string(&soundbank_file_path).context(format!(
        "Failed to read soundbank file: {}",
        soundbank_file_path.display()
    ))?;
    let existing: Soundbank = serde_json::from_str(&content).context(format!(
        "Failed to parse soundbank file: {}",
        soundbank_file_path.display()
    ))?;
    let original: serde_json::Value = serde_json::from_str(&content).context(format!(
        "Failed to parse soundbank file: {}",
        soundbank_file_path.display()
    ))?;

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let catalog = Catalog::from_validator(&validator);
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Flags win; with none, an interactive session is offered every list
    let mut patch = resolve_patch(&flags)?;
    if patch.is_empty() && can_prompt(input, output) {
        patch = prompt_update_patch(input, &catalog, &existing)?;
    }

    // Merge into a copy and validate the new state
    let soundbank = apply_patch(&existing, &patch, &catalog)?;
    soundbank.validate_rules(&context)?;

    // Serialize over the original file, keeping keys the CLI does not model
    let known = known_field_names(AssetType::Soundbank);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let mut updated =
        serde_json::to_value(&soundbank).context("Failed to serialize soundbank to JSON")?;
    if let Some(map) = updated.as_object_mut() {
        // Untouched lists stay exactly as written.
        for list in SoundbankList::ALL {
            if patch.list(list).is_none()
                && let Some(raw) = original.get(list.field())
            {
                map.insert(list.field().to_string(), raw.clone());
            }
        }
    }
    let merged = merge_update_unknown_only(&original, &updated, &known);
    let json_content = serde_json::to_string_pretty(&merged)
        .context("Failed to serialize soundbank to JSON")?;
    atomic_write(&soundbank_file_path, json_content.as_bytes())?;

    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": soundbank.id,
                    "name": soundbank.name(),
                    "path": soundbank_file_path.to_string_lossy(),
                    "asset_count": soundbank.asset_count(),
                    "updated_fields": patch.changed_fields(),
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Soundbank '{}' updated successfully at {}\n  Assets: {} total",
                    name,
                    soundbank_file_path.display(),
                    soundbank.asset_count()
                )),
                None,
            );
        }
    }

    Ok(())
}
