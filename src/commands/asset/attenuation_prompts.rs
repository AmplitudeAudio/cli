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

//! Interactive prompts for `attenuation create` and `attenuation update`.
//!
//! Every function here is only called when the session can prompt; values
//! already present in the patch are never asked for again.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::Attenuation;
use crate::forms::curve::prompt_curve;
use crate::forms::zone::prompt_zone;
use crate::input::Input;
use crate::services::attenuation::{
    AirAbsorptionPatch, AttenuationPatch, DEFAULT_AIR_ABSORPTION_COEFFICIENTS,
    DEFAULT_MAX_ATTENUATION_FACTOR,
};

use super::attenuation_args::parse_coefficients;

const ACCEPT_DEFAULTS_PROMPT: &str = "Accept defaults for remaining optional fields?";
const MAX_DISTANCE_LABEL: &str = "Max distance";
const FACTOR_LABEL: &str = "Max attenuation factor [0-1]";

fn number_validator(
    low: f64,
    high: f64,
) -> impl Fn(&str) -> Result<Validation, inquire::CustomUserError> {
    move |text: &str| {
        Ok(match text.trim().parse::<f64>() {
            Ok(v) if v.is_finite() && (low..=high).contains(&v) => Validation::Valid,
            _ => Validation::Invalid(format!("Enter a number between {low} and {high}").into()),
        })
    }
}

fn ask_number(input: &dyn Input, label: &str, low: f64, high: f64) -> Result<f64> {
    let validator = number_validator(low, high);
    let text = input.prompt_text(label, None, None, Some(&validator))?;
    Ok(text.trim().parse()?)
}

fn ask_number_with_default(input: &dyn Input, label: &str, default: f64, high: f64) -> Result<f64> {
    let validator = number_validator(0.0, high);
    let text = input.prompt_text_with_default(label, &default.to_string(), Some(&validator))?;
    Ok(text.trim().parse()?)
}

/// Asks for the maximum distance of a new attenuation.
pub fn prompt_max_distance(input: &dyn Input) -> Result<f64> {
    ask_number(input, MAX_DISTANCE_LABEL, 0.0, f64::MAX)
}

fn prompt_coefficients(input: &dyn Input) -> Result<Vec<f32>> {
    let defaults = DEFAULT_AIR_ABSORPTION_COEFFICIENTS;
    if input.confirm("Use the SDK default absorption coefficients?", Some(true))? {
        return Ok(defaults.to_vec());
    }
    let text = (1..=defaults.len())
        .map(|band| {
            ask_number(
                input,
                &format!("Air absorption coefficient {band}"),
                0.0,
                f64::MAX,
            )
            .map(|v| v.to_string())
        })
        .collect::<Result<Vec<_>>>()?
        .join(",");
    parse_coefficients(&text)
}

fn prompt_air_absorption(input: &dyn Input) -> Result<AirAbsorptionPatch> {
    let enabled = input.confirm("Enable air absorption?", Some(true))?;
    let coefficients = if enabled {
        Some(prompt_coefficients(input)?)
    } else {
        None
    };
    Ok(AirAbsorptionPatch {
        enabled: Some(enabled),
        coefficients,
    })
}

/// Fills the required values (distance, zone, gain curve) no flag provided.
pub fn complete_required(input: &dyn Input, patch: AttenuationPatch) -> Result<AttenuationPatch> {
    let max_distance = match patch.max_distance {
        Some(distance) => distance,
        None => prompt_max_distance(input)?,
    };
    let zone = match patch.zone {
        Some(zone) => zone,
        None => prompt_zone(input)?,
    };
    let gain_curve = match patch.gain_curve {
        Some(curve) => curve,
        None => prompt_curve(input)?,
    };
    Ok(AttenuationPatch {
        max_distance: Some(max_distance),
        zone: Some(zone),
        gain_curve: Some(gain_curve),
        ..patch
    })
}

fn is_walkthrough_needed(patch: &AttenuationPatch) -> bool {
    patch.max_attenuation_factor.is_none()
        || patch.spread_curve.is_none()
        || patch.air_absorption.enabled.is_none() && patch.air_absorption.coefficients.is_none()
}

/// Walks a new attenuation through every optional field no flag already set.
///
/// The author may accept defaults for all of them in a single answer.
pub fn complete_optional(input: &dyn Input, patch: AttenuationPatch) -> Result<AttenuationPatch> {
    if !is_walkthrough_needed(&patch) || input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))? {
        return Ok(patch);
    }

    let max_attenuation_factor = match patch.max_attenuation_factor {
        Some(factor) => Some(factor),
        None => Some(ask_number_with_default(
            input,
            FACTOR_LABEL,
            f64::from(DEFAULT_MAX_ATTENUATION_FACTOR),
            1.0,
        )? as f32),
    };
    let spread_curve = match patch.spread_curve {
        Some(curve) => Some(curve),
        None if input.confirm("Add a spread curve?", Some(false))? => Some(prompt_curve(input)?),
        None => None,
    };
    let air_given =
        patch.air_absorption.enabled.is_some() || patch.air_absorption.coefficients.is_some();
    let air_absorption = if air_given {
        patch.air_absorption
    } else if input.confirm("Configure air absorption?", Some(false))? {
        prompt_air_absorption(input)?
    } else {
        AirAbsorptionPatch::default()
    };

    Ok(AttenuationPatch {
        max_attenuation_factor,
        spread_curve,
        air_absorption,
        ..patch
    })
}

/// Offers every field of an existing attenuation; unchanged answers add nothing to the patch.
pub fn prompt_update_patch(input: &dyn Input, existing: &Attenuation) -> Result<AttenuationPatch> {
    let distance =
        ask_number_with_default(input, MAX_DISTANCE_LABEL, existing.max_distance, f64::MAX)?;
    let current_factor = existing
        .shape
        .as_ref()
        .map_or(DEFAULT_MAX_ATTENUATION_FACTOR, |s| s.max_attenuation_factor);
    let factor = ask_number_with_default(input, FACTOR_LABEL, f64::from(current_factor), 1.0)?;

    let zone = if input.confirm("Change the zone?", Some(false))? {
        Some(prompt_zone(input)?)
    } else {
        None
    };
    let gain_curve = if input.confirm("Replace the gain curve?", Some(false))? {
        Some(prompt_curve(input)?)
    } else {
        None
    };
    let spread_curve = if input.confirm("Replace the spread curve?", Some(false))? {
        Some(prompt_curve(input)?)
    } else {
        None
    };
    let air_absorption = if input.confirm("Change air absorption?", Some(false))? {
        prompt_air_absorption(input)?
    } else {
        AirAbsorptionPatch::default()
    };

    Ok(AttenuationPatch {
        max_distance: (distance != existing.max_distance).then_some(distance),
        zone,
        max_attenuation_factor: ((factor as f32) != current_factor).then_some(factor as f32),
        gain_curve,
        spread_curve,
        air_absorption,
    })
}
