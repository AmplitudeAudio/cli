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

//! Attenuation asset commands.
//!
//! Implements CRUD operations for Attenuation assets in Amplitude projects. The
//! create/update handlers only gather values (flags, then prompts when the
//! session is interactive); building and merging lives in `services::attenuation`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, Attenuation, ProjectContext, ProjectValidator},
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        json_merge::merge_update_unknown_only,
        utils::read_amproject_file,
    },
    database::Database,
    forms::coverage::known_field_names,
    forms::references::{RefTarget, referrers_with_type},
    forms::zone::zone_shape_name,
    input::Input,
    presentation::{Output, OutputMode},
    services::attenuation::{
        AirAbsorptionValues, AttenuationPatch, AttenuationValues, DEFAULT_MAX_ATTENUATION_FACTOR,
        apply_patch, build_attenuation,
    },
};

use super::attenuation_args::{AttenuationArgs, resolve_patch};
use super::attenuation_prompts::{complete_optional, complete_required, prompt_update_patch};
use super::find_json_files_recursive;

/// The name of the current asset, used in error messages and output.
const ASSET_NAME: &str = "Attenuation";

/// Attenuation asset subcommands.
#[derive(Subcommand, Debug)]
pub enum AttenuationCommands {
    /// Create a new attenuation asset
    #[command(after_help = "Examples:
  am asset attenuation create short --max-distance 50 --shape sphere \\
      --inner-radius 5 --outer-radius 10 --curve-part 0,1,1,0,Linear
  am asset attenuation create wide --max-distance 120 --shape box \\
      --inner-half-width 2 --inner-half-height 2 --inner-half-depth 2 \\
      --outer-half-width 8 --outer-half-height 8 --outer-half-depth 8 \\
      --curve-from-json gain.json --air-absorption-coefficients 0.0002,0.0017,0.0182
")]
    Create {
        /// Name of the attenuation asset
        name: String,

        #[command(flatten)]
        args: AttenuationArgs,
    },

    /// List all attenuation assets in the project
    #[command(after_help = "Examples:
  am asset attenuation list
  am asset attenuation list --json
")]
    List {},

    /// Update an existing attenuation asset (only the flags you pass change)
    #[command(after_help = "Examples:
  am asset attenuation update short --max-distance 80
  am asset attenuation update short --outer-radius 20
  am asset attenuation update short --air-absorption false
")]
    Update {
        /// Name of the attenuation asset to update
        name: String,

        #[command(flatten)]
        args: AttenuationArgs,
    },

    /// Delete an attenuation asset (refused while other assets reference it)
    #[command(after_help = "Examples:
  am asset attenuation delete short
  am asset attenuation delete short --force
")]
    Delete {
        /// Name of the attenuation asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle attenuation commands by routing to the appropriate handler.
pub async fn handler(
    command: &AttenuationCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        AttenuationCommands::Create { name, args } => create_attenuation(name, args, input, output),
        AttenuationCommands::List {} => list_attenuations(output),
        AttenuationCommands::Update { name, args } => update_attenuation(name, args, input, output),
        AttenuationCommands::Delete { name, force } => {
            delete_attenuation(name, *force, input, output)
        }
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

fn missing_flag_error(flag: &str, what: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Missing required {flag}"),
        format!("{what} is required and cannot be prompted for in non-interactive or JSON mode"),
    )
    .with_suggestion(format!(
        "Pass {flag} (see 'am asset attenuation create --help')"
    ))
}

/// The directory attenuations live in, plus the project's root.
fn attenuations_dir() -> Result<(PathBuf, PathBuf, String)> {
    let current_dir = env::current_dir()?;
    let project = read_amproject_file(&current_dir)?;
    let sources_base = if project.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project.sources_dir)
    };
    let dir = sources_base.join(AssetType::Attenuation.directory_name());
    Ok((current_dir, dir, project.name))
}

/// The non-interactive requirements of a new attenuation, as flag-named errors.
fn require_values(patch: AttenuationPatch) -> Result<AttenuationPatch> {
    if patch.max_distance.is_none() {
        return Err(missing_flag_error("--max-distance", "The maximum distance").into());
    }
    if patch.zone.is_none() {
        return Err(missing_flag_error(
            "--shape",
            "The attenuation zone (--shape and its dimensions)",
        )
        .into());
    }
    if patch.gain_curve.is_none() {
        return Err(missing_flag_error(
            "--curve-part",
            "The gain curve (--curve-part or --curve-from-json)",
        )
        .into());
    }
    Ok(patch)
}

