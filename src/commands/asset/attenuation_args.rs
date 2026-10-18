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

//! Command-line flags of `attenuation create` / `attenuation update` and their
//! resolution into an [`AttenuationPatch`] (numbers checked, curves and zones built).

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::assets::{AIR_ABSORPTION_BANDS, Attenuation};
use crate::common::errors::{CliError, codes};
use crate::forms::curve::curve_from_flags;
use crate::forms::zone::{ZoneArgs, zone_from_flags, zone_shape_name, zone_to_args};
use crate::services::attenuation::{AirAbsorptionPatch, AttenuationPatch};

/// Every attenuation flag; shared by `create` and `update`.
#[derive(Args, Debug, Default, Clone)]
pub struct AttenuationArgs {
    /// Distance at which the sound reaches a gain of 0 (>= 0)
    #[arg(long)]
    pub max_distance: Option<f64>,

    #[command(flatten)]
    pub zone: ZoneArgs,

    /// Maximum attenuation factor applied inside the zone, in [0..1] (default: 1)
    #[arg(long)]
    pub max_attenuation_factor: Option<f32>,

    /// Gain curve part `start_x,start_y,end_x,end_y,fader`; repeat for more parts
    #[arg(long, allow_hyphen_values = true)]
    pub curve_part: Vec<String>,

    /// Read the gain curve from a JSON file instead of --curve-part
    #[arg(long)]
    pub curve_from_json: Option<PathBuf>,

    /// Spread curve part `start_x,start_y,end_x,end_y,fader`; repeat for more parts
    #[arg(long, allow_hyphen_values = true)]
    pub spread_curve_part: Vec<String>,

    /// Read the spread curve from a JSON file instead of --spread-curve-part
    #[arg(long)]
    pub spread_curve_from_json: Option<PathBuf>,

    /// Enable or disable air absorption (true/false)
    #[arg(long)]
    pub air_absorption: Option<bool>,

    /// Air absorption coefficients for the 3 EQ bands: a,b,c
    #[arg(long, allow_hyphen_values = true)]
    pub air_absorption_coefficients: Option<String>,
}

fn field_error(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(codes::ERR_VALIDATION_FIELD, what, why)
}

fn check_max_distance(distance: f64) -> Result<f64> {
    if distance.is_finite() && distance >= 0.0 {
        return Ok(distance);
    }
    Err(field_error(
        format!("Invalid --max-distance: {distance}"),
        "The maximum distance must be a finite number that is not negative",
    )
    .with_suggestion("Use --max-distance with a value >= 0, e.g. --max-distance 50")
    .into())
}

fn check_factor(factor: f32) -> Result<f32> {
    if factor.is_finite() && (0.0..=1.0).contains(&factor) {
        return Ok(factor);
    }
    Err(field_error(
        format!("Invalid --max-attenuation-factor: {factor}"),
        "The maximum attenuation factor must be in the range [0..1]",
    )
    .with_suggestion("Use --max-attenuation-factor with a value between 0 and 1")
    .into())
}

/// Parses `--air-absorption-coefficients a,b,c`: exactly 3 finite, non-negative values.
pub fn parse_coefficients(text: &str) -> Result<Vec<f32>> {
    let invalid = |why: String| -> anyhow::Error {
        field_error("Invalid --air-absorption-coefficients", why)
            .with_suggestion("Pass exactly 3 comma-separated numbers, e.g. 0.0002,0.0017,0.0182")
            .into()
    };
    let values = text
        .split(',')
        .map(|part| match part.trim().parse::<f32>() {
            Ok(v) if v.is_finite() && v >= 0.0 => Ok(v),
            _ => Err(invalid(format!(
                "'{}' is not a finite, non-negative number",
                part.trim()
            ))),
        })
        .collect::<Result<Vec<_>>>()?;
    if values.len() != AIR_ABSORPTION_BANDS {
        return Err(invalid(format!(
            "expected exactly {AIR_ABSORPTION_BANDS} coefficients, got {}",
            values.len()
        )));
    }
    Ok(values)
}

/// The zone flags to build from: on update, dimensions the user left out come
/// from the existing zone when the shape is unchanged.
fn effective_zone_args(args: &ZoneArgs, existing: Option<&Attenuation>) -> ZoneArgs {
    let base = existing
        .and_then(|a| a.shape.as_ref())
        .and_then(|s| s.zone.as_ref());
    let Some(base) = base else {
        return args.clone();
    };
    let same_shape = args
        .shape
        .as_deref()
        .is_none_or(|s| s.trim().eq_ignore_ascii_case(zone_shape_name(base)));
    if !same_shape {
        return args.clone();
    }
    let mut merged = args.merged_over(&zone_to_args(base));
    merged.shape = merged
        .shape
        .or_else(|| Some(zone_shape_name(base).to_string()));
    merged
}

/// Turns the raw flags into a patch. `existing` is the attenuation being
/// updated (`None` on create); it only supplies zone dimensions the flags omit.
pub fn resolve_patch(
    args: &AttenuationArgs,
    existing: Option<&Attenuation>,
) -> Result<AttenuationPatch> {
    let zone = args
        .zone
        .is_set()
        .then(|| zone_from_flags(&effective_zone_args(&args.zone, existing)))
        .transpose()?;
    let coefficients = args
        .air_absorption_coefficients
        .as_deref()
        .map(parse_coefficients)
        .transpose()?;

    Ok(AttenuationPatch {
        max_distance: args.max_distance.map(check_max_distance).transpose()?,
        zone,
        max_attenuation_factor: args.max_attenuation_factor.map(check_factor).transpose()?,
        gain_curve: curve_from_flags(&args.curve_part, args.curve_from_json.as_deref())?,
        spread_curve: curve_from_flags(
            &args.spread_curve_part,
            args.spread_curve_from_json.as_deref(),
        )?,
        air_absorption: AirAbsorptionPatch {
            enabled: args.air_absorption,
            coefficients,
        },
    })
}
