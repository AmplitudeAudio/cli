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

//! Command-line flags of `sound create` / `sound update` and their resolution
//! into a [`SoundPatch`] (names looked up, enums parsed, numbers checked).

use anyhow::Result;
use clap::Args;

use crate::assets::extensions::FADER_ALGORITHM_NAMES;
use crate::assets::generated::Scope;
use crate::assets::{AssetType, ProjectValidator};
use crate::common::errors::{CliError, codes};
use crate::forms::pick::{create_command, target_label};
use crate::forms::references::{RefRule, RefTarget, rule_for};
use crate::forms::rtpc_value::rtpc_value_from_flags;
use crate::forms::validate_refs::{inactive_bus_warning, validate_reference};
use crate::presentation::Output;
use crate::services::sound::SoundPatch;

use super::parse_spatialization;

/// Accepted spellings of "no reference" for optional reference flags.
const NONE_KEYWORD: &str = "none";

/// Sound flags shared by `create` and `update` beyond the original set.
#[derive(Args, Debug, Default, Clone)]
pub struct SoundExtraArgs {
    /// Attenuation to apply, by name or ID ('none' clears it; see 'am asset attenuation list')
    #[arg(long)]
    pub attenuation: Option<String>,

    /// Effect to apply, by name or ID ('none' clears it; see 'am asset effect list')
    #[arg(long)]
    pub effect: Option<String>,

    /// Scope the sound is applied to: world or entity (default: world)
    #[arg(long)]
    pub scope: Option<String>,

    /// Fader algorithm: Constant, Ease, EaseIn, EaseInOut, EaseOut, Linear, SCurveSmooth, SCurveSharp
    #[arg(long)]
    pub fader: Option<String>,

    /// Pitch multiplier (1.0 = unchanged)
    #[arg(long)]
    pub pitch: Option<f32>,

    /// Near-field gain applied by the near-field effect node
    #[arg(long)]
    pub near_field_gain: Option<f32>,

    /// Drive the gain from an RTPC, by name (instead of --gain)
    #[arg(long)]
    pub gain_rtpc: Option<String>,

    /// Drive the pitch from an RTPC, by name (instead of --pitch)
    #[arg(long)]
    pub pitch_rtpc: Option<String>,

    /// Drive the priority from an RTPC, by name (instead of --priority)
    #[arg(long)]
    pub priority_rtpc: Option<String>,

    /// Drive the near-field gain from an RTPC, by name (instead of --near-field-gain)
    #[arg(long)]
    pub near_field_gain_rtpc: Option<String>,

    /// Drive the spread from an RTPC, by name (instead of --spread)
    #[arg(long)]
    pub spread_rtpc: Option<String>,
}

/// Every flag of `sound create` / `sound update`, in one place.
#[derive(Debug, Default, Clone)]
pub struct SoundFlags {
    pub file: Option<String>,
    pub gain: Option<f32>,
    pub bus: Option<String>,
    pub priority: Option<u8>,
    pub stream: Option<bool>,
    pub loop_enabled: Option<bool>,
    pub loop_count: Option<u32>,
    pub spatialization: Option<String>,
    pub spread: Option<f32>,
    pub extra: SoundExtraArgs,
}

/// Validates a static gain value is within 0.0-1.0.
pub fn validate_gain(gain: f32) -> Result<()> {
    if !(0.0..=1.0).contains(&gain) {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid gain value: {}", gain),
            "Gain must be between 0.0 and 1.0",
        )
        .with_suggestion("Set gain to a value between 0.0 (silent) and 1.0 (full volume)")
        .into());
    }
    Ok(())
}

/// Parses `--scope`.
pub fn parse_scope(value: &str) -> Result<Scope> {
    match value.to_lowercase().as_str() {
        "world" => Ok(Scope::World),
        "entity" => Ok(Scope::Entity),
        _ => Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid scope: '{value}'"),
            "Scope must be one of: world, entity",
        )
        .with_suggestion("Use --scope world or --scope entity")
        .into()),
    }
}

/// Parses `--fader` (case-insensitive) into the canonical algorithm name.
pub fn parse_fader(value: &str) -> Result<String> {
    FADER_ALGORITHM_NAMES
        .iter()
        .find(|name| name.eq_ignore_ascii_case(value))
        .map(|name| name.to_string())
        .ok_or_else(|| {
            CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("Unknown fader algorithm: '{value}'"),
                format!(
                    "Fader must be one of: {}",
                    FADER_ALGORITHM_NAMES.join(", ")
                ),
            )
            .with_suggestion("Use --fader with one of the listed names")
            .into()
        })
}

