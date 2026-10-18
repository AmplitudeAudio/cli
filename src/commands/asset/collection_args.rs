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

//! Command-line flags of `collection create` / `collection update` and their
//! resolution into a [`CollectionPatch`] (names looked up, enums parsed, numbers checked).

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::assets::extensions::{COLLECTION_PLAY_MODE_NAMES, SOUND_SCHEDULER_MODE_NAMES};
use crate::assets::generated::SequenceSoundSchedulerEndBehavior;
use crate::assets::generated::{CollectionPlayMode, SoundSchedulerMode};
use crate::assets::{AssetType, ProjectValidator};
use crate::common::errors::{CliError, codes};
use crate::forms::references::rule_for;
use crate::forms::rtpc_value::rtpc_value_from_flags;
use crate::forms::validate_refs::validate_reference;
use crate::services::collection::CollectionPatch;

use super::collection_entry_args::{entry_sounds, resolve_entries};
use super::parse_spatialization;
use super::sound_args::{parse_fader, parse_scope, resolve_reference, validate_gain};

/// Names of the Sequence end behaviors, as accepted by `--end-behavior`.
const END_BEHAVIOR_NAMES: &[&str] = &["Restart", "Reverse"];

/// Collection flags shared by `create` and `update` beyond the original set.
#[derive(Args, Debug, Default, Clone)]
pub struct CollectionExtraArgs {
    /// Effect to apply, by name or ID ('none' clears it; see 'am asset effect list')
    #[arg(long)]
    pub effect: Option<String>,

    /// Attenuation to apply, by name or ID ('none' clears it; see 'am asset attenuation list')
    #[arg(long)]
    pub attenuation: Option<String>,

    /// Scope the collection is applied to: world or entity (default: world)
    #[arg(long)]
    pub scope: Option<String>,

    /// Fader algorithm: Constant, Ease, EaseIn, EaseInOut, EaseOut, Linear, SCurveSmooth, SCurveSharp
    #[arg(long)]
    pub fader: Option<String>,

    /// Pitch multiplier (1.0 = unchanged)
    #[arg(long)]
    pub pitch: Option<f32>,

    /// Drive the gain from an RTPC, by name (instead of --gain)
    #[arg(long)]
    pub gain_rtpc: Option<String>,

    /// Drive the pitch from an RTPC, by name (instead of --pitch)
    #[arg(long)]
    pub pitch_rtpc: Option<String>,

    /// Drive the priority from an RTPC, by name (instead of --priority)
    #[arg(long)]
    pub priority_rtpc: Option<String>,

    /// Random scheduler: do not replay one of the last --repeat-count sounds (true/false)
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub avoid_repeat: Option<bool>,

    /// Random scheduler: how many recent sounds --avoid-repeat remembers (default: 1)
    #[arg(long)]
    pub repeat_count: Option<u32>,

    /// Sequence scheduler: what happens at the end of the list: Restart or Reverse (default: Restart)
    #[arg(long)]
    pub end_behavior: Option<String>,

    /// A sound entry: SOUND[,weight=W][,gain=G|gain-rtpc=NAME][,pitch=P|pitch-rtpc=NAME]
    /// where SOUND is a sound name or ID. weight needs the Random scheduler (default: 1).
    /// Repeatable; on update the given entries replace all existing ones
    #[arg(long = "entry", value_name = "SOUND[,KEY=VALUE...]")]
    pub entry: Vec<String>,

    /// Read the entries from a JSON file instead of --entry: a list of
    /// {"sound": NAME|ID, "weight": N, "gain": N|VALUE, "pitch": N|VALUE} objects, or the
    /// SDK form {"sounds_type": [...], "sounds": [...]}. On update it replaces all entries
    #[arg(long, value_name = "FILE")]
    pub entries_from_json: Option<PathBuf>,
}

/// Flags that edit the entry list of an existing collection one entry at a time.
#[derive(Args, Debug, Default, Clone)]
pub struct CollectionEntryEditArgs {
    /// Append a sound entry (same syntax as --entry); repeatable
    #[arg(long, value_name = "SOUND[,KEY=VALUE...]")]
    pub add_entry: Vec<String>,

    /// Remove the entries of a sound, by name or ID; repeatable
    #[arg(long, value_name = "SOUND")]
    pub remove_entry: Vec<String>,

    /// Remove every entry
    #[arg(long)]
    pub clear_entries: bool,
}

/// Every flag of `collection create` / `collection update`, in one place.
#[derive(Debug, Default, Clone)]
pub struct CollectionFlags {
    pub play_mode: Option<String>,
    pub scheduler_mode: Option<String>,
    pub gain: Option<f32>,
    pub bus: Option<String>,
    pub priority: Option<u8>,
    pub spatialization: Option<String>,
    pub extra: CollectionExtraArgs,
    pub edit: CollectionEntryEditArgs,
}

fn invalid(what: &str, value: &str, choices: &[&str], flag: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid {what}: '{value}'"),
        format!("{what} must be one of: {}", choices.join(", ")),
    )
    .with_suggestion(format!("Use {flag} with one of the listed values"))
}

