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

//! Helpers shared by the `bus` and `bus-file` command handlers.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::assets::bus::{BusFile, BusFileScan};
use crate::assets::{AssetType, ProjectValidator};
use crate::common::errors::{CliError, codes};
use crate::common::files::atomic_write;
use crate::common::utils::read_amproject_file;
use crate::forms::references::{RefTarget, referrers_with_type};
use crate::input::{Input, select_index};
use crate::presentation::{Output, OutputMode};

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
pub fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

/// The project root (the current directory), after checking it holds a project.
pub fn project_root() -> Result<PathBuf> {
    let root = env::current_dir()?;
    read_amproject_file(&root)?;
    Ok(root)
}

/// Writes `value` as pretty JSON, atomically.
pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value).context("Failed to serialize bus file")?;
    atomic_write(path, text.as_bytes())
}

/// Error for a flag that cannot be prompted for in this session.
pub fn missing_flag_error(flag: &str, what: &str, command: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Missing required {flag}"),
        format!("{what} is required and cannot be prompted for in non-interactive or JSON mode"),
    )
    .with_suggestion(format!("Pass {flag} (see '{command} --help')"))
}

/// Asks for confirmation, or demands `--force` where the session cannot ask.
pub fn confirm_deletion(
    input: &dyn Input,
    prompt: &str,
    force: bool,
    force_command: &str,
) -> Result<bool> {
    if force {
        return Ok(true);
    }
    input.confirm(prompt, Some(false)).map_err(|_| {
        CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Deletion requires confirmation",
            "In non-interactive mode, use --force to confirm deletion",
        )
        .with_suggestion(format!(
            "Use '{force_command} --force' to delete without prompting"
        ))
        .into()
    })
}

/// A `ProjectValidator` for the project at `root`.
pub fn validator(root: &Path, output: &dyn Output) -> Result<ProjectValidator> {
    ProjectValidator::new(root.to_path_buf(), output)
}

/// `"sound 'boom', collection 'x'"`-style list of assets referencing bus `id`.
pub fn bus_referrers(validator: &ProjectValidator, id: u64) -> Vec<String> {
    referrers_with_type(validator, RefTarget::Bus, id)
        .into_iter()
        .map(|(kind, asset): (AssetType, _)| format!("{kind} '{}'", asset.name))
        .collect()
}

/// Whether the bus `id` is defined in a file of `scan` other than `except`.
pub fn defined_elsewhere(scan: &BusFileScan, except: &BusFile, id: u64) -> bool {
    scan.files
        .iter()
        .filter(|f| f.name != except.name)
        .any(|f| {
            f.buses()
                .iter()
                .any(|b| b.get("id").and_then(Value::as_u64) == Some(id))
        })
}

/// Selects a file by name, or picks one when the session can prompt.
pub fn select_file_name(input: &dyn Input, files: &[BusFile]) -> Result<String> {
    let names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
    let index = select_index(input, "Bus file", &names)?;
    Ok(names[index].clone())
}
