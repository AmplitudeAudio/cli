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

//! Parsing of switch container entries from `--entry`, `--add-entry`, `--map` and
//! `--entries-from-json`.
//!
//! Entries name their switch states, but the states belong to the container's
//! switch, which may only be chosen later (interactively). So parsing yields
//! [`RawEntry`] values that keep the state names, and [`bind_entries`] resolves
//! them once the switch is known.

use anyhow::Result;

use crate::assets::generated::FadeTransitionSettings;
use crate::assets::{AssetType, ProjectValidator};
use crate::common::errors::{CliError, codes};
use crate::forms::pick::{create_command, target_label};
use crate::forms::references::{RefTarget, rule_for};
use crate::forms::rtpc_value::rtpc_value_from_flags;
use crate::services::switch_container_entries::{DEFAULT_FADE_FADER, EntriesPatch, EntrySpec};

use super::sound_args::parse_fader;
use super::switch_container_args::{SwitchContainerEntryEditArgs, SwitchContainerExtraArgs};
use super::switch_container_entry_json::entries_from_json;

const SYNTAX: &str = "Entry syntax: OBJECT,states=STATE|STATE[,continue=true][,fade-in=SECONDS[:FADER]][,fade-out=SECONDS[:FADER]][,gain=G|gain-rtpc=NAME][,pitch=P|pitch-rtpc=NAME]";
/// An entry whose states are still names (or ids) to look up in the container's switch.
#[derive(Debug, Clone, PartialEq)]
pub struct RawEntry {
    /// The entry with an empty state list.
    pub spec: EntrySpec,
    /// State names or numeric ids, as written.
    pub states: Vec<String>,
}

/// The entry changes asked for by the flags, with state names not yet resolved.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawEntriesPatch {
    pub replace: Option<Vec<RawEntry>>,
    pub remove: Vec<u64>,
    pub add: Vec<RawEntry>,
    /// `(state, object)` pairs from `--map STATE=OBJECT`.
    pub map: Vec<(String, u64)>,
}

impl RawEntriesPatch {
    /// The objects of the entries the patch brings in, in order (replacements first).
    pub fn incoming_objects(&self) -> Vec<u64> {
        self.replace
            .iter()
            .flatten()
            .chain(&self.add)
            .map(|e| e.spec.object)
            .chain(self.map.iter().map(|(_, object)| *object))
            .collect()
    }
}

fn field_error(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(codes::ERR_VALIDATION_FIELD, what, why).with_suggestion(SYNTAX)
}

/// Resolves an entry's object given as `NAME`, `sound:NAME`, `collection:NAME` or a numeric ID.
///
/// A numeric ID is returned as given; whether it exists is checked afterwards. A name
/// shared by a sound and a collection must be qualified.
pub fn resolve_object(validator: &ProjectValidator, raw: &str) -> Result<u64> {
    let raw = raw.trim();
    let sounds = RefTarget::Asset(AssetType::Sound);
    let collections = RefTarget::Asset(AssetType::Collection);
    let (kinds, name) = match raw.split_once(':') {
        Some((kind, name)) if kind.eq_ignore_ascii_case("sound") => (vec![sounds], name),
        Some((kind, name)) if kind.eq_ignore_ascii_case("collection") => (vec![collections], name),
        _ => (vec![sounds, collections], raw),
    };
    let matches: Vec<(RefTarget, u64)> = kinds
        .iter()
        .flat_map(|kind| {
            validator
                .list(*kind)
                .into_iter()
                .filter(|c| c.name == name)
                .map(|c| (*kind, c.id))
        })
        .collect();
    match matches.as_slice() {
        [(_, id)] => return Ok(*id),
        [_, _, ..] => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_REFERENCE,
                format!("'{name}' is both a sound and a collection"),
                "an entry object can be either; the name alone does not say which",
            )
            .with_suggestion(format!("Write sound:{name} or collection:{name}"))
            .into());
        }
        [] => {}
    }
    if let Ok(id) = name.parse::<u64>() {
        return Ok(id);
    }

    let target =
        rule_for(AssetType::SwitchContainer, "entries.object").map_or(sounds, |rule| rule.target);
    let available: Vec<String> = validator.list(target).into_iter().map(|c| c.name).collect();
    let hint = if available.is_empty() {
        format!(
            "the project has no {} yet; create one with `{}`",
            target_label(target),
            create_command(target)
        )
    } else {
        format!("available: {}", available.join(", "))
    };
    Err(CliError::new(
        codes::ERR_VALIDATION_REFERENCE,
        format!("No {} named '{name}'", target_label(target)),
        "an entry object must be the name or ID of an existing sound or collection",
    )
    .with_suggestion(hint)
    .into())
}

