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

//! Bus commands: `am asset bus create|list|update|delete`.
//!
//! Buses are entries inside `*.buses.json` files of the project root. The
//! handlers only gather values (flags, then prompts when the session is
//! interactive); building and merging lives in `services::bus`, and every
//! change rewrites the whole file atomically after it was validated.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde_json::{Value, json};

use super::bus_args::{BusArgs, DUCK_SYNTAX, resolve_patch};
use super::bus_prompts::{complete_create, prompt_update_patch};
use super::bus_shared::{
    bus_referrers, can_prompt, confirm_deletion, missing_flag_error, project_root,
    select_file_name, validator, write_json,
};
use crate::assets::ProjectContext;
use crate::assets::bus::{BusFile, BusFileScan, normalize_file_name, scan_bus_files};
use crate::common::errors::{CliError, asset_already_exists, asset_not_found, codes};
use crate::database::Database;
use crate::input::Input;
use crate::presentation::{Output, OutputMode};
use crate::services::bus::{
    BusPatch, BusValues, add_bus, find_bus, remove_bus, summarize, update_bus,
};

const ASSET_NAME: &str = "Bus";

/// Bus subcommands.
#[derive(Subcommand, Debug)]
pub enum BusCommands {
    /// Create a bus in a bus file
    #[command(after_help = "Examples:
  am asset bus create sfx
  am asset bus create weapons --parent sfx --gain 0.8 --fader Ease
  am asset bus create voices --duck music:0.25:1:2
  am asset bus create ui --file cutscenes

--duck takes <bus>:<target_gain>:<fade_in>:<fade_out>[:<fade_in_fader>[:<fade_out_fader>]]:
the gain the ducked bus fades to while this bus plays, the fade durations in
seconds, and optional faders (default Linear). Repeat --duck for several buses.
Parent and duck buses must be in the same file: the SDK loads one file as one tree.
")]
    Create {
        /// Name of the bus (unique across all bus files)
        name: String,

        #[command(flatten)]
        args: BusArgs,
    },

    /// List the buses of one bus file, or of every file
    #[command(after_help = "Examples:
  am asset bus list
  am asset bus list --file main --json
")]
    List {
        /// Only list this bus file (name without '.buses.json')
        #[arg(long)]
        file: Option<String>,
    },

    /// Update a bus (only the flags you pass change)
    #[command(after_help = "Examples:
  am asset bus update sfx --gain 0.5
  am asset bus update weapons --parent music
  am asset bus update voices --duck music:0.25:1:2 --duck sfx:0.5:0.5:1
  am asset bus update voices --clear-ducks

--duck replaces all duck buses of the bus. Re-parenting updates the child lists
of both parents.
")]
    Update {
        /// Name of the bus to update
        name: String,

        #[command(flatten)]
        args: BusArgs,

        /// Remove every duck bus of the bus
        #[arg(long, conflicts_with = "duck")]
        clear_ducks: bool,
    },

    /// Delete a bus (refused for master, buses with children, and buses in use)
    #[command(after_help = "Examples:
  am asset bus delete sfx
  am asset bus delete sfx --file main --force
")]
    Delete {
        /// Name of the bus to delete
        name: String,

        /// Bus file holding the bus (required when the project has several)
        #[arg(long)]
        file: Option<String>,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle bus commands by routing to the appropriate handler.
pub async fn handler(
    command: &BusCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        BusCommands::Create { name, args } => create_bus(name, args, input, output),
        BusCommands::List { file } => list_buses(file.as_deref(), output),
        BusCommands::Update {
            name,
            args,
            clear_ducks,
        } => update_existing(name, args, *clear_ducks, input, output),
        BusCommands::Delete { name, file, force } => {
            delete_bus(name, file.as_deref(), *force, input, output)
        }
    }
}

fn read_scan(root: &Path) -> Result<BusFileScan> {
    scan_bus_files(root)
}

/// Picks the bus file a command works on.
///
/// An explicit `--file` wins; a single file is the default; with several files
/// the user is asked, or the missing flag is reported.
fn resolve_file<'a>(
    scan: &'a BusFileScan,
    requested: Option<&str>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<&'a BusFile> {
    if let Some(requested) = requested {
        let name = normalize_file_name(requested)?;
        return scan.files.iter().find(|f| f.name == name).ok_or_else(|| {
            asset_not_found("Bus file", &name)
                .with_suggestion("Use 'am asset bus-file list' to see the bus files, or 'am asset bus-file create <file-name>'")
                .into()
        });
    }
    match scan.files.as_slice() {
        [] => Err(CliError::new(
            codes::ERR_ASSET_NOT_FOUND,
            "The project has no bus file",
            "Buses live in '<name>.buses.json' files of the project root",
        )
        .with_suggestion("Create one first with 'am asset bus-file create main'")
        .into()),
        [only] => Ok(only),
        files if can_prompt(input, output) => {
            let name = select_file_name(input, files)?;
            Ok(files.iter().find(|f| f.name == name).unwrap_or(&files[0]))
        }
        _ => Err(missing_flag_error(
            "--file",
            "The bus file (the project has several)",
            "am asset bus create",
        )
        .into()),
    }
}

