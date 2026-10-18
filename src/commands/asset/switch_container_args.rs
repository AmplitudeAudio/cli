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

//! Command-line flags of `switch-container create` / `switch-container update` and
//! their resolution into a [`SwitchContainerPatch`] (names looked up, enums parsed,
//! numbers checked).
//!
//! State names depend on the container's switch, so resolution happens in two
//! steps: [`resolve_flags`] looks up everything that does not depend on the switch,
//! and [`bind_states`] resolves the state names once the switch is known.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::assets::generated::SwitchContainerUpdateBehavior;
use crate::assets::{AssetType, ProjectValidator};
use crate::common::errors::{CliError, codes};
use crate::forms::references::rule_for;
use crate::forms::rtpc_value::rtpc_value_from_flags;
use crate::forms::validate_refs::validate_reference;
use crate::services::switch_container::SwitchContainerPatch;

use super::parse_spatialization;
use super::sound_args::{parse_fader, parse_scope, resolve_reference, validate_gain};
use super::switch_container_entry_args::{
    RawEntriesPatch, bind_entries, resolve_entries, resolve_state,
};

/// Names of the update behaviors, as accepted by `--update-behavior`.
pub const UPDATE_BEHAVIOR_NAMES: &[&str] = &["UpdateOnPlay", "UpdateOnChange"];

/// Switch container flags shared by `create` and `update` beyond the basic set.
#[derive(Args, Debug, Default, Clone)]
pub struct SwitchContainerExtraArgs {
    /// Effect to apply, by name or ID ('none' clears it; see 'am asset effect list')
    #[arg(long)]
    pub effect: Option<String>,

    /// Attenuation to apply, by name or ID ('none' clears it; see 'am asset attenuation list')
    #[arg(long)]
    pub attenuation: Option<String>,

    /// Scope the container is applied to: world or entity (default: world)
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

    /// When the sounds follow a switch state change: UpdateOnPlay or UpdateOnChange (default: UpdateOnPlay)
    #[arg(long)]
    pub update_behavior: Option<String>,

    /// State played while the switch has no current state, by name (or ID) in the
    /// switch group (default: the first state of the switch)
    #[arg(long)]
    pub default_switch_state: Option<String>,

    /// An entry: OBJECT,states=STATE|STATE[,continue=true][,fade-in=SECONDS[:FADER]]
    /// [,fade-out=SECONDS[:FADER]][,gain=G|gain-rtpc=NAME][,pitch=P|pitch-rtpc=NAME]
    /// where OBJECT is a sound or collection name, sound:NAME, collection:NAME or an ID.
    /// Repeatable; on update the given entries replace all existing ones
    #[arg(long = "entry", value_name = "OBJECT,states=STATE|STATE[,KEY=VALUE...]")]
    pub entry: Vec<String>,

    /// Read the entries from a JSON file instead of --entry: a list of
    /// {"object": NAME|ID, "switch_states": [NAME|ID...], "continue_between_states": bool,
    /// "fade_in": {"duration": N, "fader": NAME}, "fade_out": {...}, "gain": N|VALUE,
    /// "pitch": N|VALUE} objects, or an SDK switch container with such 'entries'.
    /// On update it replaces all entries
    #[arg(long, value_name = "FILE")]
    pub entries_from_json: Option<PathBuf>,

    /// Play OBJECT in STATE: adds the state to the entry of the object (creating it
    /// with default settings when there is none). Repeatable
    #[arg(long, value_name = "STATE=OBJECT")]
    pub map: Vec<String>,
}

/// Flags that edit the entry list of an existing switch container one entry at a time.
#[derive(Args, Debug, Default, Clone)]
pub struct SwitchContainerEntryEditArgs {
    /// Append an entry (same syntax as --entry); repeatable
    #[arg(long, value_name = "OBJECT,states=STATE|STATE[,KEY=VALUE...]")]
    pub add_entry: Vec<String>,

    /// Remove the entries of an object (sound or collection, by name or ID); repeatable
    #[arg(long, value_name = "OBJECT")]
    pub remove_entry: Vec<String>,

    /// Remove every entry
    #[arg(long)]
    pub clear_entries: bool,
}

