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

//! Parsing of collection entries from `--entry`, `--add-entry` and `--entries-from-json`.

use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::assets::generated::RtpcCompatibleValue;
use crate::assets::{AssetType, ProjectValidator};
use crate::common::errors::{CliError, codes};
use crate::forms::references::rule_for;
use crate::forms::rtpc_value::rtpc_value_from_flags;
use crate::services::collection_entries::{EntriesPatch, EntrySpec};

use super::collection_args::{CollectionEntryEditArgs, CollectionExtraArgs};
use super::sound_args::resolve_reference;

/// Settings an entry accepts, in `--entry` syntax and in JSON.
const ENTRY_KEYS: &[&str] = &["sound", "weight", "gain", "pitch"];

fn field_error(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(codes::ERR_VALIDATION_FIELD, what, why).with_suggestion(
        "Entry syntax: SOUND[,weight=W][,gain=G|gain-rtpc=NAME][,pitch=P|pitch-rtpc=NAME]",
    )
}

/// Resolves an entry's sound given as a name or a numeric ID.
///
/// A numeric ID is returned as given; whether it exists is checked afterwards.
pub fn resolve_sound(validator: &ProjectValidator, raw: &str) -> Result<u64> {
    let rule = rule_for(AssetType::Collection, "sounds.sound")
        .ok_or_else(|| anyhow::anyhow!("Collection has no reference rule for 'sounds.sound'"))?;
    resolve_reference(validator, &rule, raw.trim())
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

/// Parses one `--entry` value: `SOUND[,weight=W][,gain=G|gain-rtpc=NAME][,pitch=P|pitch-rtpc=NAME]`.
pub fn parse_entry(raw: &str, validator: &ProjectValidator) -> Result<EntrySpec> {
    let mut parts = raw.split(',');
    let sound_name = parts.next().unwrap_or("").trim();
    if sound_name.is_empty() {
        return Err(field_error(
            format!("Entry '{raw}' has no sound"),
            "An entry starts with the name or ID of its sound",
        )
        .into());
    }

    let (mut weight, mut gain, mut pitch) = (None, None, None);
    let (mut gain_rtpc, mut pitch_rtpc) = (None, None);
    for part in parts {
        let (key, value) = part.split_once('=').ok_or_else(|| {
            field_error(
                format!("Entry setting '{part}' in '{raw}' is not KEY=VALUE"),
                "After the sound, every setting is written key=value",
            )
        })?;
        let (key, value) = (key.trim(), value.trim());
        let seen = match key {
            "weight" => weight.replace(parse_number(key, value, raw)?).is_some(),
            "gain" => gain.replace(parse_number(key, value, raw)?).is_some(),
            "pitch" => pitch.replace(parse_number(key, value, raw)?).is_some(),
            "gain-rtpc" => gain_rtpc.replace(value).is_some(),
            "pitch-rtpc" => pitch_rtpc.replace(value).is_some(),
            _ => {
                return Err(field_error(
                    format!("Unknown entry setting '{key}' in '{raw}'"),
                    "Valid settings: weight, gain, gain-rtpc, pitch, pitch-rtpc",
                )
                .into());
            }
        };
        if seen {
            return Err(field_error(
                format!("Entry setting '{key}' is given twice in '{raw}'"),
                "Each setting can appear once per entry",
            )
            .into());
        }
    }

    Ok(EntrySpec {
        sound: resolve_sound(validator, sound_name)?,
        gain: rtpc_value_from_flags(gain, gain_rtpc, validator)?,
        pitch: rtpc_value_from_flags(pitch, pitch_rtpc, validator)?,
        weight,
    })
}

fn json_error(path: &Path, why: impl Into<String>) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Cannot use entries file {}", path.display()),
        why,
    )
    .with_suggestion(
        "The file holds a list of {\"sound\": NAME|ID, \"weight\": N, \"gain\": N|VALUE, \"pitch\": N|VALUE} objects",
    )
}

