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

//! Interactive prompts for `sound create` and `sound update`.
//!
//! Every function here is only called when the session can prompt; flag values
//! already present in the patch are never asked for again.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::extensions::FADER_ALGORITHM_NAMES;
use crate::assets::generated::{Scope, Spatialization};
use crate::assets::{AssetType, ProjectValidator, RtpcCompatibleValue, Sound};
use crate::common::errors::{CliError, codes};
use crate::forms::pick::{NONE_LABEL, pick_reference, target_label};
use crate::forms::references::rule_for;
use crate::forms::rtpc_value::prompt_rtpc_value;
use crate::input::{Input, select_index};
use crate::services::sound::{DEFAULT_GAIN, DEFAULT_PRIORITY, SoundPatch};

const ACCEPT_DEFAULTS_PROMPT: &str = "Accept defaults for remaining optional fields?";
const KEEP_LABEL: &str = "(keep current)";

const DEFAULT_PITCH: f32 = 1.0;
const DEFAULT_NEAR_FIELD_GAIN: f32 = 1.0;
/// Fader written when none is chosen; matches the builder and the service default.
const DEFAULT_FADER: &str = "Linear";
/// Neutral suggestion only; an unset spread keeps the asset's native layout width.
const SUGGESTED_SPREAD: f32 = 60.0;
const SPREAD_LABEL: &str = "Spread in degrees (0-360)";

const SCOPE_LABELS: [&str; 2] = ["World", "Entity"];
const SCOPES: [Scope; 2] = [Scope::World, Scope::Entity];
const SPATIALIZATION_LABELS: [&str; 4] = ["None", "Position", "Position + Orientation", "HRTF"];
const SPATIALIZATIONS: [Spatialization; 4] = [
    Spatialization::None,
    Spatialization::Position,
    Spatialization::PositionOrientation,
    Spatialization::HRTF,
];

fn to_labels(labels: &[&str]) -> Vec<String> {
    labels.iter().map(|l| l.to_string()).collect()
}

fn range_error(label: &str, value: f32, low: f32, high: f32) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid {label} value: {value}"),
        format!("{label} must be between {low} and {high}"),
    )
    .with_suggestion(format!("Choose a {label} between {low} and {high}"))
}

/// Rejects a static value outside `low..=high`; RTPC bindings pass unchecked.
fn check_range(
    label: &str,
    value: RtpcCompatibleValue,
    low: f32,
    high: f32,
) -> Result<RtpcCompatibleValue> {
    match value.as_static() {
        Some(v) if !(low..=high).contains(&v) => Err(range_error(label, v, low, high).into()),
        _ => Ok(value),
    }
}

fn describe_value(value: &Option<RtpcCompatibleValue>) -> String {
    match value {
        None => "not set".to_string(),
        Some(v) => v
            .as_static()
            .map_or_else(|| "bound to RTPC".to_string(), |n| n.to_string()),
    }
}

/// Asks for the audio file path (relative to `data/`).
pub fn prompt_audio_file(input: &dyn Input) -> Result<String> {
    input.prompt_text(
        "Path to audio file (relative to data/)",
        Some("sfx/sound.wav"),
        None,
        Some(&|value: &str| {
            if value.trim().is_empty() {
                return Ok(Validation::Invalid("Audio file path is required".into()));
            }
            Ok(Validation::Valid)
        }),
    )
}

/// Picks the bus of a new sound; the bus is required, so there is no "(none)".
pub fn prompt_bus(input: &dyn Input, validator: &ProjectValidator) -> Result<u64> {
    let rule = rule_for(AssetType::Sound, "bus")
        .ok_or_else(|| anyhow::anyhow!("Sound has no bus reference rule"))?;
    pick_reference(input, validator, &rule)?
        .ok_or_else(|| anyhow::anyhow!("A required reference cannot be left unset"))
}

fn pick_optional(input: &dyn Input, validator: &ProjectValidator, field: &str) -> Result<u64> {
    let rule = rule_for(AssetType::Sound, field)
        .ok_or_else(|| anyhow::anyhow!("Sound has no reference rule for '{field}'"))?;
    Ok(pick_reference(input, validator, &rule)?.unwrap_or(0))
}

