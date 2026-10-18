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

//! Command-line flags of `event create` / `event update` and their resolution
//! into an [`EventPatch`] (names looked up, enums parsed, numbers checked).

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;

use crate::assets::event_actions::{new_action, validate_action_shape, validate_action_targets};
use crate::assets::generated::{EventActionDefinition, EventActionRunningMode, EventActionType};
use crate::assets::ProjectValidator;
use crate::common::errors::{CliError, codes};
use crate::forms::validate_refs::inactive_bus_warning;
use crate::presentation::Output;
use crate::services::event::{ActionsPatch, EventPatch};

use super::event_action_parse::{field_error, parse_action, parse_type, resolve_targets};
use super::sound_args::parse_scope;

/// Keys an action object of `--actions-from-json` accepts.
const JSON_KEYS: &[&str] = &["type", "active", "scope", "targets"];

/// Every action-related flag of `event create` / `event update`, in one place.
#[derive(Debug, Default, Clone)]
pub struct EventFlags {
    pub run_mode: Option<String>,
    /// `--action`: replaces the whole list.
    pub action: Vec<String>,
    /// `--actions-from-json`: replaces the whole list.
    pub actions_from_json: Option<PathBuf>,
    /// `--add-action`: appended.
    pub add_action: Vec<String>,
    /// `--remove-action`: 0-based indexes.
    pub remove_action: Vec<usize>,
    /// `--clear-actions`: replaces the list with an empty one.
    pub clear_actions: bool,
}

/// Parses `--run-mode`.
pub fn parse_run_mode(value: &str) -> Result<EventActionRunningMode> {
    EventActionRunningMode::from_name(value).map_err(|_| {
        CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid run mode: '{value}'"),
            "Run mode must be one of: parallel, sequential",
        )
        .with_suggestion("Use --run-mode parallel or --run-mode sequential")
        .into()
    })
}

fn json_error(path: &Path, why: impl Into<String>) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Cannot use actions file {}", path.display()),
        why,
    )
    .with_suggestion(
        "The file holds a list of {\"type\": \"Play\", \"targets\": [NAME|ID...], \"active\": true, \"scope\": \"World\"} objects",
    )
}

fn json_targets(
    path: &Path,
    type_: EventActionType,
    value: Option<&Value>,
    validator: &ProjectValidator,
) -> Result<Vec<u64>> {
    let Some(items) = value.and_then(|v| v.as_array()) else {
        return Err(json_error(path, "'targets' must be a list").into());
    };
    let raw: Vec<String> = items
        .iter()
        .map(|item| match item {
            Value::String(name) => Ok(name.clone()),
            Value::Number(n) if n.is_u64() => Ok(n.to_string()),
            other => Err(json_error(
                path,
                format!("invalid target {other}: use a name or a non-negative ID"),
            )),
        })
        .collect::<Result<_, _>>()?;
    resolve_targets(validator, type_, &raw)
}

fn json_action(
    path: &Path,
    item: &Value,
    validator: &ProjectValidator,
) -> Result<EventActionDefinition> {
    let object = item
        .as_object()
        .ok_or_else(|| json_error(path, "every action must be an object"))?;
    if let Some(unknown) = object.keys().find(|k| !JSON_KEYS.contains(&k.as_str())) {
        return Err(json_error(
            path,
            format!("unknown action key '{unknown}' (valid: {})", JSON_KEYS.join(", ")),
        )
        .into());
    }
    let type_ = match object.get("type").and_then(|t| t.as_str()) {
        Some(name) => parse_type(name)?,
        None => return Err(json_error(path, "every action needs a 'type'").into()),
    };
    let targets = match object.get("targets") {
        Some(value) => json_targets(path, type_, Some(value), validator)?,
        None => Vec::new(),
    };
    let mut action = new_action(type_, targets);
    if let Some(active) = object.get("active") {
        action.active = active
            .as_bool()
            .ok_or_else(|| json_error(path, "'active' must be true or false"))?;
    }
    if let Some(scope) = object.get("scope") {
        let name = scope
            .as_str()
            .ok_or_else(|| json_error(path, "'scope' must be \"World\" or \"Entity\""))?;
        action.scope = parse_scope(name)?;
    }
    Ok(action)
}