/// Resolves a state given as a name of the switch `group` or a numeric ID.
///
/// A numeric ID is returned as given; whether the switch has it is checked afterwards.
pub fn resolve_state(validator: &ProjectValidator, group: u64, raw: &str) -> Result<u64> {
    let raw = raw.trim();
    let states = validator.switch_states(group).unwrap_or(&[]);
    if let Some(found) = states.iter().find(|s| s.name == raw) {
        return Ok(found.id);
    }
    if let Ok(id) = raw.parse::<u64>() {
        return Ok(id);
    }
    Err(CliError::new(
        codes::ERR_VALIDATION_REFERENCE,
        format!("No switch state named '{raw}'"),
        "a state must be the name or ID of a state of the container's switch",
    )
    .with_suggestion(validator.describe_states(group))
    .into())
}

fn parse_number(key: &str, value: &str, entry: &str) -> Result<f32> {
    value.trim().parse::<f32>().map_err(|_| {
        field_error(
            format!("Invalid {key} '{value}' in entry '{entry}'"),
            format!("{key} must be a number"),
        )
        .into()
    })
}

fn parse_bool(key: &str, value: &str, entry: &str) -> Result<bool> {
    match value.trim().to_lowercase().as_str() {
        "true" | "yes" => Ok(true),
        "false" | "no" => Ok(false),
        _ => Err(field_error(
            format!("Invalid {key} '{value}' in entry '{entry}'"),
            format!("{key} must be true or false"),
        )
        .into()),
    }
}

/// A fade written `SECONDS` or `SECONDS:FADER`.
fn parse_fade(key: &str, value: &str, entry: &str) -> Result<FadeTransitionSettings> {
    let (seconds, fader) = match value.split_once(':') {
        Some((seconds, fader)) => (seconds, parse_fader(fader.trim())?),
        None => (value, DEFAULT_FADE_FADER.to_string()),
    };
    let duration = parse_number(key, seconds, entry)?;
    if !duration.is_finite() || duration < 0.0 {
        return Err(field_error(
            format!("Invalid {key} '{value}' in entry '{entry}'"),
            format!("{key} needs a duration in seconds that is not negative"),
        )
        .into());
    }
    Ok(FadeTransitionSettings {
        duration: f64::from(duration),
        fader: Some(fader),
    })
}

fn split_states(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn missing_states(entry: &str) -> CliError {
    field_error(
        format!("Entry '{entry}' has no switch states"),
        "An entry plays only in the states listed with states=STATE|STATE",
    )
}

/// Parses one `--entry` value (see the syntax in `--help`).
pub fn parse_entry(raw: &str, validator: &ProjectValidator) -> Result<RawEntry> {
    let mut parts = raw.split(',');
    let object_name = parts.next().unwrap_or("").trim();
    if object_name.is_empty() {
        return Err(field_error(
            format!("Entry '{raw}' has no object"),
            "An entry starts with the name or ID of its sound or collection",
        )
        .into());
    }

    let mut spec = EntrySpec::new(resolve_object(validator, object_name)?, Vec::new());
    let mut states: Option<Vec<String>> = None;
    let (mut gain, mut pitch) = (None, None);
    let (mut gain_rtpc, mut pitch_rtpc) = (None, None);
    let mut seen: Vec<&str> = Vec::new();
    for part in parts {
        let (key, value) = match part.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim()),
            None if part.trim() == "continue" => ("continue", "true"),
            None => {
                return Err(field_error(
                    format!("Entry setting '{part}' in '{raw}' is not KEY=VALUE"),
                    "After the object, every setting is written key=value",
                )
                .into());
            }
        };
        if seen.contains(&key) {
            return Err(field_error(
                format!("Entry setting '{key}' is given twice in '{raw}'"),
                "Each setting can appear once per entry",
            )
            .into());
        }
        match key {
            "states" => states = Some(split_states(value)),
            "continue" => spec.continue_between_states = parse_bool(key, value, raw)?,
            "fade-in" => spec.fade_in = Some(parse_fade(key, value, raw)?),
            "fade-out" => spec.fade_out = Some(parse_fade(key, value, raw)?),
            "gain" => gain = Some(parse_number(key, value, raw)?),
            "pitch" => pitch = Some(parse_number(key, value, raw)?),
            "gain-rtpc" => gain_rtpc = Some(value),
            "pitch-rtpc" => pitch_rtpc = Some(value),
            _ => {
                return Err(field_error(
                    format!("Unknown entry setting '{key}' in '{raw}'"),
                    "Valid settings: states, continue, fade-in, fade-out, gain, gain-rtpc, pitch, pitch-rtpc",
                )
                .into());
            }
        }
        seen.push(key);
    }
    spec.gain = rtpc_value_from_flags(gain, gain_rtpc, validator)?;
    spec.pitch = rtpc_value_from_flags(pitch, pitch_rtpc, validator)?;

    match states {
        Some(states) if !states.is_empty() => Ok(RawEntry { spec, states }),
        _ => Err(missing_states(raw).into()),
    }
}