/// An RTPC-capable value that may stay unset: asks first, then for the value.
fn prompt_optional_rtpc(
    input: &dyn Input,
    validator: &ProjectValidator,
    label: &str,
    default: f32,
    range: Option<(f32, f32)>,
) -> Result<Option<RtpcCompatibleValue>> {
    if !input.confirm(&format!("Set {}?", label.to_lowercase()), Some(false))? {
        return Ok(None);
    }
    prompt_checked_rtpc(input, validator, label, default, range).map(Some)
}

fn prompt_checked_rtpc(
    input: &dyn Input,
    validator: &ProjectValidator,
    label: &str,
    default: f32,
    range: Option<(f32, f32)>,
) -> Result<RtpcCompatibleValue> {
    let value = prompt_rtpc_value(input, validator, label, default)?;
    match range {
        Some((low, high)) => check_range(label, value, low, high),
        None => Ok(value),
    }
}

fn prompt_loop(input: &dyn Input) -> Result<(bool, u32)> {
    if !input.confirm("Enable looping?", Some(false))? {
        return Ok((false, 0));
    }
    let count = prompt_loop_count(input, 0)?;
    Ok((true, count))
}

fn prompt_loop_count(input: &dyn Input, current: u32) -> Result<u32> {
    let text = input.prompt_text(
        "Loop count (0=infinite)",
        Some(&current.to_string()),
        None,
        Some(&|value: &str| match value.trim().parse::<u32>() {
            Ok(_) => Ok(Validation::Valid),
            Err(_) => Ok(Validation::Invalid("Must be a non-negative number".into())),
        }),
    )?;
    Ok(text.trim().parse().unwrap_or(current))
}

fn is_walkthrough_needed(patch: &SoundPatch) -> bool {
    patch.attenuation.is_none()
        || patch.effect.is_none()
        || patch.scope.is_none()
        || patch.fader.is_none()
        || patch.gain.is_none()
        || patch.pitch.is_none()
        || patch.near_field_gain.is_none()
        || patch.priority.is_none()
        || patch.spread.is_none()
        || patch.stream.is_none()
        || patch.loop_enabled.is_none()
        || patch.spatialization.is_none()
}

/// Walks a new sound through every optional field that no flag already set.
///
/// After the required fields (audio file and bus, handled by the caller) the
/// author may accept defaults for everything that is left in a single answer.
pub fn complete_create_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    patch: SoundPatch,
) -> Result<SoundPatch> {
    if !is_walkthrough_needed(&patch) || input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))? {
        return Ok(patch);
    }

    let attenuation = match patch.attenuation {
        Some(id) => id,
        None => pick_optional(input, validator, "attenuation")?,
    };
    let effect = match patch.effect {
        Some(id) => id,
        None => pick_optional(input, validator, "effect")?,
    };
    let scope = match patch.scope {
        Some(scope) => scope,
        None => SCOPES[select_index(input, "Scope", &to_labels(&SCOPE_LABELS))?],
    };
    let fader = match &patch.fader {
        Some(fader) => fader.clone(),
        None => {
            // Linear first so accepting the first option matches the non-interactive default.
            let mut names = to_labels(FADER_ALGORITHM_NAMES);
            names.sort_by_key(|name| name != DEFAULT_FADER);
            names[select_index(input, "Fader", &names)?].clone()
        }
    };
    let gain = match &patch.gain {
        Some(v) => v.clone(),
        None => prompt_checked_rtpc(input, validator, "Gain", DEFAULT_GAIN, Some((0.0, 1.0)))?,
    };
    let pitch = match &patch.pitch {
        Some(v) => Some(v.clone()),
        None => prompt_optional_rtpc(input, validator, "Pitch", DEFAULT_PITCH, None)?,
    };
    let near_field_gain = match &patch.near_field_gain {
        Some(v) => Some(v.clone()),
        None => prompt_optional_rtpc(
            input,
            validator,
            "Near-field gain",
            DEFAULT_NEAR_FIELD_GAIN,
            None,
        )?,
    };
    let priority = match &patch.priority {
        Some(v) => v.clone(),
        None => prompt_checked_rtpc(
            input,
            validator,
            "Priority",
            DEFAULT_PRIORITY,
            Some((0.0, 255.0)),
        )?,
    };
    let spread = match &patch.spread {
        Some(v) => Some(v.clone()),
        None => prompt_optional_rtpc(
            input,
            validator,
            SPREAD_LABEL,
            SUGGESTED_SPREAD,
            Some((0.0, 360.0)),
        )?,
    };
    let stream = match patch.stream {
        Some(stream) => stream,
        None => input.confirm("Stream from disk?", Some(false))?,
    };
    let (loop_enabled, loop_count) = match patch.loop_enabled {
        Some(enabled) => (enabled, patch.loop_count),
        None => {
            let (enabled, count) = prompt_loop(input)?;
            (enabled, Some(count))
        }
    };
    let spatialization = match patch.spatialization {
        Some(mode) => mode,
        None => SPATIALIZATIONS[select_index(
            input,
            "Spatialization mode",
            &to_labels(&SPATIALIZATION_LABELS),
        )?],
    };

    Ok(SoundPatch {
        attenuation: Some(attenuation),
        effect: Some(effect),
        scope: Some(scope),
        fader: Some(fader),
        gain: Some(gain),
        pitch,
        near_field_gain,
        priority: Some(priority),
        spread,
        stream: Some(stream),
        loop_enabled: Some(loop_enabled),
        loop_count,
        spatialization: Some(spatialization),
        ..patch
    })
}

