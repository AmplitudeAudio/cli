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

//! Parsing of event actions from `--action TYPE:TARGET[,KEY=VALUE...]` and
//! target resolution by name, `kind:name` or ID.

use anyhow::Result;

use crate::assets::generated::{EventActionDefinition, EventActionType, Scope};
use crate::assets::{AssetType, ProjectValidator};
use crate::common::errors::{CliError, codes};
use crate::forms::pick::{create_command, target_label};
use crate::forms::references::{PLAYBACK_TARGETS, RefTarget};

use super::sound_args::parse_scope;

/// Action settings (besides the targets) the compact syntax accepts.
pub const SETTING_KEYS: &[&str] = &["active", "scope"];

/// The action type names as written in flags, in schema order.
pub const CLI_TYPE_NAMES: &str =
    "none, play, pause, resume, stop, seek, mute-bus, unmute-bus, wait";

/// The syntax reminder attached to action errors.
pub const SYNTAX: &str = "Action syntax: TYPE:TARGET[,TARGET...][,active=BOOL][,scope=world|entity] \
(e.g. play:step1,crowd or mute-bus:sfx or wait:500); name a target by kind with sound:NAME, \
collection:NAME, switch-container:NAME or bus:NAME";

pub fn field_error(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(codes::ERR_VALIDATION_FIELD, what, why).with_suggestion(SYNTAX)
}

fn reference_error(what: String, why: String, suggestion: String) -> CliError {
    CliError::new(codes::ERR_VALIDATION_REFERENCE, what, why).with_suggestion(suggestion)
}

/// Parses an action type name (`play`, `mute-bus`, `MuteBus`...).
pub fn parse_type(name: &str) -> Result<EventActionType> {
    EventActionType::from_name(name.trim()).map_err(|_| {
        field_error(
            format!("Invalid action type: '{name}'"),
            format!("Action type must be one of: {CLI_TYPE_NAMES}"),
        )
        .into()
    })
}

/// The kind named by a `kind:` prefix, if `prefix` is one.
fn kind_from_prefix(prefix: &str) -> Option<RefTarget> {
    match prefix
        .trim()
        .replace(['-', '_'], "")
        .to_ascii_lowercase()
        .as_str()
    {
        "sound" => Some(RefTarget::Asset(AssetType::Sound)),
        "collection" => Some(RefTarget::Asset(AssetType::Collection)),
        "switchcontainer" => Some(RefTarget::Asset(AssetType::SwitchContainer)),
        "bus" => Some(RefTarget::Bus),
        _ => None,
    }
}

/// Every kind some action can target, to recognise an object of the wrong kind.
fn all_kinds() -> Vec<RefTarget> {
    let mut kinds = PLAYBACK_TARGETS.members();
    kinds.push(RefTarget::Bus);
    kinds
}

fn find_named(validator: &ProjectValidator, kind: RefTarget, name: &str) -> Option<u64> {
    if let Some(found) = validator.list(kind).iter().find(|c| c.name == name) {
        return Some(found.id);
    }
    // A bus outside the active bus file is accepted (the caller warns).
    if kind == RefTarget::Bus {
        return validator.find_bus_anywhere(Some(name), None).map(|b| b.id);
    }
    None
}

fn wrong_kind(
    name: &str,
    actual: RefTarget,
    action: EventActionType,
    wanted: RefTarget,
) -> CliError {
    reference_error(
        format!(
            "'{name}' is a {}, not a {}",
            target_label(actual),
            target_label(wanted)
        ),
        format!("{action:?} actions target a {}", target_label(wanted)),
        format!("Choose a {}", target_label(wanted)),
    )
}

