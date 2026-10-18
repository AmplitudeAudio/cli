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

//! Bus file commands: `am asset bus-file create|list|delete`.
//!
//! A bus file (`<name>.buses.json` in the project root) holds one complete bus
//! tree. A project may keep several; the engine configuration picks one.

use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::{Value, json};

use super::bus_shared::{
    bus_referrers, confirm_deletion, defined_elsewhere, project_root, validator, write_json,
};
use crate::assets::bus::{
    BusFile, BusFileScan, bus_file_path, normalize_file_name, scan_bus_files,
};
use crate::assets::engine_config::bus_file_referrers;
use crate::common::errors::{CliError, asset_already_exists, asset_not_found, codes};
use crate::database::Database;
use crate::input::Input;
use crate::presentation::{Output, OutputMode};
use crate::services::bus::new_bus_file;

const ASSET_NAME: &str = "Bus file";

/// Bus file subcommands.
#[derive(Subcommand, Debug)]
pub enum BusFileCommands {
    /// Create a bus file holding only the `master` bus (id 1)
    #[command(after_help = "Examples:
  am asset bus-file create main
  am asset bus-file create cutscenes
")]
    Create {
        /// File name, without '.buses.json'
        file_name: String,
    },

    /// List the bus files of the project with their bus counts
    #[command(after_help = "Examples:
  am asset bus-file list
  am asset bus-file list --json
")]
    List {},

    /// Delete a bus file (refused for the last file, while an engine config loads it, or while its buses are used)
    #[command(after_help = "Examples:
  am asset bus-file delete cutscenes
  am asset bus-file delete cutscenes --force
")]
    Delete {
        /// File name, without '.buses.json'
        file_name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle bus file commands by routing to the appropriate handler.
pub async fn handler(
    command: &BusFileCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        BusFileCommands::Create { file_name } => create_file(file_name, output),
        BusFileCommands::List {} => list_files(output),
        BusFileCommands::Delete { file_name, force } => {
            delete_file(file_name, *force, input, output)
        }
    }
}

fn create_file(file_name: &str, output: &dyn Output) -> Result<()> {
    let root = project_root()?;
    let name = normalize_file_name(file_name)?;
    let path = bus_file_path(&root, &name);
    if path.exists() {
        return Err(asset_already_exists(ASSET_NAME, &name)
            .with_suggestion(
                "Choose a different file name, or use 'am asset bus create' to add buses to it",
            )
            .into());
    }
    write_json(&path, &new_bus_file())?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"file": name, "path": path.to_string_lossy(), "buses": 1}),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Bus file '{name}' created at {} with the 'master' bus",
                path.display()
            )),
            None,
        ),
    }
    Ok(())
}

fn list_files(output: &dyn Output) -> Result<()> {
    let root = project_root()?;
    let scan = scan_bus_files(&root)?;
    let rows: Vec<Value> = scan
        .files
        .iter()
        .map(|f| {
            json!({
                "file": f.name,
                "path": f.path.to_string_lossy(),
                "buses": f.buses().len(),
            })
        })
        .collect();

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"files": rows, "count": rows.len(), "warnings": scan.problems}),
            None,
        ),
        OutputMode::Interactive => {
            for problem in &scan.problems {
                output.progress(&format!("{} {problem}", "Warning:".yellow()));
            }
            if rows.is_empty() {
                output.progress("No bus files found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset bus-file create <file-name>".green()
                ));
                return Ok(());
            }
            output.table(None, json!(rows));
            output.progress("");
            output.progress(&format!("{} bus file(s) found", rows.len()));
        }
    }
    Ok(())
}

/// Buses defined only in `file` that assets still reference, as `(bus, referrers)`.
fn used_buses(
    scan: &BusFileScan,
    file: &BusFile,
    output: &dyn Output,
    root: &std::path::Path,
) -> Result<Vec<String>> {
    let validator = validator(root, output)?;
    let mut used = Vec::new();
    for bus in file.buses() {
        let (Some(id), Some(name)) = (
            bus.get("id").and_then(Value::as_u64),
            bus.get("name").and_then(Value::as_str),
        ) else {
            continue;
        };
        if defined_elsewhere(scan, file, id) {
            continue;
        }
        let users = bus_referrers(&validator, id);
        if !users.is_empty() {
            used.push(format!("bus '{name}' is used by {}", users.join(", ")));
        }
    }
    Ok(used)
}

fn delete_file(file_name: &str, force: bool, input: &dyn Input, output: &dyn Output) -> Result<()> {
    let root = project_root()?;
    let name = normalize_file_name(file_name)?;
    let scan = scan_bus_files(&root)?;
    let Some(file) = scan.files.iter().find(|f| f.name == name) else {
        return Err(asset_not_found(ASSET_NAME, &name)
            .with_suggestion("Use 'am asset bus-file list' to see the bus files of the project")
            .into());
    };

    if scan.files.len() <= 1 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Cannot delete '{name}': it is the last bus file of the project"),
            "The SDK engine needs a bus file to load; a project must keep at least one",
        )
        .with_suggestion("Create another file first with 'am asset bus-file create <file-name>'")
        .into());
    }

    // An engine config names the bus file it loads; deleting it would leave the config
    // dangling, so --force does not override this.
    let configs = bus_file_referrers(&root, &name)?;
    if !configs.is_empty() {
        return Err(CliError::new(
            codes::ERR_ASSET_IN_USE,
            format!("Bus file '{name}' is still in use"),
            format!("It is the buses file of: {}", configs.join(", ")),
        )
        .with_suggestion(
            "Point those engine configs at another bus file first (am asset engine-config update <file> --buses-file <name>), then delete this one",
        )
        .into());
    }

    let used = used_buses(&scan, file, output, &root)?;
    if !used.is_empty() {
        return Err(CliError::new(
            codes::ERR_ASSET_IN_USE,
            format!("Bus file '{name}' is still in use"),
            used.join("; "),
        )
        .with_suggestion(
            "Point those assets at another bus (e.g. 'am asset sound update <name> --bus master') first, then delete the file",
        )
        .into());
    }

    let confirmed = confirm_deletion(
        input,
        &format!("Delete bus file '{name}'? This cannot be undone."),
        force,
        &format!("am asset bus-file delete {name}"),
    )?;
    if !confirmed {
        output.progress("Deletion cancelled.");
        return Ok(());
    }

    fs::remove_file(&file.path)
        .with_context(|| format!("Failed to delete bus file: {}", file.path.display()))?;

    match output.mode() {
        OutputMode::Json => output.success(json!({"file": name, "deleted": true}), None),
        OutputMode::Interactive => output.success(
            json!(format!("Bus file '{name}' deleted successfully")),
            None,
        ),
    }
    Ok(())
}