// -----------------------------------------------------------------------------
// Update
// -----------------------------------------------------------------------------

/// Text prompt pre-filled with the current value; `None` when unchanged.
fn ask_text(input: &dyn Input, label: &str, current: &str) -> Result<Option<String>> {
    let prompt = format!("{label} (current: {current}, Enter to keep)");
    let value = input.prompt_text_with_default(
        &prompt,
        current,
        Some(&|text: &str| {
            Ok(if text.trim().is_empty() {
                Validation::Invalid("The path cannot be empty".into())
            } else {
                Validation::Valid
            })
        }),
    )?;
    if value.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("{label} cannot be empty"),
            "Sounds must reference an audio file in the data/ directory",
        )
        .into());
    }
    Ok((value != current).then_some(value))
}

/// Yes/no prompt defaulting to the current value; `None` when unchanged.
fn ask_bool(input: &dyn Input, label: &str, current: bool) -> Result<Option<bool>> {
    let prompt = format!("{label} (current: {})", if current { "yes" } else { "no" });
    let value = input.confirm(&prompt, Some(current))?;
    Ok((value != current).then_some(value))
}

/// Choice among fixed options; `None` when the current one is chosen again.
fn ask_choice(
    input: &dyn Input,
    label: &str,
    options: &[String],
    current: usize,
) -> Result<Option<usize>> {
    let prompt = format!("{label} (current: {})", options[current]);
    let chosen = select_index(input, &prompt, options)?;
    Ok((chosen != current).then_some(chosen))
}

/// Reference chooser with a "(keep current)" entry; `None` when unchanged.
fn ask_reference(
    input: &dyn Input,
    validator: &ProjectValidator,
    field: &str,
    current: u64,
) -> Result<Option<u64>> {
    let rule = rule_for(AssetType::Sound, field)
        .ok_or_else(|| anyhow::anyhow!("Sound has no reference rule for '{field}'"))?;
    let candidates = validator.list(rule.target);
    let current_name = validator
        .resolve(rule.target, current)
        .map_or_else(|| NONE_LABEL.to_string(), |r| r.name);

    let mut labels = vec![KEEP_LABEL.to_string()];
    labels.extend(candidates.iter().map(|c| c.name.clone()));
    if !rule.required {
        labels.push(NONE_LABEL.to_string());
    }
    let prompt = format!(
        "Select {} for '{field}' (current: {current_name})",
        target_label(rule.target)
    );
    let chosen = match select_index(input, &prompt, &labels)? {
        0 => return Ok(None),
        i if i <= candidates.len() => candidates[i - 1].id,
        _ => 0,
    };
    Ok((chosen != current).then_some(chosen))
}

