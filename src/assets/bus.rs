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

//! Bus files: `<name>.buses.json` in the project root.
//!
//! Buses are not one-file-per-asset. Each file holds a complete bus tree
//! (`BusDefinitionList`) and the engine configuration names exactly one such
//! file (`buses_file`), so a project may keep several alternative trees.
//!
//! [`validate_bus_file`] mirrors what the SDK needs to load a file
//! (`Engine::Initialize` and `BusInternalState.cpp`); the SDK fails to load or
//! crashes on every violation reported here.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::assets::ValidationError;
use crate::assets::extensions::FADER_ALGORITHM_NAMES;
use crate::common::errors::{CliError, codes};

/// ID of the root bus (`kAmMasterBusId` in the SDK).
pub const MASTER_BUS_ID: u64 = 1;
/// Name of the root bus; the SDK falls back to it when no bus has id 1.
pub const MASTER_BUS_NAME: &str = "master";
/// Suffix of bus file names.
pub const BUS_FILE_SUFFIX: &str = ".buses.json";
/// Fader used when none is given (a registered SDK fader).
pub const DEFAULT_FADER: &str = "Linear";

/// A bus file read from disk.
#[derive(Debug, Clone)]
pub struct BusFile {
    /// File name without the `.buses.json` suffix.
    pub name: String,
    pub path: PathBuf,
    pub value: Value,
}

impl BusFile {
    /// The bus entries of the file (empty when `buses` is missing).
    pub fn buses(&self) -> &[Value] {
        buses_of(&self.value)
    }
}

/// Every readable bus file of a project, plus a note per unreadable one.
#[derive(Debug, Clone, Default)]
pub struct BusFileScan {
    pub files: Vec<BusFile>,
    pub problems: Vec<String>,
}

/// The bus entries of a parsed file (empty when `buses` is missing or not an array).
pub fn buses_of(file: &Value) -> &[Value] {
    file.get("buses")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Path of the bus file called `name` in `root`.
pub fn bus_file_path(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{name}{BUS_FILE_SUFFIX}"))
}

/// Reads every `*.buses.json` file of the project root, sorted by name.
pub fn scan_bus_files(root: &Path) -> Result<BusFileScan> {
    let mut scan = BusFileScan::default();
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
            .and_then(|n| n.strip_suffix(BUS_FILE_SUFFIX))
            .map(str::to_string)
        else {
            continue;
        };
        match fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| e.to_string()))
        {
            Ok(value) => scan.files.push(BusFile { name, path, value }),
            Err(reason) => scan
                .problems
                .push(format!("Invalid bus file {}: {reason}", path.display())),
        }
    }
    Ok(scan)
}

/// Checks a bus file name and strips an optional `.buses.json` suffix.
pub fn normalize_file_name(input: &str) -> Result<String, CliError> {
    let name = input.trim();
    let name = name.strip_suffix(BUS_FILE_SUFFIX).unwrap_or(name);
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
        format!("Invalid bus file name '{input}'"),
        "A bus file name may only contain letters, digits, underscores, hyphens and dots, and cannot be empty",
    )
    .with_suggestion("Use a plain name such as 'main' (the file becomes 'main.buses.json')"))
}

fn violation(what: String) -> ValidationError {
    ValidationError::type_rule_violation(
        what,
        "The Amplitude SDK cannot load this bus file when this rule is broken",
    )
    .with_suggestion("Fix the bus file (see 'am asset bus --help') before changing it")
}

fn label(bus: &Value, index: usize) -> String {
    match bus.get("name").and_then(Value::as_str) {
        Some(name) if !name.is_empty() => format!("bus '{name}'"),
        _ => format!("bus #{index}"),
    }
}

