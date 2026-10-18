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

//! Effect asset commands.
//!
//! Implements CRUD operations for Effect assets in Amplitude projects. The
//! create/update handlers only gather values (flags, then prompts when the
//! session is interactive); building and merging lives in `services::effect`.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, Effect, ProjectContext, ProjectValidator},
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
    services::effect::{EffectPatch, EffectValues, apply_patch, build_effect, filter_warning},
};

use super::effect_args::{
    EffectEditArgs, EffectExtraArgs, EffectFlags, resolve_patch, validate_patch_references,
};
use super::effect_list::{delete_effect, list_effects};
use super::effect_prompts::{complete_create_patch, prompt_update_patch};
use super::sound_args::with_warnings;

/// The name of the current asset.
const ASSET_NAME: &str = "Effect";

/// Effect asset subcommands.
#[derive(Subcommand, Debug)]
pub enum EffectCommands {
    /// Create a new effect asset
    #[command(after_help = "Examples:
  am asset effect create hall --effect-type Delay --param 0.5 --param 0.3
  am asset effect create wobble --effect-type Flanger --param-rtpc 0=speed
  am asset effect create from_file --effect-type Equalizer --parameters-from-json params.json

Parameters are positional: the Nth value feeds the Nth parameter of the filter.
  --param N                 a static number; repeat for several
  --param-rtpc INDEX=RTPC   bind the parameter at INDEX (0-based) to an RTPC by name or ID;
                            INDEX may be the current length to append
  --parameters-from-json F  a list of numbers, \"rtpc:NAME\" strings or value objects, or an
                            SDK effect object with \"parameters\"

The filter is the exact name of a registered SDK filter: BassBoost, BiquadResonant,
CompressorFilter, DCRemoval, Delay, Equalizer, Flanger, Lofi, MonoPole, PitchShift, Robotize,
WaveShaper. A name outside this list only loads if a plugin registers it, so the CLI warns.
")]
    Create {
        /// Name of the effect asset
        name: String,

        /// Name of the registered SDK filter the effect applies (required; for example
        /// Delay, Equalizer, Lofi)
        #[arg(long)]
        effect_type: Option<String>,

        /// Parameter values as static floats (repeatable)
        #[arg(long = "param", allow_negative_numbers = true)]
        param: Option<Vec<f32>>,

        #[command(flatten)]
        extra: EffectExtraArgs,
    },

    /// List all effect assets in the project
    #[command(after_help = "Examples:
  am asset effect list
  am asset effect list --json
")]
    List {},

    /// Update an existing effect asset
    #[command(after_help = "Examples:
  am asset effect update hall --effect-type Equalizer
  am asset effect update hall --param-rtpc 1=wind
  am asset effect update hall --add-param 0.4 --add-param rtpc:speed --remove-param 0
  am asset effect update hall --clear-params
")]
    Update {
        /// Name of the effect asset to update
        name: String,

        /// New registered SDK filter name
        #[arg(long)]
        effect_type: Option<String>,

        /// New parameter values as static floats (repeatable, replaces the list)
        #[arg(long = "param", allow_negative_numbers = true)]
        param: Option<Vec<f32>>,

        #[command(flatten)]
        extra: EffectExtraArgs,

        #[command(flatten)]
        edit: EffectEditArgs,
    },