fn to_values(name: &str, patch: AttenuationPatch) -> Result<AttenuationValues> {
    let (Some(max_distance), Some(zone), Some(gain_curve)) =
        (patch.max_distance, patch.zone, patch.gain_curve)
    else {
        anyhow::bail!("attenuation values are incomplete");
    };
    let air = patch.air_absorption;
    let air_absorption =
        (air.enabled.is_some() || air.coefficients.is_some()).then(|| AirAbsorptionValues {
            enabled: air.enabled.unwrap_or(true),
            coefficients: air.coefficients,
        });
    Ok(AttenuationValues {
        name: name.to_string(),
        max_distance,
        zone,
        max_attenuation_factor: patch
            .max_attenuation_factor
            .unwrap_or(DEFAULT_MAX_ATTENUATION_FACTOR),
        gain_curve,
        spread_curve: patch.spread_curve,
        air_absorption,
    })
}

/// Create a new attenuation asset.
fn create_attenuation(
    name: &str,
    args: &AttenuationArgs,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let (current_dir, dir, project_name) = attenuations_dir()?;
    output.progress(&format!(
        "Creating attenuation '{name}' in project '{project_name}'..."
    ));

    let file_path = dir.join(format!("{name}.json"));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset attenuation update {name}' to modify it, or choose a different name"
        ))
    };
    if file_path.exists() {
        return Err(already_exists().into());
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir).with_validator(validator);
    if context.has_name(AssetType::Attenuation, name) {
        return Err(already_exists().into());
    }

    // Flags first; whatever is still missing is asked for, or reported by flag name.
    let patch = resolve_patch(args, None)?;
    let patch = if can_prompt(input, output) {
        complete_optional(input, complete_required(input, patch)?)?
    } else {
        require_values(patch)?
    };

    let attenuation = build_attenuation(to_values(name, patch)?, &context)?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
    let json_content = serde_json::to_string_pretty(&attenuation)
        .context("Failed to serialize attenuation to JSON")?;
    atomic_write(&file_path, json_content.as_bytes())?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({
                "id": attenuation.id,
                "name": attenuation.name(),
                "path": file_path.to_string_lossy(),
                "max_distance": attenuation.max_distance,
            }),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Attenuation '{name}' created successfully at {}",
                file_path.display()
            )),
            None,
        ),
    }
    Ok(())
}

fn shape_of(attenuation: &Attenuation) -> &'static str {
    attenuation
        .shape
        .as_ref()
        .and_then(|s| s.zone.as_ref())
        .map_or("none", zone_shape_name)
}

fn factor_of(attenuation: &Attenuation) -> f32 {
    attenuation
        .shape
        .as_ref()
        .map_or(DEFAULT_MAX_ATTENUATION_FACTOR, |s| s.max_attenuation_factor)
}

fn read_attenuations(
    dir: &std::path::Path,
    output: &dyn Output,
) -> Result<(Vec<Attenuation>, Vec<String>)> {
    let mut attenuations = Vec::new();
    let mut warnings = Vec::new();
    for path in find_json_files_recursive(dir)? {
        let parsed = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Attenuation>(&text).map_err(|e| e.to_string()));
        match parsed {
            Ok(attenuation) => attenuations.push(attenuation),
            Err(reason) => {
                output.warning(&format!(
                    "Skipping invalid attenuation file: {}",
                    path.display()
                ));
                warnings.push(format!(
                    "Invalid attenuation file {}: {reason}",
                    path.display()
                ));
            }
        }
    }
    attenuations.sort_by(|a, b| a.name().cmp(b.name()));
    Ok((attenuations, warnings))
}

/// List all attenuation assets in the current project.
fn list_attenuations(output: &dyn Output) -> Result<()> {
    let (_, dir, _) = attenuations_dir()?;
    let (attenuations, warnings) = read_attenuations(&dir, output)?;

    match output.mode() {
        OutputMode::Json => {
            let rows: Vec<_> = attenuations
                .iter()
                .map(|a| {
                    json!({
                        "id": a.id,
                        "name": a.name(),
                        "max_distance": a.max_distance,
                        "shape": shape_of(a),
                        "max_attenuation_factor": factor_of(a),
                    })
                })
                .collect();
            output.success(
                json!({"attenuations": rows, "count": attenuations.len(), "warnings": warnings}),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {warning}", "Warning:".yellow()));
            }
            if attenuations.is_empty() {
                output.progress("No attenuations found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset attenuation create <name>".green()
                ));
                return Ok(());
            }
            let rows: Vec<_> = attenuations
                .iter()
                .map(|a| {
                    json!({
                        "id": a.id,
                        "name": a.name(),
                        "max_distance": a.max_distance,
                        "shape": shape_of(a),
                    })
                })
                .collect();
            output.table(None, json!(rows));
            output.progress("");
            output.progress(&format!("{} attenuation(s) found", attenuations.len()));
        }
    }
    Ok(())
}

