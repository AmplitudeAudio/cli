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

//! Command-line flags of `switch create` / `switch update` and their resolution
//! into a [`SwitchPatch`] (state syntax and files checked).

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use serde_json::Value;

use crate::common::errors::{CliError, codes};
use crate::services::switch::{StateSpec, StatesPatch, SwitchPatch};

/// Keys a state object of `--states-from-json` accepts.
const JSON_KEYS: &[&str] = &["name", "id"];

const SYNTAX: &str = "State syntax: NAME[,id=N] (N is a non-zero whole number, unique in the switch)";

/// State flags shared by `create` and `update` beyond the original `--states`.
#[derive(Args, Debug, Default, Clone)]
pub struct SwitchExtraArgs {
    /// A state: NAME[,id=N]; repeatable. N is the id switch containers store (a fresh one
    /// is generated when omitted). Replaces the whole list
    #[arg(long, value_name = "NAME[,id=N]")]
    pub state: Vec<String>,

    /// Read the states from a JSON file instead of --states: a list of names or
    /// {"name": "wood", "id": 5} objects, or an SDK switch object with a "states" list
    #[arg(long, value_name = "FILE")]
    pub states_from_json: Option<PathBuf>,
}

/// State edits only `update` offers.
#[derive(Args, Debug, Default, Clone)]
pub struct SwitchEditArgs {
    /// Append a state (same syntax as --state); repeatable
    #[arg(long, value_name = "NAME[,id=N]")]
    pub add_state: Vec<String>,

    /// Remove a state by name or ID; refused while a switch container uses it; repeatable
    #[arg(long, value_name = "NAME|ID")]
    pub remove_state: Vec<String>,

    /// Rename a state, keeping its id: OLD=NEW (OLD is a name or an ID); repeatable
    #[arg(long, value_name = "OLD=NEW")]
    pub rename_state: Vec<String>,

    /// Remove every state (the switch needs new ones: combine with --add-state)
    #[arg(long)]
    pub clear_states: bool,
}

/// Every flag of `switch create` / `switch update`, in one place.
#[derive(Debug, Default, Clone)]
pub struct SwitchFlags {
    /// `--states`: comma-separated names, replaces the list.
    pub states: Option<Vec<String>>,
    pub extra: SwitchExtraArgs,
    pub edit: SwitchEditArgs,
}

fn field_error(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(codes::ERR_VALIDATION_FIELD, what, why).with_suggestion(SYNTAX)
}

fn parse_id(text: &str, state: &str) -> Result<u64> {
    match text.trim().parse::<u64>() {
        Ok(id) if id != 0 => Ok(id),
        _ => Err(field_error(
            format!("Invalid id '{text}' for state '{state}'"),
            "an id must be a non-zero whole number; the SDK treats 0 as no state",
        )
        .into()),
    }
}

/// Parses one `--state` / `--add-state` value: `NAME[,id=N]`.
pub fn parse_state(raw: &str) -> Result<StateSpec> {
    let mut parts = raw.split(',');
    let name = parts.next().unwrap_or("").trim();
    if name.is_empty() {
        return Err(field_error(
            format!("State '{raw}' has no name"),
            "a state starts with its name",
        )
        .into());
    }
    let mut id = None;
    for part in parts {
        let (key, value) = part.split_once('=').ok_or_else(|| {
            field_error(
                format!("State setting '{part}' in '{raw}' is not KEY=VALUE"),
                "after the name, every setting is written key=value",
            )
        })?;
        match key.trim() {
            "id" => {
                if id.replace(parse_id(value, name)?).is_some() {
                    return Err(field_error(
                        format!("State setting 'id' is given twice in '{raw}'"),
                        "each setting can appear once per state",
                    )
                    .into());
                }
            }
            other => {
                return Err(field_error(
                    format!("Unknown state setting '{other}' in '{raw}'"),
                    "the only setting is id",
                )
                .into());
            }
        }
    }
    Ok(StateSpec {
        name: name.to_string(),
        id,
    })
}

