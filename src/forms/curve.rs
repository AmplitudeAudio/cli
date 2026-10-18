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

//! Input helpers for `CurveDefinition` fields: `--curve-part`, `--curve-from-json`
//! and part-by-part interactive entry.

use std::path::Path;

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::extensions::FADER_ALGORITHM_NAMES;
use crate::assets::generated::{CurveDefinition, CurvePartDefinition, CurvePointDefinition};
use crate::common::errors::{CliError, codes::ERR_VALIDATION_FIELD};
use crate::input::Input;

const PART_FORMAT: &str = "start_x,start_y,end_x,end_y,fader";

fn invalid(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(ERR_VALIDATION_FIELD, what, why).with_suggestion(format!(
        "Use the format `{PART_FORMAT}`, e.g. `0,0,1,1,Linear`"
    ))
}

fn parse_number(text: &str, name: &str, whole: &str) -> Result<f64> {
    match text.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(invalid(
            format!("Invalid curve part '{whole}'"),
            format!("{name} must be a finite number (expected format: {PART_FORMAT})"),
        )
        .into()),
    }
}

fn parse_fader(text: &str) -> Result<String> {
    FADER_ALGORITHM_NAMES
        .iter()
        .find(|n| n.eq_ignore_ascii_case(text.trim()))
        .map(|n| n.to_string())
        .ok_or_else(|| {
            invalid(
                format!("Unknown fader '{}'", text.trim()),
                format!(
                    "fader must be one of: {} (expected format: {PART_FORMAT})",
                    FADER_ALGORITHM_NAMES.join(", ")
                ),
            )
            .into()
        })
}

fn build_part(sx: f64, sy: f64, ex: f64, ey: f64, fader: String) -> Result<CurvePartDefinition> {
    if sx >= ex {
        return Err(invalid(
            "Invalid curve part",
            format!("start_x ({sx}) must be less than end_x ({ex})"),
        )
        .into());
    }
    Ok(CurvePartDefinition {
        start: Some(CurvePointDefinition {
            x: sx,
            y: sy as f32,
        }),
        end: Some(CurvePointDefinition {
            x: ex,
            y: ey as f32,
        }),
        fader: Some(fader),
    })
}

/// Parses one `--curve-part start_x,start_y,end_x,end_y,fader` value.
pub fn parse_curve_part(s: &str) -> Result<CurvePartDefinition> {
    let fields: Vec<&str> = s.split(',').collect();
    if fields.len() != 5 {
        return Err(invalid(
            format!("Invalid curve part '{s}'"),
            format!("expected 5 comma-separated fields: {PART_FORMAT}"),
        )
        .into());
    }
    let sx = parse_number(fields[0], "start_x", s)?;
    let sy = parse_number(fields[1], "start_y", s)?;
    let ex = parse_number(fields[2], "end_x", s)?;
    let ey = parse_number(fields[3], "end_y", s)?;
    build_part(sx, sy, ex, ey, parse_fader(fields[4])?)
}

fn finite_point(p: &Option<CurvePointDefinition>, what: &str) -> Result<CurvePointDefinition> {
    match p {
        Some(p) if p.x.is_finite() && p.y.is_finite() => Ok(p.clone()),
        _ => Err(invalid(
            "Invalid curve",
            format!("every part needs a finite {what} point"),
        )
        .into()),
    }
}

/// Checks a whole curve: at least one part, finite points, a known fader on
/// every part, and `start_x < end_x` per part. Order and gaps are valid (the
/// SDK scans the parts linearly; a gap evaluates to 0).
pub fn validate_curve(curve: &CurveDefinition) -> Result<()> {
    let parts = curve.parts.as_deref().unwrap_or(&[]);
    if parts.is_empty() {
        return Err(invalid("Empty curve", "a curve needs at least one part").into());
    }
    for (index, part) in parts.iter().enumerate() {
        let number = index + 1;
        let start = finite_point(&part.start, "start")?;
        let end = finite_point(&part.end, "end")?;
        if start.x >= end.x {
            return Err(invalid(
                format!("Invalid curve part {number}"),
                format!("start_x ({}) must be less than end_x ({})", start.x, end.x),
            )
            .into());
        }
        let valid = part
            .fader
            .as_deref()
            .is_some_and(|f| FADER_ALGORITHM_NAMES.contains(&f));
        if !valid {
            let found = part
                .fader
                .as_deref()
                .map_or("missing".to_string(), |f| format!("'{f}'"));
            return Err(invalid(
                format!("Invalid fader on curve part {number}"),
                format!(
                    "part {number} has a {found} fader; it must be one of: {}",
                    FADER_ALGORITHM_NAMES.join(", ")
                ),
            )
            .into());
        }
    }
    Ok(())
}

/// Builds a curve from repeated `--curve-part` values or a `--curve-from-json` file.
///
/// Neither given yields `Ok(None)`; both given is an error.
pub fn curve_from_flags(
    parts: &[String],
    from_json: Option<&Path>,
) -> Result<Option<CurveDefinition>> {
    let curve = match (parts.is_empty(), from_json) {
        (true, None) => return Ok(None),
        (false, Some(_)) => {
            return Err(CliError::new(
                ERR_VALIDATION_FIELD,
                "Conflicting flags",
                "both --curve-part and --curve-from-json were given; use one of them",
            )
            .into());
        }
        (false, None) => CurveDefinition {
            parts: Some(
                parts
                    .iter()
                    .map(|p| parse_curve_part(p))
                    .collect::<Result<Vec<_>>>()?,
            ),
        },
        (true, Some(path)) => {
            let text = std::fs::read_to_string(path).map_err(|e| {
                CliError::new(
                    ERR_VALIDATION_FIELD,
                    format!("Cannot read curve file {}", path.display()),
                    e.to_string(),
                )
            })?;
            serde_json::from_str::<CurveDefinition>(&text).map_err(|e| {
                CliError::new(
                    ERR_VALIDATION_FIELD,
                    format!("Invalid curve JSON in {}", path.display()),
                    e.to_string(),
                )
            })?
        }
    };
    validate_curve(&curve)?;
    Ok(Some(curve))
}

fn validate_number(text: &str) -> Result<Validation, inquire::CustomUserError> {
    Ok(match text.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => Validation::Valid,
        _ => Validation::Invalid("Enter a valid number".into()),
    })
}

fn ask_number(input: &dyn Input, label: &str) -> Result<f64> {
    let text = input.prompt_text(label, None, None, Some(&validate_number))?;
    parse_number(&text, label, &text)
}

fn prompt_part(input: &dyn Input, number: usize) -> Result<CurvePartDefinition> {
    let start_x = ask_number(input, &format!("Part {number}: start x"))?;
    let start_y = ask_number(input, &format!("Part {number}: start y"))?;
    let end_x = ask_number(input, &format!("Part {number}: end x"))?;
    let end_y = ask_number(input, &format!("Part {number}: end y"))?;
    let faders: Vec<String> = FADER_ALGORITHM_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect();
    let fader = input.select(&format!("Part {number}: fader"), &faders)?;
    build_part(start_x, start_y, end_x, end_y, parse_fader(&fader)?)
}

/// Interactive part-by-part curve entry; loops on "add another part?".
pub fn prompt_curve(input: &dyn Input) -> Result<CurveDefinition> {
    let mut parts: Vec<CurvePartDefinition> = Vec::new();
    loop {
        parts.push(prompt_part(input, parts.len() + 1)?);
        if !input.confirm("Add another curve part?", Some(false))? {
            break;
        }
    }
    let curve = CurveDefinition { parts: Some(parts) };
    validate_curve(&curve)?;
    Ok(curve)
}