/// Parses `--map STATE=OBJECT`.
pub fn parse_map(raw: &str, validator: &ProjectValidator) -> Result<(String, u64)> {
    let (state, object) = raw.split_once('=').ok_or_else(|| {
        CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid mapping format: '{raw}'"),
            "Mappings must be in format 'state=object'",
        )
        .with_suggestion(
            "Use --map state_name=sound_or_collection_name (e.g. --map wood=footstep_wood)",
        )
    })?;
    if state.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid mapping format: '{raw}'"),
            "The state before '=' is empty",
        )
        .into());
    }
    Ok((state.trim().to_string(), resolve_object(validator, object)?))
}

/// Builds the entry changes asked for by the entry flags.
pub fn resolve_entries(
    extra: &SwitchContainerExtraArgs,
    edit: &SwitchContainerEntryEditArgs,
    validator: &ProjectValidator,
) -> Result<RawEntriesPatch> {
    let replace_sources = [
        !extra.entry.is_empty(),
        extra.entries_from_json.is_some(),
        edit.clear_entries,
    ];
    if replace_sources.iter().filter(|set| **set).count() > 1 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Conflicting entry flags",
            "--entry, --entries-from-json and --clear-entries each replace the whole entry list",
        )
        .with_suggestion(
            "Pass only one of them; use --add-entry / --remove-entry / --map for incremental edits",
        )
        .into());
    }

    let replace = if let Some(path) = &extra.entries_from_json {
        Some(entries_from_json(path, validator)?)
    } else if !extra.entry.is_empty() {
        Some(
            extra
                .entry
                .iter()
                .map(|raw| parse_entry(raw, validator))
                .collect::<Result<Vec<_>>>()?,
        )
    } else if edit.clear_entries {
        Some(Vec::new())
    } else {
        None
    };

    Ok(RawEntriesPatch {
        replace,
        remove: edit
            .remove_entry
            .iter()
            .map(|raw| resolve_object(validator, raw))
            .collect::<Result<_>>()?,
        add: edit
            .add_entry
            .iter()
            .map(|raw| parse_entry(raw, validator))
            .collect::<Result<_>>()?,
        map: extra
            .map
            .iter()
            .map(|raw| parse_map(raw, validator))
            .collect::<Result<_>>()?,
    })
}

fn bind_entry(validator: &ProjectValidator, group: u64, raw: &RawEntry) -> Result<EntrySpec> {
    let states = raw
        .states
        .iter()
        .map(|s| resolve_state(validator, group, s))
        .collect::<Result<Vec<_>>>()?;
    Ok(EntrySpec {
        switch_states: states,
        ..raw.spec.clone()
    })
}

/// Resolves the state names of `raw` against the states of the switch `group`.
pub fn bind_entries(
    validator: &ProjectValidator,
    group: u64,
    raw: &RawEntriesPatch,
) -> Result<EntriesPatch> {
    let bind_all = |entries: &[RawEntry]| -> Result<Vec<EntrySpec>> {
        entries
            .iter()
            .map(|e| bind_entry(validator, group, e))
            .collect()
    };
    Ok(EntriesPatch {
        replace: raw.replace.as_deref().map(bind_all).transpose()?,
        remove: raw.remove.clone(),
        add: bind_all(&raw.add)?,
        map: raw
            .map
            .iter()
            .map(|(state, object)| Ok((*object, resolve_state(validator, group, state)?)))
            .collect::<Result<_>>()?,
    })
}
