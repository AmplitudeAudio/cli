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

//! Command-line flags of `effect create` / `effect update` and their resolution
//! into an [`EffectPatch`] (RTPC names looked up, numbers and files checked).

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use serde_json::Value;

use crate::assets::generated::ValueKind;
use crate::assets::{ProjectValidator, RtpcCompatibleValue};
use crate::common::errors::{CliError, codes};
use crate::forms::references::EFFECT_PARAMETER_RULE;
use crate::forms::rtpc_value::linear_curve;
use crate::forms::validate_refs::validate_reference;
use crate::services::effect::{EffectPatch, ParamsPatch};

use super::sound_args::resolve_reference;

/// Prefix that marks a parameter bound to an RTPC in `--add-param` and JSON strings.
const RTPC_PREFIX: &str = "rtpc:";

/// Parameter flags shared by `create` and `update` beyond the original `--param`.
#[derive(Args, Debug, Default, Clone)]
pub struct EffectExtraArgs {
    /// Bind the parameter at INDEX (0-based) to an RTPC, by name or ID; INDEX may be the
    /// current length to append. Applied after --param; repeatable
    #[arg(long, value_name = "INDEX=RTPC")]
    pub param_rtpc: Vec<String>,

    /// Read the parameters from a JSON file instead of --param: a list of numbers,
    /// "rtpc:NAME" strings or {"kind": "Static"|"RTPC", ...} value objects, or an SDK effect
    /// object with a "parameters" list
    #[arg(long, value_name = "FILE")]
    pub parameters_from_json: Option<PathBuf>,
}

/// Parameter edits only `update` offers.
#[derive(Args, Debug, Default, Clone)]
pub struct EffectEditArgs {
    /// Append a parameter: a number, or rtpc:NAME to bind it to an RTPC; repeatable
    #[arg(long, value_name = "VALUE|rtpc:NAME")]
    pub add_param: Vec<String>,

    /// Remove the parameter at the given index (0-based); repeatable
    #[arg(long, value_name = "INDEX")]
    pub remove_param: Vec<usize>,

    /// Remove every parameter
    #[arg(long)]
    pub clear_params: bool,
}

/// Every flag of `effect create` / `effect update`, in one place.
#[derive(Debug, Default, Clone)]
pub struct EffectFlags {
    pub effect_type: Option<String>,
    pub param: Option<Vec<f32>>,
    pub extra: EffectExtraArgs,
    pub edit: EffectEditArgs,
}

const SYNTAX: &str = "A parameter is a number (0.5) or rtpc:NAME to bind it to an RTPC";

fn field_error(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(codes::ERR_VALIDATION_FIELD, what, why).with_suggestion(SYNTAX)
}

fn parse_static(text: &str, context: &str) -> Result<RtpcCompatibleValue> {
    match text.trim().parse::<f32>() {
        Ok(v) if v.is_finite() => Ok(RtpcCompatibleValue::static_value(v)),
        _ => Err(field_error(
            format!("Invalid parameter '{text}' in {context}"),
            "a parameter must be a finite number or rtpc:NAME; NaN and infinity are not allowed",
        )
        .into()),
    }
}

fn bind(validator: &ProjectValidator, rtpc: &str) -> Result<RtpcCompatibleValue> {
    let id = resolve_reference(validator, &EFFECT_PARAMETER_RULE, rtpc.trim())?;
    Ok(RtpcCompatibleValue::rtpc(id, linear_curve()))
}

/// Parses one `--add-param` value: a number or `rtpc:NAME`.
pub fn parse_param(raw: &str, validator: &ProjectValidator) -> Result<RtpcCompatibleValue> {
    match raw.trim().strip_prefix(RTPC_PREFIX) {
        Some(name) if name.trim().is_empty() => Err(field_error(
            format!("Parameter '{raw}' names no RTPC"),
            "after rtpc: write the name or ID of the RTPC",
        )
        .into()),
        Some(name) => bind(validator, name),
        None => parse_static(raw, "--add-param"),
    }
}

/// Parses one `--param-rtpc` value: `INDEX=RTPC`.
pub fn parse_param_rtpc(
    raw: &str,
    validator: &ProjectValidator,
) -> Result<(usize, RtpcCompatibleValue)> {
    let bad = |why: &str| {
        field_error(
            format!("Invalid --param-rtpc '{raw}'"),
            format!("{why}; the syntax is INDEX=RTPC (for example 0=speed)"),
        )
    };
    let (index, name) = raw.split_once('=').ok_or_else(|| bad("there is no '='"))?;
    let index = index
        .trim()
        .parse::<usize>()
        .map_err(|_| bad("INDEX must be a non-negative whole number"))?;
    if name.trim().is_empty() {
        return Err(bad("the RTPC name is empty").into());
    }
    Ok((index, bind(validator, name)?))
}

fn json_error(path: &Path, why: impl Into<String>) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Cannot use parameters file {}", path.display()),
        why,
    )
    .with_suggestion(
        "The file holds a list of numbers, \"rtpc:NAME\" strings or {\"kind\": \"Static\", \"value\": N} / {\"kind\": \"RTPC\", \"rtpc\": {...}} objects",
    )
}

