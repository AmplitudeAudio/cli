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

//! Engine config creation and update, independent of how values were gathered.
//!
//! Values arrive as `(dotted key, JSON value)` pairs (see
//! [`assets::engine_config_fields`](crate::assets::engine_config_fields)); an
//! optional JSON document provides a whole config (create) or whole sections
//! (update) that the pairs then override. Every function returns a new,
//! validated value; inputs are never mutated and nothing touches the disk.

use serde_json::{Map, Value, json};

use crate::assets::engine_config::validate_engine_config;
use crate::assets::engine_config_fields::{
    FIELDS, FieldSpec, Need, buses_ref_name, find_field, pipeline_ref_name,
};
use crate::common::errors::{CliError, codes};

/// The sections of the engine config in schema order, then the top-level keys.
const KEY_ORDER: &[&str] = &["output", "mixer", "hrtf", "game", "buses_file", "driver"];

/// Curves used for `game.obstruction` / `game.occlusion` when none are given.
///
/// The schema has no default, but the SDK dereferences both tables, so they
/// must exist. These are the curves of the default project template.
fn default_curves(table: &str) -> Value {
    let part = |y0: f64, y1: f64| {
        json!({"parts": [{
            "start": {"x": 0, "y": y0},
            "end": {"x": 1, "y": y1},
            "fader": "Linear",
        }]})
    };
    match table {
        "obstruction" => json!({"lpf_curve": part(0.0, 1.0), "gain_curve": part(1.0, 1.0)}),
        _ => json!({"lpf_curve": part(0.0, 1.0), "gain_curve": part(1.0, 0.0)}),
    }
}

fn field_error(what: String, why: String, suggestion: String) -> CliError {
    CliError::new(codes::ERR_VALIDATION_FIELD, what, why).with_suggestion(suggestion)
}

fn as_object(value: Value) -> Result<Map<String, Value>, CliError> {
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(field_error(
            "The engine config is not a JSON object".into(),
            "An engine config file holds one JSON object".into(),
            "Check the file passed to --from-json".into(),
        )),
    }
}

/// Sets (or, for `Null`, removes) the field `key` in `config`, creating its section.
fn set_field(config: &mut Map<String, Value>, spec: &FieldSpec, value: &Value) {
    let target = match spec.section {
        Some(section) => {
            let entry = config
                .entry(section.to_string())
                .or_insert_with(|| json!({}));
            if !entry.is_object() {
                *entry = json!({});
            }
            let Some(map) = entry.as_object_mut() else {
                return;
            };
            map
        }
        None => config,
    };
    if value.is_null() {
        target.remove(spec.field);
    } else {
        target.insert(spec.field.to_string(), value.clone());
    }
}

fn apply_overrides(config: &mut Map<String, Value>, set: &[(&str, Value)]) -> Result<(), CliError> {
    for (key, value) in set {
        let spec = find_field(key).ok_or_else(|| {
            field_error(
                format!("Unknown engine config field '{key}'"),
                "No such field in engine_config_definition.fbs".into(),
                "Use a field listed in 'am asset engine-config create --help'".into(),
            )
        })?;
        set_field(config, spec, value);
    }
    Ok(())
}

fn section_present(config: &Map<String, Value>, spec: &FieldSpec) -> bool {
    spec.section.is_none_or(|s| config.contains_key(s))
}

fn has_value(config: &Map<String, Value>, spec: &FieldSpec) -> bool {
    let holder = match spec.section {
        Some(section) => config.get(section).and_then(Value::as_object),
        None => Some(config),
    };
    holder
        .and_then(|h| h.get(spec.field))
        .is_some_and(|v| !v.is_null())
}

fn fill_defaults(config: &mut Map<String, Value>) {
    for section in ["output", "mixer", "game"] {
        config
            .entry(section.to_string())
            .or_insert_with(|| json!({}));
    }
    for spec in FIELDS {
        let Some(default) = spec.default else {
            continue;
        };
        if !section_present(config, spec) || has_value(config, spec) {
            continue;
        }
        if let Ok(value) = crate::assets::engine_config_fields::parse_value(spec, default) {
            set_field(config, spec, &value);
        }
    }
    if let Some(game) = config.get_mut("game").and_then(Value::as_object_mut) {
        for table in ["obstruction", "occlusion"] {
            game.entry(table.to_string())
                .or_insert_with(|| default_curves(table));
        }
    }
}

/// Flags of required fields that no value was supplied for.
fn missing_flags(config: &Map<String, Value>) -> Vec<String> {
    let hrtf_needed = config.contains_key("hrtf")
        || config
            .get("mixer")
            .and_then(|m| m.get("panning_mode"))
            .is_some_and(|m| m.as_str() != Some("Stereo") && m.as_i64() != Some(0));
    FIELDS
        .iter()
        .filter(|spec| match spec.need {
            Need::Always => !has_value(config, spec),
            Need::WithSection => hrtf_needed && !has_value(config, spec),
            Need::Optional => false,
        })
        .map(FieldSpec::flag)
        .collect()
}

