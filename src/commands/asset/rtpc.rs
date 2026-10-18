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

//! RTPC asset commands.
//!
//! Implements CRUD operations for RTPC assets in Amplitude projects. The
//! create/update handlers only gather values (flags, then prompts when the
//! session is interactive); building and merging lives in `services::rtpc`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{Asset, AssetType, ProjectContext, ProjectValidator, Rtpc},
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        utils::read_amproject_file,
    },
    database::Database,
    forms::references::{AssetRef, RefTarget, referrers_with_type},
    input::Input,
    presentation::{Output, OutputMode},
    services::rtpc::{
        FadePatch, FadeTransitionValues, FadeValues, RtpcPatch, RtpcValues, apply_patch,
        build_rtpc, merge_into_original,
    },
};

use super::find_json_files_recursive;
use super::rtpc_args::{RtpcArgs, resolve_patch};
use super::rtpc_prompts::{complete_optional, complete_required, prompt_update_patch};

/// The name of the current asset, used in error messages and output.
const ASSET_NAME: &str = "RTPC";

/// RTPC asset subcommands.
#[derive(Subcommand, Debug)]
pub enum RtpcCommands {
    /// Create a new RTPC asset
    #[command(after_help = "Examples:
  am asset rtpc create speed --min-value 0 --max-value 100
  am asset rtpc create health --min-value 0 --max-value 1 --default-value 1 \\
      --fade-attack-duration 0.2 --fade-attack-fader Ease \\
      --fade-release-duration 1 --fade-release-fader Linear
")]
    Create {
        /// Name of the RTPC asset
        name: String,

        #[command(flatten)]
        args: RtpcArgs,
    },

    /// List all RTPC assets in the project
    #[command(after_help = "Examples:
  am asset rtpc list
  am asset rtpc list --json
")]
    List {},

    /// Update an existing RTPC asset (only the flags you pass change)
    #[command(after_help = "Examples:
  am asset rtpc update speed --max-value 200
  am asset rtpc update speed --fade-settings-enabled false
")]
    Update {
        /// Name of the RTPC asset to update
        name: String,

        #[command(flatten)]
        args: RtpcArgs,
    },

    /// Delete an RTPC asset (refused while other assets bind it)
    #[command(after_help = "Examples:
  am asset rtpc delete speed
  am asset rtpc delete speed --force
")]
    Delete {
        /// Name of the RTPC asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle RTPC commands by routing to the appropriate handler.
pub async fn handler(
    command: &RtpcCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        RtpcCommands::Create { name, args } => create_rtpc(name, args, input, output),
        RtpcCommands::List {} => list_rtpcs(output),
        RtpcCommands::Update { name, args } => update_rtpc(name, args, input, output),
        RtpcCommands::Delete { name, force } => delete_rtpc(name, *force, input, output),
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
    .with_suggestion(format!("Pass {flag} (see 'am asset rtpc create --help')"))
}

/// The directory RTPCs live in, plus the project's root and name.
fn rtpcs_dir() -> Result<(PathBuf, PathBuf, String)> {
    let current_dir = env::current_dir()?;
    let project = read_amproject_file(&current_dir)?;
    let sources_base = if project.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project.sources_dir)
    };
    let dir = sources_base.join(AssetType::Rtpc.directory_name());
    Ok((current_dir, dir, project.name))
}

fn project_context(current_dir: PathBuf, output: &dyn Output) -> Result<ProjectContext> {
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    Ok(ProjectContext::new(current_dir).with_validator(validator))
}

/// The non-interactive requirements of a new RTPC, as flag-named errors.
fn require_values(patch: RtpcPatch) -> Result<RtpcPatch> {
    if patch.min_value.is_none() {
        return Err(missing_flag_error("--min-value", "The minimum value").into());
    }
    if patch.max_value.is_none() {
        return Err(missing_flag_error("--max-value", "The maximum value").into());
    }
    Ok(patch)
}

fn to_values(name: &str, patch: RtpcPatch) -> Result<RtpcValues> {
    let (Some(min_value), Some(max_value)) = (patch.min_value, patch.max_value) else {
        anyhow::bail!("RTPC values are incomplete");
    };
    let fade = patch.fade;
    let fade_values = (!fade.is_empty()).then(|| fade_values(fade));
    Ok(RtpcValues {
        name: name.to_string(),
        min_value,
        max_value,
        default_value: patch.default_value,
        fade: fade_values,
    })
}