/// Parses one `--rename-state` value: `OLD=NEW`.
pub fn parse_rename(raw: &str) -> Result<(String, String)> {
    let bad = |why: &str| {
        field_error(
            format!("Invalid --rename-state '{raw}'"),
            format!("{why}; the syntax is OLD=NEW (for example stone=rock)"),
        )
    };
    let (old, new) = raw.split_once('=').ok_or_else(|| bad("there is no '='"))?;
    if old.trim().is_empty() || new.trim().is_empty() {
        return Err(bad("a name is empty").into());
    }
    Ok((old.trim().to_string(), new.trim().to_string()))
}

fn json_error(path: &Path, why: impl Into<String>) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Cannot use states file {}", path.display()),
        why,
    )
    .with_suggestion("The file holds a list of names or {\"name\": \"wood\", \"id\": 5} objects")
}

fn json_state(path: &Path, item: &Value) -> Result<StateSpec> {
    match item {
        Value::String(name) if !name.trim().is_empty() => Ok(StateSpec {
            name: name.trim().to_string(),
            id: None,
        }),
        Value::Object(map) => {
            if let Some(unknown) = map.keys().find(|k| !JSON_KEYS.contains(&k.as_str())) {
                return Err(json_error(
                    path,
                    format!("unknown state key '{unknown}' (valid: {})", JSON_KEYS.join(", ")),
                )
                .into());
            }
            let name = map
                .get("name")
                .and_then(|n| n.as_str())
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .ok_or_else(|| json_error(path, "every state needs a non-empty 'name'"))?;
            let id = match map.get("id") {
                None => None,
                Some(value) => Some(
                    value
                        .as_u64()
                        .filter(|id| *id != 0)
                        .ok_or_else(|| json_error(path, "'id' must be a non-zero whole number"))?,
                ),
            };
            Ok(StateSpec {
                name: name.to_string(),
                id,
            })
        }
        other => Err(json_error(path, format!("invalid state {other}: use a name or an object")).into()),
    }
}

/// Reads states from a JSON file: a list of states, or an SDK switch object whose
/// `states` holds that list.
pub fn states_from_json(path: &Path) -> Result<Vec<StateSpec>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| json_error(path, format!("the file cannot be read: {e}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| json_error(path, format!("the file is not valid JSON: {e}")))?;
    let items = match &value {
        Value::Array(items) => items,
        Value::Object(map) => match map.get("states") {
            Some(Value::Array(items)) => items,
            _ => return Err(json_error(path, "expected a list, or an object with 'states'").into()),
        },
        _ => return Err(json_error(path, "expected a list of states").into()),
    };
    items.iter().map(|item| json_state(path, item)).collect()
}

fn replace_list(flags: &SwitchFlags) -> Result<Option<Vec<StateSpec>>> {
    let sources = [
        flags.states.is_some(),
        !flags.extra.state.is_empty(),
        flags.extra.states_from_json.is_some(),
        flags.edit.clear_states,
    ];
    if sources.iter().filter(|set| **set).count() > 1 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Conflicting state flags",
            "--states, --state, --states-from-json and --clear-states each replace the whole state list",
        )
        .with_suggestion("Pass only one of them; use --add-state / --remove-state / --rename-state for incremental edits")
        .into());
    }
    if let Some(path) = &flags.extra.states_from_json {
        return states_from_json(path).map(Some);
    }
    if !flags.extra.state.is_empty() {
        return flags.extra.state.iter().map(|raw| parse_state(raw)).collect::<Result<Vec<_>>>().map(Some);
    }
    if let Some(names) = &flags.states {
        return Ok(Some(
            names
                .iter()
                .map(|n| n.trim())
                .filter(|n| !n.is_empty())
                .map(|n| StateSpec {
                    name: n.to_string(),
                    id: None,
                })
                .collect(),
        ));
    }
    Ok(flags.edit.clear_states.then(Vec::new))
}

/// Turns the raw flags into a patch. Fails fast on the first unusable value.
pub fn resolve_patch(flags: &SwitchFlags) -> Result<SwitchPatch> {
    Ok(SwitchPatch {
        states: StatesPatch {
            replace: replace_list(flags)?,
            rename: flags
                .edit
                .rename_state
                .iter()
                .map(|raw| parse_rename(raw))
                .collect::<Result<_>>()?,
            remove: flags.edit.remove_state.clone(),
            add: flags
                .edit
                .add_state
                .iter()
                .map(|raw| parse_state(raw))
                .collect::<Result<_>>()?,
        },
    })
}
