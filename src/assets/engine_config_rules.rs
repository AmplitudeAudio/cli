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

//! SDK-backed validation of an engine config (`<name>.config.json`).
//!
//! Mirrors what `Engine::Initialize` (`Core/Engine.cpp`) and its helpers need;
//! every rule below is a null dereference, a critical load failure or a type
//! the schema (and `flatc`) rejects. Unknown keys are ignored, and values the
//! SDK tolerates (zero counts, unregistered driver or resampler names) are
//! accepted.

use serde_json::Value;

use super::engine_config_fields::{FIELDS, FieldSpec, Kind, Need};
use crate::assets::ValidationError;
use crate::assets::generated::CurveDefinition;
use crate::forms::curve::validate_curve;

fn violation(what: String, why: &str) -> ValidationError {
    ValidationError::type_rule_violation(what, why)
        .with_suggestion("Fix the engine config (see 'am asset engine-config --help')")
}

fn type_error(spec: &FieldSpec, expected: &str) -> ValidationError {
    violation(
        format!("{} must be {expected} (flag {})", spec.key(), spec.flag()),
        "The schema rejects this type, so the engine config cannot be built",
    )
}

fn lookup<'a>(config: &'a Value, spec: &FieldSpec) -> Option<&'a Value> {
    match spec.section {
        Some(section) => config.get(section)?.get(spec.field),
        None => config.get(spec.field),
    }
}

fn check_enum(options: &[(&str, i64)], value: &Value) -> bool {
    match value {
        Value::String(name) => options.iter().any(|(n, _)| n == name),
        Value::Number(n) => n
            .as_i64()
            .is_some_and(|v| options.iter().any(|(_, o)| *o == v)),
        _ => false,
    }
}

fn check_value(spec: &FieldSpec, value: &Value) -> Result<(), ValidationError> {
    let ok = match spec.kind {
        Kind::Uint => value.as_u64().is_some_and(|v| v <= u64::from(u32::MAX)),
        Kind::Float => value.as_f64().is_some_and(f64::is_finite),
        Kind::Bool => value.is_boolean(),
        Kind::Text => value.is_string(),
        Kind::Enum(options) => check_enum(options, value),
    };
    if ok {
        return Ok(());
    }
    let expected = match spec.kind {
        Kind::Uint => "a whole number from 0 to 4294967295".to_string(),
        Kind::Float => "a number".to_string(),
        Kind::Bool => "true or false".to_string(),
        Kind::Text => "a string".to_string(),
        Kind::Enum(options) => format!(
            "one of: {}",
            super::engine_config_fields::enum_names(options)
        ),
    };
    Err(type_error(spec, &expected))
}

fn require_table<'a>(
    config: &'a Value,
    name: &str,
    why: &str,
) -> Result<&'a Value, ValidationError> {
    config
        .get(name)
        .filter(|v| v.is_object())
        .ok_or_else(|| violation(format!("The '{name}' table is missing"), why))
}

fn require_text(config: &Value, spec: &FieldSpec, why: &str) -> Result<(), ValidationError> {
    match lookup(config, spec).and_then(Value::as_str) {
        Some(text) if !text.is_empty() => Ok(()),
        _ => Err(violation(
            format!("{} is missing or empty (flag {})", spec.key(), spec.flag()),
            why,
        )),
    }
}

fn field(key: &str) -> &'static FieldSpec {
    FIELDS
        .iter()
        .find(|f| f.key() == key)
        .expect("the field table lists this key")
}

fn check_curves(game: &Value) -> Result<(), ValidationError> {
    for table in ["obstruction", "occlusion"] {
        let Some(table_value) = game.get(table) else {
            return Err(violation(
                format!("game.{table} is missing"),
                "The engine reads the table without a null check (EngineInternalState.h: ObstructionOcclusionState::Init), so it must exist; its curves may be left out",
            ));
        };
        if !table_value.is_object() {
            return Err(violation(
                format!("game.{table} must be a table"),
                "Schema type",
            ));
        }
        for curve_name in ["lpf_curve", "gain_curve"] {
            let Some(curve) = table_value.get(curve_name) else {
                continue;
            };
            let parsed: CurveDefinition = serde_json::from_value(curve.clone()).map_err(|e| {
                violation(
                    format!("game.{table}.{curve_name} is not a curve: {e}"),
                    "Schema type",
                )
            })?;
            validate_curve(&parsed).map_err(|e| {
                violation(
                    format!("game.{table}.{curve_name}: {e}"),
                    "The curve is not usable",
                )
            })?;
        }
    }
    Ok(())
}

/// Checks that the SDK can start an engine with `config`.
///
/// Rules, each backed by the SDK source:
/// - `output`, `mixer`, `game` tables exist and `buses_file` is a non-empty
///   string (dereferenced without checks in `Engine::Initialize`, `Amplimix::Init`);
/// - `mixer.pipeline` is non-empty (critical "no pipeline is configured");
/// - `game.obstruction` and `game.occlusion` exist (`ObstructionOcclusionState::Init`
///   reads their curves unguarded); their curves, when present, are usable;
/// - `hrtf`, when present, has a non-empty `amir_file` (dereferenced), and a
///   `mixer.panning_mode` other than `Stereo` needs `hrtf` (critical failure);
/// - every present field has the schema type (and enum names/values flatc accepts).
pub fn validate_engine_config(config: &Value) -> Result<(), ValidationError> {
    if !config.is_object() {
        return Err(violation(
            "The engine config is not a JSON object".into(),
            "Schema root",
        ));
    }
    require_table(
        config,
        "output",
        "Schema (required); the mixer reads output.buffer_size without a null check",
    )?;
    require_table(
        config,
        "mixer",
        "Schema (required); the engine reads mixer.* without a null check",
    )?;
    let game = require_table(
        config,
        "game",
        "Schema (required); the engine reads game.* without a null check",
    )?;
    if let Some(hrtf) = config.get("hrtf")
        && !hrtf.is_object()
    {
        return Err(violation("hrtf must be a table".into(), "Schema type"));
    }
    require_text(
        config,
        field("buses_file"),
        "Engine::Initialize dereferences buses_file to load the bus file",
    )?;
    require_text(
        config,
        field("mixer.pipeline"),
        "Engine::Initialize fails with 'no pipeline is configured'",
    )?;

    for spec in FIELDS {
        if let Some(value) = lookup(config, spec) {
            check_value(spec, value)?;
        }
    }

    let hrtf_present = config.get("hrtf").is_some();
    if hrtf_present {
        let spec = field("hrtf.amir_file");
        debug_assert_eq!(spec.need, Need::WithSection);
        require_text(
            config,
            spec,
            "Engine::Initialize dereferences hrtf.amir_file when an hrtf table exists",
        )?;
    }
    let stereo = lookup(config, field("mixer.panning_mode"))
        .is_none_or(|v| matches!(v.as_str(), Some("Stereo")) || v.as_i64() == Some(0));
    if !stereo && !hrtf_present {
        return Err(violation(
            "mixer.panning_mode is not Stereo but hrtf.amir_file is missing (flag --hrtf-amir-file)".into(),
            "Engine::Initialize refuses to start: the HRTF configuration is missing but the panning mode is not stereo",
        ));
    }
    check_curves(game)
}