/// Every flag of `switch-container create` / `switch-container update`, in one place.
#[derive(Debug, Default, Clone)]
pub struct SwitchContainerFlags {
    pub switch_group: Option<String>,
    pub gain: Option<f32>,
    pub bus: Option<String>,
    pub priority: Option<u8>,
    pub spatialization: Option<String>,
    pub extra: SwitchContainerExtraArgs,
    pub edit: SwitchContainerEntryEditArgs,
}

/// The flags after the first resolution step: everything but the switch-dependent parts.
#[derive(Debug, Clone, Default)]
pub struct ResolvedFlags {
    /// The patch without default state and entries.
    pub patch: SwitchContainerPatch,
    /// `--default-switch-state`, still a name or an ID.
    pub default_state: Option<String>,
    /// The entry flags, with state names still unresolved.
    pub entries: RawEntriesPatch,
}

fn parse_update_behavior(value: &str) -> Result<SwitchContainerUpdateBehavior> {
    let name = UPDATE_BEHAVIOR_NAMES
        .iter()
        .find(|n| n.eq_ignore_ascii_case(value))
        .copied();
    match name {
        Some("UpdateOnPlay") => Ok(SwitchContainerUpdateBehavior::UpdateOnPlay),
        Some(_) => Ok(SwitchContainerUpdateBehavior::UpdateOnChange),
        None => Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid update behavior: '{value}'"),
            format!("Update behavior must be one of: {}", UPDATE_BEHAVIOR_NAMES.join(", ")),
        )
        .with_suggestion("Use --update-behavior with one of the listed values")
        .into()),
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
    let rule = rule_for(AssetType::SwitchContainer, field)
        .ok_or_else(|| anyhow::anyhow!("Switch container has no reference rule for '{field}'"))?;
    resolve_reference(validator, &rule, raw).map(Some)
}

/// Turns the raw flags into a patch: names become ids, enums are parsed and
/// numbers checked. Fails fast on the first unusable value.
pub fn resolve_flags(
    flags: &SwitchContainerFlags,
    validator: &ProjectValidator,
) -> Result<ResolvedFlags> {
    let extra = &flags.extra;
    // Resolve the bus first so a bad --bus is reported before anything else.
    let bus = resolve_optional_reference(validator, "bus", flags.bus.as_deref())?;
    if let Some(gain) = flags.gain {
        validate_gain(gain)?;
    }

    let patch = SwitchContainerPatch {
        bus,
        switch_group: resolve_optional_reference(
            validator,
            "switch_group",
            flags.switch_group.as_deref(),
        )?,
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
        update_behavior: extra
            .update_behavior
            .as_deref()
            .map(parse_update_behavior)
            .transpose()?,
        ..Default::default()
    };
    Ok(ResolvedFlags {
        patch,
        default_state: extra.default_switch_state.clone(),
        entries: resolve_entries(extra, &flags.edit, validator)?,
    })
}

/// Checks the reference ids the flags set against the project, so a dangling
/// id is reported before the user is asked anything else.
pub fn validate_flag_references(
    resolved: &ResolvedFlags,
    validator: &ProjectValidator,
) -> Result<()> {
    let patch = &resolved.patch;
    let provided = [
        ("bus", patch.bus),
        ("switch_group", patch.switch_group),
        ("attenuation", patch.attenuation),
        ("effect", patch.effect),
    ];
    for (field, id) in provided {
        let (Some(id), Some(rule)) = (id, rule_for(AssetType::SwitchContainer, field)) else {
            continue;
        };
        validate_reference(&rule, id, validator)?;
    }

    let rule = rule_for(AssetType::SwitchContainer, "entries.object")
        .ok_or_else(|| anyhow::anyhow!("Switch container has no reference rule for 'entries.object'"))?;
    for (index, object) in resolved.entries.incoming_objects().into_iter().enumerate() {
        validate_reference(&rule, object, validator)
            .map_err(|e| e.with_field(format!("entries[{index}].object")))?;
    }
    Ok(())
}

/// Completes the patch with the parts that depend on the switch `group`: the default
/// state and the entries' state names.
pub fn bind_states(
    resolved: ResolvedFlags,
    group: u64,
    validator: &ProjectValidator,
) -> Result<SwitchContainerPatch> {
    let default_switch_state = resolved
        .default_state
        .as_deref()
        .map(|raw| resolve_state(validator, group, raw))
        .transpose()?;
    let entries = bind_entries(validator, group, &resolved.entries)?;
    Ok(SwitchContainerPatch {
        default_switch_state,
        entries,
        ..resolved.patch
    })
}