/// Fade flags on a new RTPC imply enabled fading unless `--fade-settings-enabled` says otherwise.
fn fade_values(fade: FadePatch) -> FadeValues {
    let fallback = FadeTransitionValues::default();
    FadeValues {
        enabled: fade.enabled.unwrap_or(true),
        attack: FadeTransitionValues {
            duration: fade.attack_duration.unwrap_or(fallback.duration),
            fader: fade.attack_fader.unwrap_or_else(|| fallback.fader.clone()),
        },
        release: FadeTransitionValues {
            duration: fade.release_duration.unwrap_or(fallback.duration),
            fader: fade.release_fader.unwrap_or(fallback.fader),
        },
    }
}

/// Create a new RTPC asset.
fn create_rtpc(name: &str, args: &RtpcArgs, input: &dyn Input, output: &dyn Output) -> Result<()> {
    let (current_dir, dir, project_name) = rtpcs_dir()?;
    output.progress(&format!(
        "Creating RTPC '{name}' in project '{project_name}'..."
    ));

    let file_path = dir.join(format!("{name}.json"));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset rtpc update {name}' to modify it, or choose a different name"
        ))
    };
    if file_path.exists() {
        return Err(already_exists().into());
    }

    let context = project_context(current_dir, output)?;
    if context.has_name(AssetType::Rtpc, name) {
        return Err(already_exists().into());
    }

    // Flags first; whatever is still missing is asked for, or reported by flag name.
    let patch = resolve_patch(args)?;
    let patch = if can_prompt(input, output) {
        complete_optional(input, complete_required(input, patch)?)?
    } else {
        require_values(patch)?
    };

    let rtpc = build_rtpc(to_values(name, patch)?, &context)?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
    let json_content =
        serde_json::to_string_pretty(&rtpc).context("Failed to serialize RTPC to JSON")?;
    atomic_write(&file_path, json_content.as_bytes())?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({
                "id": rtpc.id,
                "name": rtpc.name(),
                "path": file_path.to_string_lossy(),
                "min_value": rtpc.min_value,
                "max_value": rtpc.max_value,
                "default_value": rtpc.default_value,
            }),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "RTPC '{name}' created successfully at {}",
                file_path.display()
            )),
            None,
        ),
    }
    Ok(())
}

fn fade_enabled(rtpc: &Rtpc) -> bool {
    rtpc.fade_settings.as_ref().is_some_and(|f| f.enabled)
}

/// Parses an RTPC file's JSON. Hand-written files may leave out the numeric
/// fields, which FlatBuffers (and so the SDK) reads as 0.
fn parse_rtpc(raw: &serde_json::Value) -> serde_json::Result<Rtpc> {
    let mut value = raw.clone();
    if let Some(fields) = value.as_object_mut() {
        for key in ["min_value", "max_value", "default_value"] {
            fields.entry(key).or_insert_with(|| json!(0.0));
        }
    }
    serde_json::from_value(value)
}

fn read_rtpcs(dir: &std::path::Path, output: &dyn Output) -> Result<(Vec<Rtpc>, Vec<String>)> {
    let mut rtpcs = Vec::new();
    let mut warnings = Vec::new();
    for path in find_json_files_recursive(dir)? {
        let parsed = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
            .and_then(|raw| parse_rtpc(&raw).map_err(|e| e.to_string()));
        match parsed {
            Ok(rtpc) => rtpcs.push(rtpc),
            Err(reason) => {
                output.warning(&format!("Skipping invalid RTPC file: {}", path.display()));
                warnings.push(format!("Invalid RTPC file {}: {reason}", path.display()));
            }
        }
    }
    rtpcs.sort_by(|a, b| a.name().cmp(b.name()));
    Ok((rtpcs, warnings))
}

/// List all RTPC assets in the current project.
fn list_rtpcs(output: &dyn Output) -> Result<()> {
    let (_, dir, _) = rtpcs_dir()?;
    let (rtpcs, warnings) = read_rtpcs(&dir, output)?;
    let rows: Vec<_> = rtpcs
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "name": r.name(),
                "min_value": r.min_value,
                "max_value": r.max_value,
                "default_value": r.default_value,
                "fade_enabled": fade_enabled(r),
            })
        })
        .collect();

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"rtpcs": rows, "count": rtpcs.len(), "warnings": warnings}),
            None,
        ),
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {warning}", "Warning:".yellow()));
            }
            if rtpcs.is_empty() {
                output.progress("No RTPCs found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset rtpc create <name>".green()
                ));
                return Ok(());
            }
            output.table(None, json!(rows));
            output.progress("");
            output.progress(&format!("{} RTPC(s) found", rtpcs.len()));
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
        "Pass at least one flag, e.g. 'am asset rtpc update {name} --max-value 200'"
    ))
}

