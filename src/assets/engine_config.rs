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

//! Engine configs: `<name>.config.json` files in the project root.
//!
//! A project may keep several (one per platform, say). They name the pipeline
//! and the bus file the engine loads by file name, so this module also answers
//! "which configs use pipeline X / bus file Y" and "which bus file is active".

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use super::engine_config_fields::{ENGINE_CONFIG_SUFFIX, buses_ref_name, pipeline_ref_name};
use crate::common::errors::{CliError, codes};

pub use super::engine_config_rules::validate_engine_config;

/// An engine config read from disk.
#[derive(Debug, Clone)]
pub struct EngineConfigFile {
    /// File name without the `.config.json` suffix.
    pub name: String,
    pub path: PathBuf,
    pub value: Value,
}

impl EngineConfigFile {
    /// The `mixer.pipeline` value, if any.
    pub fn pipeline(&self) -> Option<&str> {
        self.value.get("mixer")?.get("pipeline")?.as_str()
    }

    /// The `buses_file` value, if any.
    pub fn buses_file(&self) -> Option<&str> {
        self.value.get("buses_file")?.as_str()
    }
}

/// Every readable engine config of a project, plus a note per unreadable one.
#[derive(Debug, Clone, Default)]
pub struct EngineConfigScan {
    pub files: Vec<EngineConfigFile>,
    pub problems: Vec<String>,
}

/// Path of the engine config called `name` in `root`.
pub fn engine_config_path(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{name}{ENGINE_CONFIG_SUFFIX}"))
}

/// Reads every `*.config.json` file of the project root, sorted by name.
pub fn scan_engine_configs(root: &Path) -> Result<EngineConfigScan> {
    let mut scan = EngineConfigScan::default();
    let entries =
        fs::read_dir(root).with_context(|| format!("Failed to read {}", root.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    paths.sort();

    for path in paths {
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(ENGINE_CONFIG_SUFFIX))
            .map(str::to_string)
        else {
            continue;
        };
        match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| e.to_string()))
        {
            Ok(value) => scan.files.push(EngineConfigFile { name, path, value }),
            Err(reason) => scan.problems.push(format!(
                "Invalid engine config {}: {reason}",
                path.display()
            )),
        }
    }
    Ok(scan)
}

/// Checks an engine config file name and strips an optional `.config.json` suffix.
pub fn normalize_file_name(input: &str) -> Result<String, CliError> {
    let name = input.trim();
    let name = name.strip_suffix(ENGINE_CONFIG_SUFFIX).unwrap_or(name);
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if valid {
        return Ok(name.to_string());
    }
    Err(CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid engine config name '{input}'"),
        "An engine config name may only contain letters, digits, underscores, hyphens and dots, and cannot be empty",
    )
    .with_suggestion("Use a plain name such as 'pc' (the file becomes 'pc.config.json')"))
}

/// Names of the configs of `root` whose `mixer.pipeline` is the file of pipeline `name`.
pub fn pipeline_referrers(root: &Path, name: &str) -> Result<Vec<String>> {
    Ok(scan_engine_configs(root)?
        .files
        .iter()
        .filter(|c| c.pipeline().map(pipeline_ref_name) == Some(name))
        .map(|c| format!("{}{ENGINE_CONFIG_SUFFIX}", c.name))
        .collect())
}

/// Names of the configs of `root` whose `buses_file` is the bus file `name`.
pub fn bus_file_referrers(root: &Path, name: &str) -> Result<Vec<String>> {
    Ok(scan_engine_configs(root)?
        .files
        .iter()
        .filter(|c| c.buses_file().map(buses_ref_name) == Some(name))
        .map(|c| format!("{}{ENGINE_CONFIG_SUFFIX}", c.name))
        .collect())
}

/// The bus file every engine config agrees on, if there is exactly one.
///
/// With no config, or configs pointing at different files, no file is active.
pub fn active_bus_file(configs: &[EngineConfigFile]) -> Option<String> {
    let files: BTreeSet<&str> = configs
        .iter()
        .filter_map(EngineConfigFile::buses_file)
        .map(buses_ref_name)
        .collect();
    match files.len() {
        1 => files.into_iter().next().map(str::to_string),
        _ => None,
    }
}