/// Resolves one target of an `action` given as a name, `kind:name` or numeric ID.
///
/// A name that exists under several of the kinds the action can target must be
/// written `kind:name`. A numeric ID is returned as given; the caller validates it.
pub fn resolve_target(
    validator: &ProjectValidator,
    action: EventActionType,
    raw: &str,
) -> Result<u64> {
    let raw = raw.trim();
    let Some(wanted) = action.target_kinds() else {
        return raw.parse::<u64>().map_err(|_| {
            field_error(
                format!("Invalid target '{raw}'"),
                "Targets must be positive integers",
            )
            .into()
        });
    };

    let (explicit, name) = match raw.split_once(':') {
        Some((prefix, name)) => match kind_from_prefix(prefix) {
            Some(kind) => (Some(kind), name.trim()),
            None => (None, raw),
        },
        None => (None, raw),
    };
    if let Some(kind) = explicit
        && !wanted.includes(kind)
    {
        return Err(wrong_kind(name, kind, action, wanted).into());
    }

    let searched = explicit.map_or_else(|| wanted.members(), |kind| vec![kind]);
    let matches: Vec<(RefTarget, u64)> = searched
        .iter()
        .filter_map(|kind| find_named(validator, *kind, name).map(|id| (*kind, id)))
        .collect();
    match matches.as_slice() {
        [(_, id)] => return Ok(*id),
        [] => {}
        many => {
            let spellings: Vec<String> = many
                .iter()
                .map(|(kind, _)| format!("{}:{name}", target_label(*kind).replace(' ', "-")))
                .collect();
            let which = format!("Write {} to say which one", spellings.join(" or "));
            return Err(reference_error(
                format!("'{name}' matches more than one kind of object"),
                which.clone(),
                which,
            )
            .into());
        }
    }

    if explicit.is_none() {
        if let Ok(id) = name.parse::<u64>() {
            return Ok(id);
        }
        if let Some(actual) = all_kinds()
            .into_iter()
            .find(|k| !wanted.includes(*k) && find_named(validator, *k, name).is_some())
        {
            return Err(wrong_kind(name, actual, action, wanted).into());
        }
    }

    let label = target_label(wanted);
    let candidates = validator.list(wanted);
    let suggestion = if candidates.is_empty() {
        format!(
            "the project has no {label} yet; create one with `{}`",
            create_command(wanted)
        )
    } else {
        format!(
            "available: {}",
            candidates
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Err(reference_error(
        format!("No {label} named '{name}'"),
        format!("{action:?} targets must be the name or ID of an existing {label}; {suggestion}"),
        suggestion,
    )
    .into())
}

fn parse_bool(key: &str, value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(field_error(
            format!("Invalid {key} value: '{value}'"),
            format!("{key} must be true or false"),
        )
        .into()),
    }
}

/// Resolves the target of a Wait action: a duration in milliseconds.
fn parse_duration(raw: &str) -> Result<u64> {
    raw.trim().parse::<u64>().map_err(|_| {
        field_error(
            format!("Invalid wait duration: '{raw}'"),
            "A wait action takes its duration in milliseconds, for example wait:500",
        )
        .into()
    })
}

/// Resolves the targets of an action of `type_` from their raw spellings.
pub fn resolve_targets(
    validator: &ProjectValidator,
    type_: EventActionType,
    raw: &[String],
) -> Result<Vec<u64>> {
    raw.iter()
        .map(|t| match type_ {
            EventActionType::Wait => parse_duration(t),
            _ => resolve_target(validator, type_, t),
        })
        .collect()
}

/// Parses one `--action` value: `TYPE:TARGET[,TARGET...][,active=BOOL][,scope=world|entity]`.
pub fn parse_action(raw: &str, validator: &ProjectValidator) -> Result<EventActionDefinition> {
    let (type_name, rest) = raw.split_once(':').unwrap_or((raw, ""));
    let type_ = parse_type(type_name)?;

    let mut targets = Vec::new();
    let (mut active, mut scope): (Option<bool>, Option<Scope>) = (None, None);
    for token in rest.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        match token.split_once('=') {
            Some((key, value)) => {
                let key = key.trim();
                let seen = match key {
                    "active" => active.replace(parse_bool(key, value)?).is_some(),
                    "scope" => scope.replace(parse_scope(value.trim())?).is_some(),
                    _ => {
                        return Err(field_error(
                            format!("Unknown action setting '{key}' in '{raw}'"),
                            format!("Valid settings: {}", SETTING_KEYS.join(", ")),
                        )
                        .into());
                    }
                };
                if seen {
                    return Err(field_error(
                        format!("Action setting '{key}' is given twice in '{raw}'"),
                        "Each setting can appear once per action",
                    )
                    .into());
                }
            }
            None => targets.push(token.to_string()),
        }
    }

    if targets.is_empty() && type_ != EventActionType::None {
        return Err(field_error(
            format!("Action '{raw}' has no target"),
            format!("{type_:?} actions need at least one target after the type"),
        )
        .into());
    }

    let mut action = crate::assets::event_actions::new_action(
        type_,
        resolve_targets(validator, type_, &targets)?,
    );
    action.active = active.unwrap_or(true);
    action.scope = scope.unwrap_or(Scope::Entity);
    Ok(action)
}