/// A bus `fader` is dereferenced by the SDK but only looked up by name
/// (`Fader::Find`), so any non-empty name is accepted; duck faders must be
/// registered because the SDK fails to load otherwise.
fn check_fader(
    owner: &str,
    field: &str,
    value: Option<&Value>,
    registered_only: bool,
) -> Result<(), ValidationError> {
    match value.and_then(Value::as_str) {
        Some(f) if !f.is_empty() && (!registered_only || FADER_ALGORITHM_NAMES.contains(&f)) => {
            Ok(())
        }
        Some(f) if !f.is_empty() => Err(violation(format!(
            "{owner}: unknown {field} '{f}' (one of: {})",
            FADER_ALGORITHM_NAMES.join(", ")
        ))),
        _ => Err(violation(format!(
            "{owner}: {field} is missing or empty (the SDK reads it without a null check)"
        ))),
    }
}

fn check_non_negative(owner: &str, field: &str, value: &Value) -> Result<(), ValidationError> {
    match value.as_f64() {
        Some(v) if v.is_finite() && v >= 0.0 => Ok(()),
        _ => Err(violation(format!(
            "{owner}: {field} must be a number that is 0 or more"
        ))),
    }
}

fn id_of(bus: &Value) -> Option<u64> {
    bus.get("id").and_then(Value::as_u64)
}

fn check_bus_fields(bus: &Value, index: usize) -> Result<(), ValidationError> {
    let who = label(bus, index);
    if !bus.is_object() {
        return Err(violation(format!("{who} is not an object")));
    }
    if id_of(bus).is_none_or(|id| id == 0) {
        return Err(violation(format!("{who}: id must be a positive integer")));
    }
    if bus
        .get("name")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(violation(format!("{who}: name is missing or empty")));
    }
    if let Some(gain) = bus.get("gain") {
        check_non_negative(&who, "gain", gain)?;
    }
    check_fader(&who, "fader", bus.get("fader"), false)?;
    if let Some(children) = bus.get("child_buses")
        && children
            .as_array()
            .is_none_or(|a| a.iter().any(|c| c.as_u64().is_none()))
    {
        return Err(violation(format!(
            "{who}: child_buses must be a list of bus ids"
        )));
    }
    Ok(())
}

fn check_duck(
    who: &str,
    duck: &Value,
    ids: &HashSet<u64>,
    own_id: u64,
) -> Result<(), ValidationError> {
    let Some(target) = duck.get("id").and_then(Value::as_u64).filter(|id| *id != 0) else {
        return Err(violation(format!("{who}: a duck bus has no valid id")));
    };
    if target == own_id {
        return Err(violation(format!("{who} cannot duck itself")));
    }
    if !ids.contains(&target) {
        return Err(violation(format!(
            "{who}: duck bus id {target} is not defined in this file"
        )));
    }
    if let Some(gain) = duck.get("target_gain") {
        check_non_negative(who, "duck target_gain", gain)?;
    }
    for side in ["fade_in", "fade_out"] {
        let owner = format!("{who}: duck bus {target} {side}");
        let Some(fade) = duck.get(side).filter(|f| f.is_object()) else {
            return Err(violation(format!(
                "{owner} is missing (the SDK reads it without a null check)"
            )));
        };
        if let Some(duration) = fade.get("duration") {
            check_non_negative(&owner, "duration", duration)?;
        }
        check_fader(&owner, "fader", fade.get("fader"), true)?;
    }
    Ok(())
}

fn check_unique(buses: &[Value]) -> Result<(), ValidationError> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for bus in buses {
        let id = id_of(bus).unwrap_or(0);
        if !ids.insert(id) {
            return Err(violation(format!("Duplicate bus id {id} in the file")));
        }
        let name = bus.get("name").and_then(Value::as_str).unwrap_or_default();
        if !names.insert(name) {
            return Err(violation(format!(
                "Duplicate bus name '{name}' in the file"
            )));
        }
    }
    Ok(())
}