/// Finds the canonical spelling of `value` among `names`, ignoring case.
fn canonical<'a>(value: &str, names: &'a [&'a str]) -> Option<&'a str> {
    names
        .iter()
        .copied()
        .find(|n| n.eq_ignore_ascii_case(value))
}

fn parse_play_mode(value: &str) -> Result<CollectionPlayMode> {
    let name = canonical(value, COLLECTION_PLAY_MODE_NAMES).unwrap_or(value);
    CollectionPlayMode::from_str(name).map_err(|e| {
        CliError::new(codes::ERR_VALIDATION_FIELD, e, "Invalid play mode value").into()
    })
}

fn parse_scheduler_mode(value: &str) -> Result<SoundSchedulerMode> {
    let name = canonical(value, SOUND_SCHEDULER_MODE_NAMES).unwrap_or(value);
    SoundSchedulerMode::from_str(name).map_err(|e| {
        CliError::new(
            codes::ERR_VALIDATION_FIELD,
            e,
            "Invalid scheduler mode value",
        )
        .into()
    })
}

fn parse_end_behavior(value: &str) -> Result<SequenceSoundSchedulerEndBehavior> {
    match canonical(value, END_BEHAVIOR_NAMES) {
        Some("Restart") => Ok(SequenceSoundSchedulerEndBehavior::Restart),
        Some(_) => Ok(SequenceSoundSchedulerEndBehavior::Reverse),
        None => Err(invalid("end behavior", value, END_BEHAVIOR_NAMES, "--end-behavior").into()),
    }
}

fn resolve_optional_reference(
    validator: &ProjectValidator,
    field: &str,
    raw: Option<&str>,
) -> Result<Option<u64>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let rule = rule_for(AssetType::Collection, field)
        .ok_or_else(|| anyhow::anyhow!("Collection has no reference rule for '{field}'"))?;
    resolve_reference(validator, &rule, raw).map(Some)
}

/// Turns the raw flags into a patch: names become ids, enums are parsed and
/// numbers checked. Fails fast on the first unusable value.
pub fn resolve_patch(
    flags: &CollectionFlags,
    validator: &ProjectValidator,
) -> Result<CollectionPatch> {
    let extra = &flags.extra;
    // Resolve the bus first so a bad --bus is reported before anything else.
    let bus = resolve_optional_reference(validator, "bus", flags.bus.as_deref())?;
    if let Some(gain) = flags.gain {
        validate_gain(gain)?;
    }

    Ok(CollectionPatch {
        bus,
        effect: resolve_optional_reference(validator, "effect", extra.effect.as_deref())?,
        attenuation: resolve_optional_reference(
            validator,
            "attenuation",
            extra.attenuation.as_deref(),
        )?,
        scope: extra.scope.as_deref().map(parse_scope).transpose()?,
        fader: extra.fader.as_deref().map(parse_fader).transpose()?,
        gain: rtpc_value_from_flags(flags.gain, extra.gain_rtpc.as_deref(), validator)?,
        pitch: rtpc_value_from_flags(extra.pitch, extra.pitch_rtpc.as_deref(), validator)?,
        priority: rtpc_value_from_flags(
            flags.priority.map(f32::from),
            extra.priority_rtpc.as_deref(),
            validator,
        )?,
        spatialization: flags
            .spatialization
            .as_deref()
            .map(parse_spatialization)
            .transpose()?,
        play_mode: flags
            .play_mode
            .as_deref()
            .map(parse_play_mode)
            .transpose()?,
        scheduler_mode: flags
            .scheduler_mode
            .as_deref()
            .map(parse_scheduler_mode)
            .transpose()?,
        avoid_repeat: extra.avoid_repeat,
        repeat_count: extra.repeat_count,
        end_behavior: extra
            .end_behavior
            .as_deref()
            .map(parse_end_behavior)
            .transpose()?,
        entries: resolve_entries(extra, &flags.edit, validator)?,
    })
}

/// Checks the reference ids a patch sets against the project, so a dangling
/// id is reported before the user is asked anything else.
pub fn validate_patch_references(
    patch: &CollectionPatch,
    validator: &ProjectValidator,
) -> Result<()> {
    let provided = [
        ("bus", patch.bus),
        ("attenuation", patch.attenuation),
        ("effect", patch.effect),
    ];
    for (field, id) in provided {
        let (Some(id), Some(rule)) = (id, rule_for(AssetType::Collection, field)) else {
            continue;
        };
        validate_reference(&rule, id, validator)?;
    }

    let rule = rule_for(AssetType::Collection, "sounds.sound")
        .ok_or_else(|| anyhow::anyhow!("Collection has no reference rule for 'sounds.sound'"))?;
    for (index, sound) in entry_sounds(&patch.entries).into_iter().enumerate() {
        validate_reference(&rule, sound, validator)
            .map_err(|e| e.with_field(format!("sounds[{index}].sound")))?;
    }
    Ok(())
}