fn no_changes_error(name: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        "Nothing to update",
        "No flags were given and the session cannot prompt for values",
    )
    .with_suggestion(format!(
        "Pass at least one flag, e.g. 'am asset attenuation update {name} --max-distance 80'"
    ))
}

fn locate(name: &str, dir: &std::path::Path) -> Result<(PathBuf, Attenuation)> {
    let path = dir.join(format!("{name}.json"));
    if !path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset attenuation list' to see available attenuations, or 'am asset attenuation create {name}' to create it"
            ))
            .into());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read attenuation file: {}", path.display()))?;
    let attenuation = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse attenuation file: {}", path.display()))?;
    Ok((path, attenuation))
}

/// Update an existing attenuation asset.
fn update_attenuation(
    name: &str,
    args: &AttenuationArgs,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let (current_dir, dir, project_name) = attenuations_dir()?;
    output.progress(&format!(
        "Updating attenuation '{name}' in project '{project_name}'..."
    ));
    let (file_path, existing) = locate(name, &dir)?;

    let mut patch = resolve_patch(args, Some(&existing))?;
    if patch.is_empty() {
        if !can_prompt(input, output) {
            return Err(no_changes_error(name).into());
        }
        patch = prompt_update_patch(input, &existing)?;
    }

    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir).with_validator(validator);
    let attenuation = apply_patch(&existing, &patch);
    attenuation.validate_rules(&context)?;

    // Serialize over the original file, keeping keys the CLI does not model.
    let original: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file_path).with_context(|| {
            format!("Failed to read attenuation file: {}", file_path.display())
        })?)
        .with_context(|| format!("Failed to parse attenuation file: {}", file_path.display()))?;
    let known = known_field_names(AssetType::Attenuation);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let updated =
        serde_json::to_value(&attenuation).context("Failed to serialize attenuation to JSON")?;
    let merged = merge_update_unknown_only(&original, &updated, &known);
    let json_content =
        serde_json::to_string_pretty(&merged).context("Failed to serialize attenuation to JSON")?;
    atomic_write(&file_path, json_content.as_bytes())?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({
                "id": attenuation.id,
                "name": attenuation.name(),
                "path": file_path.to_string_lossy(),
                "updated_fields": patch.changed_fields(),
            }),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Attenuation '{name}' updated successfully at {}",
                file_path.display()
            )),
            None,
        ),
    }
    Ok(())
}

fn in_use_error(name: &str, users: &[(AssetType, crate::forms::references::AssetRef)]) -> CliError {
    let names = users
        .iter()
        .map(|(kind, asset)| format!("{kind} '{}'", asset.name))
        .collect::<Vec<_>>()
        .join(", ");
    let fixes = users
        .iter()
        .map(|(kind, asset)| format!("am asset {kind} update {} --attenuation none", asset.name))
        .collect::<Vec<_>>()
        .join("; ");
    CliError::new(
        codes::ERR_ASSET_IN_USE,
        format!("Attenuation '{name}' is still referenced"),
        format!("It is used by: {names}"),
    )
    .with_suggestion(format!(
        "Update or remove the reference first (e.g. {fixes}), then delete the attenuation"
    ))
}

/// Delete an attenuation asset.
fn delete_attenuation(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let (current_dir, dir, _) = attenuations_dir()?;
    let (file_path, attenuation) = locate(name, &dir)?;

    // Referenced assets would be left with a dangling id, so --force does not override this.
    let validator = ProjectValidator::new(current_dir, output)?;
    let users = referrers_with_type(
        &validator,
        RefTarget::Asset(AssetType::Attenuation),
        attenuation.id,
    );
    if !users.is_empty() {
        return Err(in_use_error(name, &users).into());
    }

    let confirmed = force
        || input
            .confirm(
                &format!("Delete attenuation '{name}'? This cannot be undone."),
                Some(false),
            )
            .map_err(|_| {
                CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Deletion requires confirmation",
                    "In non-interactive mode, use --force to confirm deletion",
                )
                .with_suggestion(format!(
                    "Use 'am asset attenuation delete {name} --force' to delete without prompting"
                ))
            })?;
    if !confirmed {
        output.progress("Deletion cancelled.");
        return Ok(());
    }

    fs::remove_file(&file_path)
        .with_context(|| format!("Failed to delete attenuation file: {}", file_path.display()))?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"id": attenuation.id, "name": attenuation.name(), "deleted": true}),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!("Attenuation '{name}' deleted successfully")),
            None,
        ),
    }
    Ok(())
}