fn json_value(path: &Path, key: &str, value: &Value) -> Result<RtpcCompatibleValue> {
    if let Some(number) = value.as_f64() {
        return Ok(RtpcCompatibleValue::static_value(number as f32));
    }
    serde_json::from_value(value.clone()).map_err(|e| {
        json_error(
            path,
            format!("'{key}' is neither a number nor a value object: {e}"),
        )
        .into()
    })
}

fn json_entry(path: &Path, item: &Value, validator: &ProjectValidator) -> Result<EntrySpec> {
    let object = item
        .as_object()
        .ok_or_else(|| json_error(path, "every entry must be an object"))?;
    if let Some(unknown) = object.keys().find(|k| !ENTRY_KEYS.contains(&k.as_str())) {
        return Err(json_error(
            path,
            format!(
                "unknown entry key '{unknown}' (valid: {})",
                ENTRY_KEYS.join(", ")
            ),
        )
        .into());
    }
    let sound = match object.get("sound") {
        Some(Value::String(name)) => resolve_sound(validator, name)?,
        Some(Value::Number(id)) => id
            .as_u64()
            .ok_or_else(|| json_error(path, "'sound' must be a name or a non-negative ID"))?,
        _ => return Err(json_error(path, "every entry needs a 'sound' (name or ID)").into()),
    };
    let weight = match object.get("weight") {
        None => None,
        Some(w) => Some(
            w.as_f64()
                .ok_or_else(|| json_error(path, "'weight' must be a number"))? as f32,
        ),
    };
    Ok(EntrySpec {
        sound,
        gain: object
            .get("gain")
            .map(|v| json_value(path, "gain", v))
            .transpose()?,
        pitch: object
            .get("pitch")
            .map(|v| json_value(path, "pitch", v))
            .transpose()?,
        weight,
    })
}

/// Reads entries from a JSON file: a list of entry objects, or the SDK's
/// parallel-array form `{"sounds_type": [...], "sounds": [...]}` (the tags are
/// ignored: the scheduler mode decides each entry's kind).
pub fn entries_from_json(path: &Path, validator: &ProjectValidator) -> Result<Vec<EntrySpec>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| json_error(path, format!("the file cannot be read: {e}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| json_error(path, format!("the file is not valid JSON: {e}")))?;
    let items = match &value {
        Value::Array(items) => items,
        Value::Object(map) => match map.get("sounds") {
            Some(Value::Array(items)) => {
                if let Some(Value::Array(tags)) = map.get("sounds_type")
                    && tags.len() != items.len()
                {
                    return Err(json_error(
                        path,
                        "'sounds_type' and 'sounds' must have the same length",
                    )
                    .into());
                }
                items
            }
            _ => {
                return Err(json_error(path, "expected a list, or an object with 'sounds'").into());
            }
        },
        _ => return Err(json_error(path, "expected a list of entries").into()),
    };
    items
        .iter()
        .map(|i| json_entry(path, i, validator))
        .collect()
}

/// Builds the entry changes asked for by the entry flags.
pub fn resolve_entries(
    extra: &CollectionExtraArgs,
    edit: &CollectionEntryEditArgs,
    validator: &ProjectValidator,
) -> Result<EntriesPatch> {
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
            "Pass only one of them; use --add-entry / --remove-entry for incremental edits",
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

    Ok(EntriesPatch {
        replace,
        remove: edit
            .remove_entry
            .iter()
            .map(|raw| resolve_sound(validator, raw))
            .collect::<Result<_>>()?,
        add: edit
            .add_entry
            .iter()
            .map(|raw| parse_entry(raw, validator))
            .collect::<Result<_>>()?,
    })
}

/// The sound ids of the entries a patch adds, in order (replacements first).
pub fn entry_sounds(patch: &EntriesPatch) -> Vec<u64> {
    patch
        .replace
        .iter()
        .flatten()
        .chain(&patch.add)
        .map(|e| e.sound)
        .collect()
}
