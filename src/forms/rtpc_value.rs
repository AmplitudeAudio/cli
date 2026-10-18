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

//! Input helpers for `RtpcCompatibleValue` fields (static number or RTPC binding).

use anyhow::{Result, anyhow};
use inquire::validator::Validation;

use crate::assets::generated::{
    CurveDefinition, CurvePartDefinition, CurvePointDefinition, RtpcCompatibleValue,
};
use crate::assets::{AssetType, ProjectValidator};
use crate::common::errors::{
    CliError,
    codes::{ERR_VALIDATION_EMPTY_REFERENCE, ERR_VALIDATION_FIELD, ERR_VALIDATION_REFERENCE},
};
use crate::forms::pick::create_command;
use crate::forms::references::RefTarget;
use crate::input::{Input, select_index};

const STATIC_LABEL: &str = "static";
const BIND_LABEL: &str = "bind to RTPC";
const RTPC_TARGET: RefTarget = RefTarget::Asset(AssetType::Rtpc);

/// A linear curve from (0, 0) to (1, 1), the default mapping for a bound RTPC.
fn linear_curve() -> CurveDefinition {
    CurveDefinition {
        parts: Some(vec![CurvePartDefinition {
            start: Some(CurvePointDefinition { x: 0.0, y: 0.0 }),
            end: Some(CurvePointDefinition { x: 1.0, y: 1.0 }),
            fader: Some("Linear".to_string()),
        }]),
    }
}

fn no_rtpc_error() -> CliError {
    let command = create_command(RTPC_TARGET);
    CliError::new(
        ERR_VALIDATION_EMPTY_REFERENCE,
        "No RTPC available",
        format!("the project has no RTPC to bind to; create one with `{command}`"),
    )
    .with_suggestion(format!("Create one first with `{command}`"))
}

fn validate_number(text: &str) -> Result<Validation, inquire::CustomUserError> {
    Ok(match text.trim().parse::<f32>() {
        Ok(v) if v.is_finite() => Validation::Valid,
        _ => Validation::Invalid("Enter a valid number".into()),
    })
}

/// Asks for a value that is either a plain number or bound to an RTPC.
pub fn prompt_rtpc_value(
    input: &dyn Input,
    validator: &ProjectValidator,
    label: &str,
    default: f32,
) -> Result<RtpcCompatibleValue> {
    let modes = [STATIC_LABEL.to_string(), BIND_LABEL.to_string()];
    let mode = select_index(input, &format!("{label}: value type"), &modes)?;

    if mode == 0 {
        let text =
            input.prompt_text_with_default(label, &default.to_string(), Some(&validate_number))?;
        let value = text
            .trim()
            .parse::<f32>()
            .map_err(|_| anyhow!("'{}' is not a valid number for {}", text, label))?;
        return Ok(RtpcCompatibleValue::static_value(value));
    }

    let candidates = validator.list(RTPC_TARGET);
    if candidates.is_empty() {
        return Err(no_rtpc_error().into());
    }
    let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
    let index = select_index(input, &format!("{label}: RTPC to bind"), &names)?;
    Ok(RtpcCompatibleValue::rtpc(
        candidates[index].id,
        linear_curve(),
    ))
}

/// Builds the value for paired `--<field> <n>` / `--<field>-rtpc <name>` flags.
///
/// Neither set yields `Ok(None)`; both set is an error.
pub fn rtpc_value_from_flags(
    value: Option<f32>,
    rtpc_name: Option<&str>,
    validator: &ProjectValidator,
) -> Result<Option<RtpcCompatibleValue>> {
    match (value, rtpc_name) {
        (None, None) => Ok(None),
        (Some(v), None) if !v.is_finite() => Err(CliError::new(
            ERR_VALIDATION_FIELD,
            format!("Invalid value: {v}"),
            "values must be finite numbers; NaN and infinity are not allowed",
        )
        .with_suggestion("Pass an ordinary number such as 0.8")
        .into()),
        (Some(v), None) => Ok(Some(RtpcCompatibleValue::static_value(v))),
        (Some(_), Some(_)) => Err(CliError::new(
            ERR_VALIDATION_FIELD,
            "Conflicting flags",
            "both a static value and an rtpc name were given; pass the value flag or the rtpc flag, not both",
        )
        .into()),
        (None, Some(name)) => {
            let found = validator
                .list(RTPC_TARGET)
                .into_iter()
                .find(|r| r.name == name)
                .ok_or_else(|| {
                    CliError::new(
                        ERR_VALIDATION_REFERENCE,
                        format!("RTPC '{name}' not found"),
                        format!(
                            "no RTPC with that name exists in the project; check the name or create it with `{}`",
                            create_command(RTPC_TARGET)
                        ),
                    )
                })?;
            Ok(Some(RtpcCompatibleValue::rtpc(found.id, linear_curve())))
        }
    }
}