/// Reads actions from a JSON file: a list of action objects, or an SDK event
/// object whose `actions` holds that list.
pub fn actions_from_json(
    path: &Path,
    validator: &ProjectValidator,
) -> Result<Vec<EventActionDefinition>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| json_error(path, format!("the file cannot be read: {e}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| json_error(path, format!("the file is not valid JSON: {e}")))?;
    let items = match &value {
        Value::Array(items) => items,
        Value::Object(map) => match map.get("actions") {
            Some(Value::Array(items)) => items,
            _ => return Err(json_error(path, "expected a list, or an object with 'actions'").into()),
        },
        _ => return Err(json_error(path, "expected a list of actions").into()),
    };
    items
        .iter()
        .map(|item| json_action(path, item, validator))
        .collect()
}

/// Turns the raw flags into a patch: names become ids, enums are parsed and
/// numbers checked. Fails fast on the first unusable value.
pub fn resolve_patch(flags: &EventFlags, validator: &ProjectValidator) -> Result<EventPatch> {
    let replace_sources = [
        !flags.action.is_empty(),
        flags.actions_from_json.is_some(),
        flags.clear_actions,
    ];
    if replace_sources.iter().filter(|set| **set).count() > 1 {
        return Err(field_error(
            "Conflicting action flags",
            "--action, --actions-from-json and --clear-actions each replace the whole action list",
        )
        .with_suggestion(
            "Pass only one of them; use --add-action / --remove-action for incremental edits",
        )
        .into());
    }

    let run_mode = flags.run_mode.as_deref().map(parse_run_mode).transpose()?;
    let replace = if let Some(path) = &flags.actions_from_json {
        Some(actions_from_json(path, validator)?)
    } else if !flags.action.is_empty() {
        Some(
            flags
                .action
                .iter()
                .map(|raw| parse_action(raw, validator))
                .collect::<Result<Vec<_>>>()?,
        )
    } else if flags.clear_actions {
        Some(Vec::new())
    } else {
        None
    };

    Ok(EventPatch {
        run_mode,
        actions: ActionsPatch {
            replace,
            remove: flags.remove_action.clone(),
            add: flags
                .add_action
                .iter()
                .map(|raw| parse_action(raw, validator))
                .collect::<Result<_>>()?,
        },
    })
}

/// The actions a patch brings in (replacements first), to check before anything is asked.
fn incoming_actions(patch: &EventPatch) -> impl Iterator<Item = &EventActionDefinition> {
    patch.actions.replace.iter().flatten().chain(&patch.actions.add)
}

/// Checks the shape and targets of the actions a patch sets against the project,
/// so a dangling or wrong-kind id is reported before the user is asked anything else.
pub fn validate_patch_references(patch: &EventPatch, validator: &ProjectValidator) -> Result<()> {
    for (index, action) in incoming_actions(patch).enumerate() {
        validate_action_shape(action, index)?;
        validate_action_targets(action, index, Some(validator))?;
    }
    Ok(())
}

/// Warns (does not fail) about bus targets that exist only outside the bus file the
/// engine configs load. Prints each warning and returns them for the JSON result.
pub fn bus_warnings(
    validator: &ProjectValidator,
    actions: &[&EventActionDefinition],
    output: &dyn Output,
) -> Vec<String> {
    let mut ids: Vec<u64> = actions
        .iter()
        .filter(|a| matches!(a.type_, EventActionType::MuteBus | EventActionType::UnmuteBus))
        .flat_map(|a| a.targets.iter().flatten().copied())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let warnings: Vec<String> = ids
        .into_iter()
        .filter_map(|id| inactive_bus_warning(validator, id))
        .collect();
    for warning in &warnings {
        output.warning(warning);
    }
    warnings
}

/// The actions a patch brings in, for [`bus_warnings`].
pub fn patch_actions(patch: &EventPatch) -> Vec<&EventActionDefinition> {
    incoming_actions(patch).collect()
}