/// Id of the root bus: the bus with id 1, else the bus named `master`
/// (the lookup order of `Engine::Initialize`).
pub fn master_id(buses: &[Value]) -> Option<u64> {
    buses
        .iter()
        .find(|b| id_of(b) == Some(MASTER_BUS_ID))
        .or_else(|| {
            buses
                .iter()
                .find(|b| b.get("name").and_then(Value::as_str) == Some(MASTER_BUS_NAME))
        })
        .and_then(id_of)
}

fn check_master(buses: &[Value]) -> Result<(), ValidationError> {
    if master_id(buses).is_some() {
        return Ok(());
    }
    Err(violation(format!(
        "The file has no root bus: a bus with id {MASTER_BUS_ID} or named '{MASTER_BUS_NAME}' must exist"
    )))
}

fn check_tree(buses: &[Value]) -> Result<(), ValidationError> {
    let ids: HashSet<u64> = buses.iter().filter_map(id_of).collect();
    let names: HashMap<u64, &str> = buses
        .iter()
        .filter_map(|b| Some((id_of(b)?, b.get("name")?.as_str()?)))
        .collect();
    let name_of = |id: u64| names.get(&id).copied().unwrap_or("?");
    let root = master_id(buses);

    let mut parent_of: HashMap<u64, u64> = HashMap::new();
    for bus in buses {
        let own = id_of(bus).unwrap_or(0);
        let who = format!("bus '{}'", name_of(own));
        let children = bus.get("child_buses").and_then(Value::as_array);
        for child in children.into_iter().flatten().filter_map(Value::as_u64) {
            if child == own {
                return Err(violation(format!("{who} lists itself as a child")));
            }
            if Some(child) == root {
                return Err(violation(format!(
                    "{who} lists the '{MASTER_BUS_NAME}' bus as a child; the root has no parent"
                )));
            }
            if !ids.contains(&child) {
                return Err(violation(format!(
                    "{who} lists child bus id {child}, which is not defined in this file"
                )));
            }
            if parent_of.insert(child, own).is_some() {
                return Err(violation(format!(
                    "bus '{}' has more than one parent",
                    name_of(child)
                )));
            }
        }
        let ducks = bus.get("duck_buses").and_then(Value::as_array);
        for duck in ducks.into_iter().flatten() {
            check_duck(&who, duck, &ids, own)?;
        }
    }

    // With at most one parent per bus, following parents from any bus either
    // reaches a root or loops back on itself.
    for start in ids.iter().copied() {
        let mut seen = HashSet::from([start]);
        let mut current = start;
        while let Some(parent) = parent_of.get(&current).copied() {
            if !seen.insert(parent) {
                return Err(violation(format!(
                    "The bus hierarchy contains a cycle through bus '{}'",
                    name_of(parent)
                )));
            }
            current = parent;
        }
    }
    Ok(())
}

/// Checks that the SDK can load `file` (the parsed content of a `*.buses.json`).
///
/// Unknown keys are ignored. Rules, each backed by the SDK source:
/// - `buses` exists (`busDefList->buses()->size()` is not null-checked);
/// - every bus has a positive id, a name and a registered `fader` (dereferenced
///   in `BusInternalState::Initialize`);
/// - ids and names are unique inside the file (lookups return the first match);
/// - a bus named `master` with id 1 exists (`kAmMasterBusId`);
/// - child ids refer to buses of the same file, each bus has at most one
///   parent and the tree has no cycle (`PopulateChildBuses`, recursive
///   `AdvanceFrame`);
/// - duck buses refer to buses of the same file and carry both fade blocks with
///   registered faders (`DuckBusInternalState::Initialize`).
pub fn validate_bus_file(file: &Value) -> Result<(), ValidationError> {
    let Some(buses) = file.get("buses").and_then(Value::as_array) else {
        return Err(violation("The file has no 'buses' array".to_string()));
    };
    for (index, bus) in buses.iter().enumerate() {
        check_bus_fields(bus, index)?;
    }
    check_unique(buses)?;
    check_master(buses)?;
    check_tree(buses)
}