/// Finds the RTPC file; returns its path, the typed asset and the raw JSON.
fn locate(name: &str, dir: &std::path::Path) -> Result<(PathBuf, Rtpc, serde_json::Value)> {
    let path = dir.join(format!("{name}.json"));
    if !path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset rtpc list' to see available RTPCs, or 'am asset rtpc create {name}' to create it"
            ))
            .into());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read RTPC file: {}", path.display()))?;
    let raw = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse RTPC file: {}", path.display()))?;
    let rtpc = parse_rtpc(&raw)
        .with_context(|| format!("Failed to read RTPC file: {}", path.display()))?;
    Ok((path, rtpc, raw))
}

/// Update an existing RTPC asset.
fn update_rtpc(name: &str, args: &RtpcArgs, input: &dyn Input, output: &dyn Output) -> Result<()> {
    let (current_dir, dir, project_name) = rtpcs_dir()?;
    output.progress(&format!(
        "Updating RTPC '{name}' in project '{project_name}'..."
    ));
    let (file_path, existing, raw) = locate(name, &dir)?;

    let mut patch = resolve_patch(args)?;
    if patch.is_empty() {
        if !can_prompt(input, output) {
            return Err(no_changes_error(name).into());
        }
        patch = prompt_update_patch(input, &existing)?;
    }

    let context = project_context(current_dir, output)?;
    let rtpc = apply_patch(&existing, &patch);
    rtpc.validate_rules(&context)?;

    // Keys the CLI does not know about stay in the file.
    let merged = merge_into_original(&raw, &rtpc).context("Failed to serialize RTPC to JSON")?;
    let json_content =
        serde_json::to_string_pretty(&merged).context("Failed to serialize RTPC to JSON")?;
    atomic_write(&file_path, json_content.as_bytes())?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({
                "id": rtpc.id,
                "name": rtpc.name(),
                "path": file_path.to_string_lossy(),
                "updated_fields": patch.changed_fields(),
            }),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "RTPC '{name}' updated successfully at {}",
                file_path.display()
            )),
            None,
        ),
    }
    Ok(())
}

fn in_use_error(name: &str, users: &[(AssetType, AssetRef)]) -> CliError {
    let names = users
        .iter()
        .map(|(kind, asset)| format!("{kind} '{}'", asset.name))
        .collect::<Vec<_>>()
        .join(", ");
    CliError::new(
        codes::ERR_ASSET_IN_USE,
        format!("RTPC '{name}' is still referenced"),
        format!("It is bound by: {names}"),
    )
    .with_suggestion(
        "Replace the RTPC binding with a static value on those assets first (e.g. --gain 1), then delete the RTPC",
    )
}

/// Delete an RTPC asset.
fn delete_rtpc(name: &str, force: bool, input: &dyn Input, output: &dyn Output) -> Result<()> {
    let (current_dir, dir, _) = rtpcs_dir()?;
    let (file_path, rtpc, _) = locate(name, &dir)?;

    // Bound assets would be left with a dangling id, so --force does not override this.
    let validator = ProjectValidator::new(current_dir, output)?;
    let users = referrers_with_type(&validator, RefTarget::Asset(AssetType::Rtpc), rtpc.id);
    if !users.is_empty() {
        return Err(in_use_error(name, &users).into());
    }

    let confirmed = force
        || input
            .confirm(
                &format!("Delete RTPC '{name}'? This cannot be undone."),
                Some(false),
            )
            .map_err(|_| {
                CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Deletion requires confirmation",
                    "In non-interactive mode, use --force to confirm deletion",
                )
                .with_suggestion(format!(
                    "Use 'am asset rtpc delete {name} --force' to delete without prompting"
                ))
            })?;
    if !confirmed {
        output.progress("Deletion cancelled.");
        return Ok(());
    }

    fs::remove_file(&file_path)
        .with_context(|| format!("Failed to delete RTPC file: {}", file_path.display()))?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"id": rtpc.id, "name": rtpc.name(), "deleted": true}),
            None,
        ),
        OutputMode::Interactive => {
            output.success(json!(format!("RTPC '{name}' deleted successfully")), None)
        }
    }
    Ok(())
}