    /// Delete an effect asset
    #[command(after_help = "Examples:
  am asset effect delete reverb
  am asset effect delete reverb --force
")]
    Delete {
        /// Name of the effect asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle effect commands by routing to the appropriate handler.
pub async fn handler(
    command: &EffectCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        EffectCommands::Create {
            name,
            effect_type,
            param,
            extra,
        } => {
            let flags = EffectFlags {
                effect_type: effect_type.clone(),
                param: param.clone(),
                extra: extra.clone(),
                ..Default::default()
            };
            create_effect(name, flags, input, output).await
        }
        EffectCommands::List {} => list_effects(output).await,
        EffectCommands::Update {
            name,
            effect_type,
            param,
            extra,
            edit,
        } => {
            let flags = EffectFlags {
                effect_type: effect_type.clone(),
                param: param.clone(),
                extra: extra.clone(),
                edit: edit.clone(),
            };
            update_effect(name, flags, input, output).await
        }
        EffectCommands::Delete { name, force } => delete_effect(name, *force, input, output).await,
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

/// Prints the warning for a filter the SDK does not register by itself, and returns it for
/// the JSON result.
fn filter_warnings(filter: Option<&str>, output: &dyn Output) -> Vec<String> {
    let warnings: Vec<String> = filter.and_then(filter_warning).into_iter().collect();
    for warning in &warnings {
        output.warning(warning);
    }
    warnings
}

// =============================================================================
// Create
// =============================================================================

/// Create a new effect asset.
async fn create_effect(
    name: &str,
    flags: EffectFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Validate name is not empty and contains no path separators
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Effect name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset effect create <name>'")
        .into());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Effect name contains invalid characters",
            "Name cannot contain path separators (/, \\) or parent directory references (..)",
        )
        .with_suggestion("Use a simple name without slashes or dots")
        .into());
    }

    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating effect '{}' in project '{}'...",
        name, project_config.name
    ));

    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let effects_dir = sources_base.join("effects");
    let effect_file_path = effects_dir.join(format!("{}.json", name));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset effect update {}' to modify it, or choose a different name",
            name
        ))
    };
    if effect_file_path.exists() {
        return Err(already_exists().into());
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;
    if context.has_name(AssetType::Effect, name) {
        return Err(already_exists().into());
    }

    // Resolve flags; unknown names and dangling ids fail before any prompt
    let patch = resolve_patch(&flags, validator)?;
    validate_patch_references(&patch, validator)?;

    // The filter is the one required field; everything else has a default
    let patch = if can_prompt(input, output) {
        complete_create_patch(input, validator, patch)?
    } else {
        patch
    };

    let effect = build_effect(EffectValues::from_patch(name, &patch)?, &context)?;
    let json_content =
        serde_json::to_string_pretty(&effect).context("Failed to serialize effect to JSON")?;
    fs::create_dir_all(&effects_dir)?;
    atomic_write(&effect_file_path, json_content.as_bytes())?;

    let warnings = filter_warnings(effect.effect.as_deref(), output);
    let parameter_count = effect.parameters.as_ref().map_or(0, |p| p.len());
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": effect.id,
                        "name": effect.name(),
                        "path": effect_file_path.to_string_lossy(),
                        "effect_type": effect.effect,
                        "parameter_count": parameter_count,
                    }),
                    &warnings,
                ),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Effect '{}' created successfully at {}",
                    name,
                    effect_file_path.display()
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

/// Update an existing effect asset.
async fn update_effect(
    name: &str,
    flags: EffectFlags,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating effect '{}' in project '{}'...",
        name, project_config.name
    ));

    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let effect_file_path = sources_base.join("effects").join(format!("{}.json", name));

    if !effect_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset effect list' to see available effects, or 'am asset effect create {}' to create it",
                name
            ))
            .into());
    }

    // Parse the existing effect (typed, and raw to keep what the CLI does not model)
    let content = fs::read_to_string(&effect_file_path).context(format!(
        "Failed to read effect file: {}",
        effect_file_path.display()
    ))?;
    let existing: Effect = serde_json::from_str(&content).context(format!(
        "Failed to parse effect file: {}",
        effect_file_path.display()
    ))?;
    let original: serde_json::Value = serde_json::from_str(&content).context(format!(
        "Failed to parse effect file: {}",
        effect_file_path.display()
    ))?;

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    let validator = context
        .validator
        .as_ref()
        .context("Project validator is unavailable")?;

    // Flags win; with none, an interactive session is offered every field
    let mut patch: EffectPatch = resolve_patch(&flags, validator)?;
    validate_patch_references(&patch, validator)?;
    if patch.is_empty() && can_prompt(input, output) {
        patch = prompt_update_patch(input, validator, &existing)?;
    }

    // Merge into a copy and validate the new state
    let effect = apply_patch(&existing, &patch)?;
    effect.validate_rules(&context)?;

    // Serialize over the original file, keeping keys the CLI does not model
    let known = known_field_names(AssetType::Effect);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let mut updated =
        serde_json::to_value(&effect).context("Failed to serialize effect to JSON")?;
    if patch.parameters.is_empty()
        && let (Some(raw), Some(map)) = (original.get("parameters"), updated.as_object_mut())
    {
        // Untouched parameters stay exactly as written (including keys the CLI does not model).
        map.insert("parameters".to_string(), raw.clone());
    }
    let merged = merge_update_unknown_only(&original, &updated, &known);
    let json_content =
        serde_json::to_string_pretty(&merged).context("Failed to serialize effect to JSON")?;
    atomic_write(&effect_file_path, json_content.as_bytes())?;

    let warnings = filter_warnings(patch.effect.as_deref(), output);
    match output.mode() {
        OutputMode::Json => {
            output.success(
                with_warnings(
                    json!({
                        "id": effect.id,
                        "name": effect.name(),
                        "path": effect_file_path.to_string_lossy(),
                        "parameter_count": effect.parameters.as_ref().map_or(0, |p| p.len()),
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
                    "Effect '{}' updated successfully at {}",
                    name,
                    effect_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}
