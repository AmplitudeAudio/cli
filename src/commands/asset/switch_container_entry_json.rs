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

//! `--entries-from-json`: switch container entries read from a JSON file.

use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::assets::generated::{FadeTransitionSettings, RtpcCompatibleValue};
use crate::assets::ProjectValidator;
use crate::common::errors::{CliError, codes};
use crate::services::switch_container_entries::{DEFAULT_FADE_FADER, EntrySpec};

use super::sound_args::parse_fader;
use super::switch_container_entry_args::{RawEntry, resolve_object};

const JSON_SYNTAX: &str = "The file holds a list of {\"object\": NAME|kind:NAME|ID, \"switch_states\": [NAME|ID...], \"continue_between_states\": bool, \"fade_in\": {\"duration\": N, \"fader\": NAME}, \"fade_out\": {...}, \"gain\": N|VALUE, \"pitch\": N|VALUE} objects";
const JSON_KEYS: &[&str] = &[
    "object",
    "switch_states",
    "continue_between_states",
    "fade_in",
    "fade_out",
    "gain",
    "pitch",
];

fn json_error(path: &Path, why: impl Into<String>) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Cannot use entries file {}", path.display()),
        why,
    )
    .with_suggestion(JSON_SYNTAX)
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

fn json_fade(path: &Path, key: &str, value: &Value) -> Result<FadeTransitionSettings> {
    if let Some(seconds) = value.as_f64() {
        return Ok(FadeTransitionSettings {
            duration: seconds,
            fader: Some(DEFAULT_FADE_FADER.to_string()),
        });
    }
    let object = value.as_object().ok_or_else(|| {
        json_error(path, format!("'{key}' must be a duration or {{\"duration\": N, \"fader\": NAME}}"))
    })?;
    let duration = match object.get("duration") {
        None => 0.0,
        Some(d) => d
            .as_f64()
            .ok_or_else(|| json_error(path, format!("'{key}.duration' must be a number")))?,
    };
    let fader = match object.get("fader") {
        None => DEFAULT_FADE_FADER.to_string(),
        Some(f) => parse_fader(
            f.as_str()
                .ok_or_else(|| json_error(path, format!("'{key}.fader' must be a name")))?,
        )?,
    };
    if !duration.is_finite() || duration < 0.0 {
        return Err(json_error(path, format!("'{key}.duration' must not be negative")).into());
    }
    Ok(FadeTransitionSettings {
        duration,
        fader: Some(fader),
    })
}

fn json_states(path: &Path, value: Option<&Value>) -> Result<Vec<String>> {
    let items = value
        .and_then(Value::as_array)
        .ok_or_else(|| json_error(path, "every entry needs 'switch_states' (a list of names or IDs)"))?;
    let states = items
        .iter()
        .map(|item| match item {
            Value::String(name) => Ok(name.clone()),
            Value::Number(n) if n.is_u64() => Ok(n.to_string()),
            other => Err(json_error(
                path,
                format!("invalid state {other}: use a name or a non-negative ID"),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if states.is_empty() {
        return Err(json_error(path, "'switch_states' must not be empty").into());
    }
    Ok(states)
}

fn json_entry(path: &Path, item: &Value, validator: &ProjectValidator) -> Result<RawEntry> {
    let object = item
        .as_object()
        .ok_or_else(|| json_error(path, "every entry must be an object"))?;
    if let Some(unknown) = object.keys().find(|k| !JSON_KEYS.contains(&k.as_str())) {
        return Err(json_error(
            path,
            format!("unknown entry key '{unknown}' (valid: {})", JSON_KEYS.join(", ")),
        )
        .into());
    }
    let target = match object.get("object") {
        Some(Value::String(name)) => resolve_object(validator, name)?,
        Some(Value::Number(id)) => id
            .as_u64()
            .ok_or_else(|| json_error(path, "'object' must be a name or a non-negative ID"))?,
        _ => return Err(json_error(path, "every entry needs an 'object' (name or ID)").into()),
    };
    let mut spec = EntrySpec::new(target, Vec::new());
    if let Some(flag) = object.get("continue_between_states") {
        spec.continue_between_states = flag
            .as_bool()
            .ok_or_else(|| json_error(path, "'continue_between_states' must be true or false"))?;
    }
    spec.fade_in = object.get("fade_in").map(|v| json_fade(path, "fade_in", v)).transpose()?;
    spec.fade_out = object.get("fade_out").map(|v| json_fade(path, "fade_out", v)).transpose()?;
    spec.gain = object.get("gain").map(|v| json_value(path, "gain", v)).transpose()?;
    spec.pitch = object.get("pitch").map(|v| json_value(path, "pitch", v)).transpose()?;
    Ok(RawEntry {
        spec,
        states: json_states(path, object.get("switch_states"))?,
    })
}

/// Reads entries from a JSON file: a list of entry objects, or an SDK switch
/// container object whose `entries` holds that list.
pub fn entries_from_json(path: &Path, validator: &ProjectValidator) -> Result<Vec<RawEntry>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| json_error(path, format!("the file cannot be read: {e}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| json_error(path, format!("the file is not valid JSON: {e}")))?;
    let items = match &value {
        Value::Array(items) => items,
        Value::Object(map) => match map.get("entries") {
            Some(Value::Array(items)) => items,
            _ => return Err(json_error(path, "expected a list, or an object with 'entries'").into()),
        },
        _ => return Err(json_error(path, "expected a list of entries").into()),
    };
    items
        .iter()
        .map(|i| json_entry(path, i, validator))
        .collect()
}