fn ordered(config: Map<String, Value>) -> Map<String, Value> {
    let mut rest = config;
    let mut out = Map::new();
    for key in KEY_ORDER {
        if let Some(value) = rest.remove(*key) {
            out.insert((*key).to_string(), value);
        }
    }
    out.extend(rest);
    out
}

fn validated(config: Value) -> Result<Value, CliError> {
    validate_engine_config(&config).map_err(CliError::from)?;
    Ok(config)
}

/// Builds a new engine config.
///
/// `base` (from `--from-json`) is the starting point and `set` overrides it.
/// Absent optional fields get their schema default; required fields that
/// nothing supplied fail the call, naming their flags.
pub fn build_config(base: Option<&Value>, set: &[(&str, Value)]) -> Result<Value, CliError> {
    let mut config = match base {
        Some(base) => as_object(base.clone())?,
        None => Map::new(),
    };
    apply_overrides(&mut config, set)?;
    fill_defaults(&mut config);

    let missing = missing_flags(&config);
    if !missing.is_empty() {
        return Err(field_error(
            format!("Missing required {}", missing.join(", ")),
            "These engine config fields have no usable default and cannot be prompted for in non-interactive or JSON mode".into(),
            format!("Pass {} (see 'am asset engine-config create --help'), or supply them with --from-json", missing.join(", ")),
        ));
    }
    let config = if base.is_none() {
        ordered(config)
    } else {
        config
    };
    validated(Value::Object(config))
}

/// Applies an update to `existing`.
///
/// Top-level keys of `sections` (from `--from-json`) replace the same keys of
/// `existing` as a whole, then `set` changes single fields (a `Null` value
/// removes the field). Everything else, including keys the CLI does not
/// model, keeps its value and position. The result must be valid.
pub fn apply_update(
    existing: &Value,
    sections: Option<&Value>,
    set: &[(&str, Value)],
) -> Result<Value, CliError> {
    let mut config = as_object(existing.clone())?;
    if let Some(sections) = sections {
        for (key, value) in as_object(sections.clone())? {
            config.insert(key, value);
        }
    }
    apply_overrides(&mut config, set)?;
    validated(Value::Object(config))
}

/// Checks that `mixer.pipeline` and `buses_file` name existing project files.
pub fn check_references(
    config: &Value,
    pipeline_names: &[String],
    bus_file_names: &[String],
) -> Result<(), CliError> {
    match reference_problems(config, pipeline_names, bus_file_names)
        .into_iter()
        .next()
    {
        Some(first) => Err(first),
        None => Ok(()),
    }
}

/// Every dangling reference of `config` (`mixer.pipeline`, then `buses_file`).
pub fn reference_problems(
    config: &Value,
    pipeline_names: &[String],
    bus_file_names: &[String],
) -> Vec<CliError> {
    let mut problems = Vec::new();
    let pipeline = config
        .get("mixer")
        .and_then(|m| m.get("pipeline"))
        .and_then(Value::as_str)
        .map(pipeline_ref_name);
    if let Some(name) = pipeline.filter(|n| !pipeline_names.iter().any(|p| p == n)) {
        problems.push(CliError::new(
            codes::ERR_VALIDATION_REFERENCE,
            format!("No pipeline named '{name}'"),
            format!(
                "--mixer-pipeline must name an existing pipeline asset; available: {}",
                list_or_none(pipeline_names)
            ),
        )
        .with_suggestion("Create one with 'am asset pipeline create', or pick an existing one"));
    }
    let buses = config
        .get("buses_file")
        .and_then(Value::as_str)
        .map(buses_ref_name);
    if let Some(name) = buses.filter(|n| !bus_file_names.iter().any(|f| f == n)) {
        problems.push(CliError::new(
            codes::ERR_VALIDATION_REFERENCE,
            format!("No bus file named '{name}'"),
            format!(
                "--buses-file must name an existing '*.buses.json' file; available: {}",
                list_or_none(bus_file_names)
            ),
        )
        .with_suggestion("Create one with 'am asset bus-file create <file-name>'"));
    }
    problems
}

fn list_or_none(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// Non-blocking warnings about a valid config.
pub fn config_warnings(config: &Value) -> Vec<String> {
    let channels = config
        .get("mixer")
        .and_then(|m| m.get("active_channels"))
        .and_then(Value::as_u64);
    if channels.unwrap_or(0) == 0 {
        return vec![
            "mixer.active_channels is 0: the engine will have no audible channels (set --mixer-active-channels)"
                .to_string(),
        ];
    }
    Vec::new()
}

/// The key fields of a config, for listings.
pub fn summarize(config: &Value) -> Value {
    let get = |path: &[&str]| {
        path.iter()
            .try_fold(config, |value, key| value.get(*key))
            .cloned()
            .unwrap_or(Value::Null)
    };
    json!({
        "pipeline": get(&["mixer", "pipeline"]),
        "buses_file": get(&["buses_file"]),
        "driver": get(&["driver"]),
        "frequency": get(&["output", "frequency"]),
        "channels": get(&["output", "channels"]),
        "active_channels": get(&["mixer", "active_channels"]),
        "virtual_channels": get(&["mixer", "virtual_channels"]),
        "panning_mode": get(&["mixer", "panning_mode"]),
    })
}