/// Resolves a reference flag given as a name or a numeric ID.
///
/// Optional references accept `none` (and `0`) to mean "unset". A name that
/// matches no object is a reference error that lists what exists.
pub(super) fn resolve_reference(
    validator: &ProjectValidator,
    rule: &RefRule,
    raw: &str,
) -> Result<u64> {
    let candidates = validator.list(rule.target);
    if let Some(found) = candidates.iter().find(|c| c.name == raw) {
        return Ok(found.id);
    }
    // A bus outside the active bus file is accepted (the caller warns).
    if rule.target == RefTarget::Bus
        && let Some(found) = validator.find_bus_anywhere(Some(raw), None)
    {
        return Ok(found.id);
    }
    if !rule.required && raw.eq_ignore_ascii_case(NONE_KEYWORD) {
        return Ok(0);
    }
    if let Ok(id) = raw.parse::<u64>() {
        return Ok(id);
    }

    let label = target_label(rule.target);
    let available = if candidates.is_empty() {
        format!("the project has no {label} yet; create one with `{}`", create_command(rule.target))
    } else {
        format!(
            "available: {}",
            candidates
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Err(CliError::new(
        codes::ERR_VALIDATION_REFERENCE,
        format!("No {label} named '{raw}'"),
        format!("'{}' must be the name or ID of an existing {label}", rule.field),
    )
    .with_suggestion(available)
    .into())
}

fn resolve_optional_reference(
    validator: &ProjectValidator,
    field: &str,
    raw: Option<&str>,
) -> Result<Option<u64>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let rule = rule_for(AssetType::Sound, field)
        .ok_or_else(|| anyhow::anyhow!("Sound has no reference rule for '{field}'"))?;
    resolve_reference(validator, &rule, raw).map(Some)
}

/// Turns the raw flags into a patch: names become ids, enums are parsed and
/// numbers checked. Fails fast on the first unusable value.
pub fn resolve_patch(flags: &SoundFlags, validator: &ProjectValidator) -> Result<SoundPatch> {
    let extra = &flags.extra;
    // Resolve the bus first so a bad --bus is reported before anything else.
    let bus = resolve_optional_reference(validator, "bus", flags.bus.as_deref())?;
    if let Some(gain) = flags.gain {
        validate_gain(gain)?;
    }

    Ok(SoundPatch {
        audio_file: flags.file.clone(),
        bus,
        attenuation: resolve_optional_reference(
            validator,
            "attenuation",
            extra.attenuation.as_deref(),
        )?,
        effect: resolve_optional_reference(validator, "effect", extra.effect.as_deref())?,
        scope: extra.scope.as_deref().map(parse_scope).transpose()?,
        fader: extra.fader.as_deref().map(parse_fader).transpose()?,
        gain: rtpc_value_from_flags(flags.gain, extra.gain_rtpc.as_deref(), validator)?,
        pitch: rtpc_value_from_flags(extra.pitch, extra.pitch_rtpc.as_deref(), validator)?,
        near_field_gain: rtpc_value_from_flags(
            extra.near_field_gain,
            extra.near_field_gain_rtpc.as_deref(),
            validator,
        )?,
        priority: rtpc_value_from_flags(
            flags.priority.map(f32::from),
            extra.priority_rtpc.as_deref(),
            validator,
        )?,
        spread: rtpc_value_from_flags(flags.spread, extra.spread_rtpc.as_deref(), validator)?,
        stream: flags.stream,
        loop_enabled: flags.loop_enabled,
        loop_count: flags.loop_count,
        spatialization: flags
            .spatialization
            .as_deref()
            .map(parse_spatialization)
            .transpose()?,
    })
}

/// Checks the reference ids a patch sets against the project, so a dangling
/// id is reported before the user is asked anything else.
pub fn validate_patch_references(patch: &SoundPatch, validator: &ProjectValidator) -> Result<()> {
    let provided = [
        ("bus", patch.bus),
        ("attenuation", patch.attenuation),
        ("effect", patch.effect),
    ];
    for (field, id) in provided {
        let (Some(id), Some(rule)) = (id, rule_for(AssetType::Sound, field)) else {
            continue;
        };
        validate_reference(&rule, id, validator)?;
    }
    Ok(())
}

/// Warns (does not fail) when the sound's bus exists only outside the bus file
/// the engine configs load. Prints each warning and returns them for the JSON result.
pub fn bus_warnings(
    validator: &ProjectValidator,
    bus: Option<u64>,
    output: &dyn Output,
) -> Vec<String> {
    let warnings: Vec<String> = bus
        .and_then(|id| inactive_bus_warning(validator, id))
        .into_iter()
        .collect();
    for warning in &warnings {
        output.warning(warning);
    }
    warnings
}

/// Adds a `warnings` array to a JSON result when there are warnings.
pub fn with_warnings(mut data: serde_json::Value, warnings: &[String]) -> serde_json::Value {
    if let (false, Some(map)) = (warnings.is_empty(), data.as_object_mut()) {
        map.insert("warnings".to_string(), serde_json::json!(warnings));
    }
    data
}