/// RTPC-capable value: asks whether to change it, then for the new value.
fn ask_rtpc(
    input: &dyn Input,
    validator: &ProjectValidator,
    label: &str,
    current: &Option<RtpcCompatibleValue>,
    default: f32,
    range: Option<(f32, f32)>,
) -> Result<Option<RtpcCompatibleValue>> {
    let question = format!("Change {}? (current: {})", label.to_lowercase(), describe_value(current));
    if !input.confirm(&question, Some(false))? {
        return Ok(None);
    }
    let start = current.as_ref().and_then(|v| v.as_static()).unwrap_or(default);
    prompt_checked_rtpc(input, validator, label, start, range).map(Some)
}

/// Offers every sound field with the current value; unchanged fields stay out of the patch.
pub fn prompt_update_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    sound: &Sound,
) -> Result<SoundPatch> {
    let audio_file = ask_text(input, "Audio file path", sound.path.as_deref().unwrap_or(""))?;
    let bus = ask_reference(input, validator, "bus", sound.bus)?;
    let attenuation = ask_reference(input, validator, "attenuation", sound.attenuation)?;
    let effect = ask_reference(input, validator, "effect", sound.effect)?;

    let current_scope = SCOPES.iter().position(|s| *s == sound.scope).unwrap_or(0);
    let scope = ask_choice(input, "Scope", &to_labels(&SCOPE_LABELS), current_scope)?
        .map(|i| SCOPES[i]);

    let faders = to_labels(FADER_ALGORITHM_NAMES);
    let current_fader = faders
        .iter()
        .position(|f| Some(f.as_str()) == sound.fader.as_deref())
        .unwrap_or(0);
    let fader = ask_choice(input, "Fader", &faders, current_fader)?.map(|i| faders[i].clone());

    let gain = ask_rtpc(input, validator, "Gain", &sound.gain, DEFAULT_GAIN, Some((0.0, 1.0)))?;
    let pitch = ask_rtpc(input, validator, "Pitch", &sound.pitch, DEFAULT_PITCH, None)?;
    let near_field_gain = ask_rtpc(
        input,
        validator,
        "Near-field gain",
        &sound.near_field_gain,
        DEFAULT_NEAR_FIELD_GAIN,
        None,
    )?;
    let priority = ask_rtpc(
        input,
        validator,
        "Priority",
        &sound.priority,
        DEFAULT_PRIORITY,
        Some((0.0, 255.0)),
    )?;
    let spread = ask_rtpc(
        input,
        validator,
        SPREAD_LABEL,
        &sound.spread,
        SUGGESTED_SPREAD,
        Some((0.0, 360.0)),
    )?;

    let stream = ask_bool(input, "Stream from disk", sound.stream)?;
    let (current_loop, current_count) = sound
        .loop_
        .as_ref()
        .map_or((false, 0), |l| (l.enabled, l.loop_count));
    let loop_enabled = ask_bool(input, "Enable looping", current_loop)?;
    let loop_count = if loop_enabled.unwrap_or(current_loop) {
        let count = prompt_loop_count(input, current_count)?;
        (count != current_count).then_some(count)
    } else {
        None
    };

    let current_spat = SPATIALIZATIONS
        .iter()
        .position(|m| *m == sound.spatialization)
        .unwrap_or(0);
    let spatialization = ask_choice(
        input,
        "Spatialization mode",
        &to_labels(&SPATIALIZATION_LABELS),
        current_spat,
    )?
    .map(|i| SPATIALIZATIONS[i]);

    Ok(SoundPatch {
        audio_file,
        bus,
        attenuation,
        effect,
        scope,
        fader,
        gain,
        pitch,
        near_field_gain,
        priority,
        spread,
        stream,
        loop_enabled,
        loop_count,
        spatialization,
    })
}