/// Ids used by assets, and by buses of any file.
fn taken_ids(context: &ProjectContext, scan: &BusFileScan) -> impl Fn(u64) -> bool {
    let bus_ids: std::collections::HashSet<u64> = scan
        .files
        .iter()
        .flat_map(|f| f.buses().iter())
        .filter_map(|b| b.get("id").and_then(Value::as_u64))
        .collect();
    let asset_ids = context.id_registry.clone();
    move |id| bus_ids.contains(&id) || asset_ids.contains(&id)
}

fn name_used_elsewhere(scan: &BusFileScan, current: &BusFile, name: &str) -> Option<String> {
    scan.files
        .iter()
        .filter(|f| f.name != current.name)
        .find(|f| find_bus(&f.value, name).is_some())
        .map(|f| f.name.clone())
}

fn to_values(name: &str, patch: BusPatch) -> BusValues {
    let defaults = BusValues::with_defaults(name);
    BusValues {
        gain: patch.gain.unwrap_or(defaults.gain),
        parent: patch.parent.unwrap_or(defaults.parent),
        fader: patch.fader.unwrap_or(defaults.fader),
        ducks: patch.ducks.unwrap_or(defaults.ducks),
        ..defaults
    }
}

fn create_bus(name: &str, args: &BusArgs, input: &dyn Input, output: &dyn Output) -> Result<()> {
    let root = project_root()?;
    output.progress(&format!("Creating bus '{name}'..."));
    let scan = read_scan(&root)?;
    let file = resolve_file(&scan, args.file.as_deref(), input, output)?;

    crate::services::bus::validate_bus_name(name)?;
    if let Some(other) = name_used_elsewhere(&scan, file, name) {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Bus '{name}' is already defined in '{other}'; bus names are unique across bus files"
            ))
            .into());
    }
    if find_bus(&file.value, name).is_some() {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset bus update {name}' to modify it, or choose a different name"
            ))
            .into());
    }

    let patch = resolve_patch(args)?;
    let values = if can_prompt(input, output) {
        complete_create(input, name, patch, &file.value)?
    } else {
        to_values(name, patch)
    };

    let context = ProjectContext::new(root.clone()).with_validator(validator(&root, output)?);
    let is_taken = taken_ids(&context, &scan);
    let (updated, id) = add_bus(&file.value, &values, &is_taken)?;
    write_json(&file.path, &updated)?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({
                "id": id,
                "name": name,
                "file": file.name,
                "path": file.path.to_string_lossy(),
                "parent": values.parent,
                "gain": values.gain,
                "fader": values.fader,
                "duck_buses": values.ducks.len(),
            }),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Bus '{name}' created successfully in {}",
                file.path.display()
            )),
            None,
        ),
    }
    Ok(())
}