fn json_param(
    path: &Path,
    item: &Value,
    validator: &ProjectValidator,
) -> Result<RtpcCompatibleValue> {
    match item {
        Value::Number(n) => parse_static(&n.to_string(), &path.display().to_string()),
        Value::String(text) if text.trim().starts_with(RTPC_PREFIX) => parse_param(text, validator),
        Value::String(text) => Err(json_error(
            path,
            format!("invalid parameter \"{text}\": use a number or \"rtpc:NAME\""),
        )
        .into()),
        Value::Object(map) => {
            if !matches!(
                map.get("kind").and_then(|k| k.as_str()),
                Some("Static" | "RTPC")
            ) {
                return Err(json_error(
                    path,
                    "a parameter object needs a 'kind' of \"Static\" or \"RTPC\"",
                )
                .into());
            }
            // The SDK ignores `value` of an RTPC-kind value, so a file may leave it out.
            let mut object = map.clone();
            object.entry("value").or_insert(Value::from(0.0));
            let value: RtpcCompatibleValue = serde_json::from_value(Value::Object(object))
                .map_err(|e| json_error(path, format!("invalid parameter object: {e}")))?;
            if value.kind == ValueKind::RTPC && value.rtpc.is_none() {
                return Err(json_error(
                    path,
                    "an RTPC parameter needs an 'rtpc' object with its id",
                )
                .into());
            }
            Ok(value)
        }
        other => Err(json_error(path, format!("invalid parameter {other}")).into()),
    }
}

/// Reads parameters from a JSON file: a list of parameters, or an SDK effect object
/// whose `parameters` holds that list.
pub fn parameters_from_json(
    path: &Path,
    validator: &ProjectValidator,
) -> Result<Vec<RtpcCompatibleValue>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| json_error(path, format!("the file cannot be read: {e}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| json_error(path, format!("the file is not valid JSON: {e}")))?;
    let items = match &value {
        Value::Array(items) => items,
        Value::Object(map) => match map.get("parameters") {
            Some(Value::Array(items)) => items,
            _ => {
                return Err(
                    json_error(path, "expected a list, or an object with 'parameters'").into(),
                );
            }
        },
        _ => return Err(json_error(path, "expected a list of parameters").into()),
    };
    items
        .iter()
        .map(|item| json_param(path, item, validator))
        .collect()
}

fn parse_effect_type(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Effect type cannot be whitespace-only",
            "Provide the name of a registered filter or omit the flag",
        )
        .with_suggestion("Use --effect-type with a filter such as Delay or Equalizer")
        .into());
    }
    Ok(trimmed.to_string())
}

fn replace_list(
    flags: &EffectFlags,
    validator: &ProjectValidator,
) -> Result<Option<Vec<RtpcCompatibleValue>>> {
    let sources = [
        flags.param.is_some(),
        flags.extra.parameters_from_json.is_some(),
        flags.edit.clear_params,
    ];
    if sources.iter().filter(|set| **set).count() > 1 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Conflicting parameter flags",
            "--param, --parameters-from-json and --clear-params each replace the whole parameter list",
        )
        .with_suggestion("Pass only one of them; use --param-rtpc / --add-param / --remove-param for incremental edits")
        .into());
    }
    if let Some(path) = &flags.extra.parameters_from_json {
        return parameters_from_json(path, validator).map(Some);
    }
    if let Some(values) = &flags.param {
        return values
            .iter()
            .map(|v| parse_static(&v.to_string(), "--param"))
            .collect::<Result<Vec<_>>>()
            .map(Some);
    }
    Ok(flags.edit.clear_params.then(Vec::new))
}

/// Turns the raw flags into a patch: RTPC names become ids and numbers and files are
/// checked. Fails fast on the first unusable value.
pub fn resolve_patch(flags: &EffectFlags, validator: &ProjectValidator) -> Result<EffectPatch> {
    Ok(EffectPatch {
        effect: flags
            .effect_type
            .as_deref()
            .map(parse_effect_type)
            .transpose()?,
        parameters: ParamsPatch {
            replace: replace_list(flags, validator)?,
            bind: flags
                .extra
                .param_rtpc
                .iter()
                .map(|raw| parse_param_rtpc(raw, validator))
                .collect::<Result<_>>()?,
            remove: flags.edit.remove_param.clone(),
            add: flags
                .edit
                .add_param
                .iter()
                .map(|raw| parse_param(raw, validator))
                .collect::<Result<_>>()?,
        },
    })
}

/// Checks the RTPC bindings a patch brings in against the project, so a dangling id is
/// reported before the user is asked anything else.
pub fn validate_patch_references(patch: &EffectPatch, validator: &ProjectValidator) -> Result<()> {
    for value in patch.parameters.incoming() {
        if let (ValueKind::RTPC, Some(rtpc)) = (value.kind, value.rtpc.as_ref()) {
            validate_reference(&EFFECT_PARAMETER_RULE, rtpc.id, validator)?;
        }
    }
    Ok(())
}
