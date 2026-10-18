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

//! Interactive prompts for `rtpc create` and `rtpc update`.
//!
//! Every function here is only called when the session can prompt; values
//! already present in the patch are never asked for again.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::Rtpc;
use crate::assets::extensions::FADER_ALGORITHM_NAMES;
use crate::input::Input;
use crate::services::rtpc::{DEFAULT_FADER, FadePatch, RtpcPatch};

const ACCEPT_DEFAULTS_PROMPT: &str = "Accept defaults for remaining optional fields?";
const MIN_LABEL: &str = "Min value";
const MAX_LABEL: &str = "Max value";
const DEFAULT_LABEL: &str = "Default value";

fn validate_finite(text: &str) -> Result<Validation, inquire::CustomUserError> {
    Ok(match text.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => Validation::Valid,
        _ => Validation::Invalid("Enter a finite number".into()),
    })
}

fn validate_duration(text: &str) -> Result<Validation, inquire::CustomUserError> {
    Ok(match text.trim().parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => Validation::Valid,
        _ => Validation::Invalid("Enter a number of seconds, 0 or more".into()),
    })
}

type TextValidator = fn(&str) -> Result<Validation, inquire::CustomUserError>;

fn ask(
    input: &dyn Input,
    label: &str,
    default: Option<f64>,
    validator: TextValidator,
) -> Result<f64> {
    let text = match default {
        Some(value) => {
            input.prompt_text_with_default(label, &value.to_string(), Some(&validator))?
        }
        None => input.prompt_text(label, None, None, Some(&validator))?,
    };
    Ok(text.trim().parse()?)
}

fn ask_fader(input: &dyn Input, label: &str, current: &str) -> Result<String> {
    let mut faders: Vec<String> = FADER_ALGORITHM_NAMES
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Offer the current fader first so that confirming keeps it.
    if let Some(index) = faders.iter().position(|f| f == current) {
        faders.swap(0, index);
    }
    input.select(label, &faders)
}

/// Fills the required values (min, max) no flag provided.
pub fn complete_required(input: &dyn Input, patch: RtpcPatch) -> Result<RtpcPatch> {
    let min_value = match patch.min_value {
        Some(v) => v,
        None => ask(input, MIN_LABEL, None, validate_finite)?,
    };
    let max_value = match patch.max_value {
        Some(v) => v,
        None => ask(input, MAX_LABEL, None, validate_finite)?,
    };
    Ok(RtpcPatch {
        min_value: Some(min_value),
        max_value: Some(max_value),
        ..patch
    })
}

/// Asks for every fade field the patch leaves unset, with `enabled` already settled.
fn prompt_fade(input: &dyn Input, patch: FadePatch) -> Result<FadePatch> {
    let enabled = match patch.enabled {
        Some(e) => e,
        None => input.confirm("Enable fade settings?", Some(true))?,
    };
    let attack_duration = match patch.attack_duration {
        Some(d) => d,
        None => ask(
            input,
            "Fade attack duration (seconds)",
            Some(0.0),
            validate_duration,
        )?,
    };
    let attack_fader = match patch.attack_fader {
        Some(f) => f,
        None => ask_fader(input, "Fade attack fader", DEFAULT_FADER)?,
    };
    let release_duration = match patch.release_duration {
        Some(d) => d,
        None => ask(
            input,
            "Fade release duration (seconds)",
            Some(0.0),
            validate_duration,
        )?,
    };
    let release_fader = match patch.release_fader {
        Some(f) => f,
        None => ask_fader(input, "Fade release fader", DEFAULT_FADER)?,
    };
    Ok(FadePatch {
        enabled: Some(enabled),
        attack_duration: Some(attack_duration),
        attack_fader: Some(attack_fader),
        release_duration: Some(release_duration),
        release_fader: Some(release_fader),
    })
}

/// Walks a new RTPC through every optional field no flag already set.
///
/// The author may accept defaults for all of them in a single answer.
pub fn complete_optional(input: &dyn Input, patch: RtpcPatch) -> Result<RtpcPatch> {
    let fade_given = !patch.fade.is_empty();
    if patch.default_value.is_some() && fade_given
        || input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))?
    {
        return Ok(patch);
    }

    let default_value = match patch.default_value {
        Some(v) => Some(v),
        None => {
            let (min, max) = (
                patch.min_value.unwrap_or(0.0),
                patch.max_value.unwrap_or(0.0),
            );
            let suggested = if min <= max {
                0.0_f64.clamp(min, max)
            } else {
                0.0
            };
            Some(ask(input, DEFAULT_LABEL, Some(suggested), validate_finite)?)
        }
    };
    let fade = if fade_given || input.confirm("Configure fade settings?", Some(false))? {
        prompt_fade(input, patch.fade)?
    } else {
        patch.fade
    };
    Ok(RtpcPatch {
        default_value,
        fade,
        ..patch
    })
}

/// Offers every field of an existing RTPC; unchanged answers add nothing to the patch.
pub fn prompt_update_patch(input: &dyn Input, existing: &Rtpc) -> Result<RtpcPatch> {
    let changed = |new: f64, old: f64| (new != old).then_some(new);
    let min = ask(input, MIN_LABEL, Some(existing.min_value), validate_finite)?;
    let max = ask(input, MAX_LABEL, Some(existing.max_value), validate_finite)?;
    let default = ask(
        input,
        DEFAULT_LABEL,
        Some(existing.default_value),
        validate_finite,
    )?;
    let fade = if input.confirm("Change fade settings?", Some(false))? {
        prompt_fade(input, FadePatch::default())?
    } else {
        FadePatch::default()
    };
    Ok(RtpcPatch {
        min_value: changed(min, existing.min_value),
        max_value: changed(max, existing.max_value),
        default_value: changed(default, existing.default_value),
        fade,
    })
}