fn list_buses(requested: Option<&str>, output: &dyn Output) -> Result<()> {
    let root = project_root()?;
    let scan = read_scan(&root)?;
    let files: Vec<&BusFile> = match requested {
        Some(requested) => {
            let name = normalize_file_name(requested)?;
            let Some(file) = scan.files.iter().find(|f| f.name == name) else {
                return Err(asset_not_found("Bus file", &name)
                    .with_suggestion("Use 'am asset bus-file list' to see the bus files")
                    .into());
            };
            vec![file]
        }
        None => scan.files.iter().collect(),
    };

    let rows: Vec<Value> = files
        .iter()
        .flat_map(|f| {
            summarize(&f.value).into_iter().map(|s| {
                json!({
                    "file": f.name,
                    "id": s.id,
                    "name": s.name,
                    "gain": s.gain,
                    "fader": s.fader,
                    "parent": s.parent,
                    "children": s.children,
                    "duck_buses": s.ducks,
                })
            })
        })
        .collect();

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"buses": rows, "count": rows.len(), "warnings": scan.problems}),
            None,
        ),
        OutputMode::Interactive => {
            for problem in &scan.problems {
                output.progress(&format!("{} {problem}", "Warning:".yellow()));
            }
            if rows.is_empty() {
                output.progress("No buses found in this project.");
                output.progress(&format!(
                    "Use '{}' to add a bus file.",
                    "am asset bus-file create main".green()
                ));
                return Ok(());
            }
            output.table(None, json!(rows));
            output.progress("");
            output.progress(&format!("{} bus(es) found", rows.len()));
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
        "Pass at least one flag, e.g. 'am asset bus update {name} --gain 0.5' (--duck takes {DUCK_SYNTAX})"
    ))
}

fn require_bus<'a>(file: &'a BusFile, name: &str) -> Result<&'a Value> {
    find_bus(&file.value, name).ok_or_else(|| {
        asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset bus list' to see available buses, or 'am asset bus create {name}' to create it"
            ))
            .into()
    })
}

fn update_existing(
    name: &str,
    args: &BusArgs,
    clear_ducks: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let root = project_root()?;
    output.progress(&format!("Updating bus '{name}'..."));
    let scan = read_scan(&root)?;
    let file = resolve_file(&scan, args.file.as_deref(), input, output)?;
    let bus = require_bus(file, name)?;
    let id = bus.get("id").and_then(Value::as_u64).unwrap_or(0);

    let mut patch = resolve_patch(args)?;
    if clear_ducks {
        patch.ducks = Some(Vec::new());
    }
    if patch.is_empty() {
        if !can_prompt(input, output) {
            return Err(no_changes_error(name).into());
        }
        patch = prompt_update_patch(input, name, &file.value)?;
        if patch.is_empty() {
            return Err(no_changes_error(name).into());
        }
    }

    let updated = update_bus(&file.value, name, &patch)?;
    write_json(&file.path, &updated)?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({
                "id": id,
                "name": name,
                "file": file.name,
                "path": file.path.to_string_lossy(),
                "updated_fields": patch.changed_fields(),
            }),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Bus '{name}' updated successfully in {}",
                file.path.display()
            )),
            None,
        ),
    }
    Ok(())
}

fn in_use_error(name: &str, users: &[String]) -> CliError {
    CliError::new(
        codes::ERR_ASSET_IN_USE,
        format!("Bus '{name}' is still referenced"),
        format!("It is used by: {}", users.join(", ")),
    )
    .with_suggestion(
        "Point those assets at another bus first (e.g. 'am asset sound update <name> --bus master'), then delete the bus",
    )
}

fn delete_bus(
    name: &str,
    requested: Option<&str>,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let root = project_root()?;
    let scan = read_scan(&root)?;
    let file = resolve_file(&scan, requested, input, output)?;
    let bus = require_bus(file, name)?;
    let id = bus.get("id").and_then(Value::as_u64).unwrap_or(0);

    // Refuses master, buses with children and buses other buses duck.
    let updated = remove_bus(&file.value, name)?;

    // Assets bound to the bus would be left with a dangling id, so --force does not override this.
    let users = bus_referrers(&validator(&root, output)?, id);
    if !users.is_empty() {
        return Err(in_use_error(name, &users).into());
    }

    let confirmed = confirm_deletion(
        input,
        &format!("Delete bus '{name}'? This cannot be undone."),
        force,
        &format!("am asset bus delete {name}"),
    )?;
    if !confirmed {
        output.progress("Deletion cancelled.");
        return Ok(());
    }
    write_json(&file.path, &updated)?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"id": id, "name": name, "file": file.name, "deleted": true}),
            None,
        ),
        OutputMode::Interactive => {
            output.success(json!(format!("Bus '{name}' deleted successfully")), None)
        }
    }
    Ok(())
}
