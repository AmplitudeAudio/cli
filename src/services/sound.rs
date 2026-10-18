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

//! Sound creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI all produce a [`SoundValues`] (create) or
//! a [`SoundPatch`] (update); this module turns them into a validated [`Sound`]
//! without touching the terminal or the file system. Functions return new
//! values and never mutate their inputs.

use crate::assets::generated::{Scope, Spatialization};
use crate::assets::{
    Asset, AssetType, FaderAlgorithm, ProjectContext, ProjectValidator, RtpcCompatibleValue, Sound,
    SoundLoopConfig, ValidationError, ValidationLayer,
};
use crate::common::errors::codes::ERR_ASSET_ALREADY_EXISTS;
use crate::common::utils::generate_unique_id;
use crate::forms::references::rule_for;
use crate::forms::validate_refs::validate_reference;

/// Number of times id generation is retried after a collision.
const ID_COLLISION_RETRIES: usize = 3;

/// Every value a new sound needs. Reference fields use `0` for "unset".
#[derive(Debug, Clone, PartialEq)]
pub struct SoundValues {
    pub name: String,
    pub audio_file: String,
    pub bus: u64,
    pub attenuation: u64,
    pub effect: u64,
    pub scope: Scope,
    pub fader: String,
    pub gain: RtpcCompatibleValue,
    pub pitch: Option<RtpcCompatibleValue>,
    pub near_field_gain: Option<RtpcCompatibleValue>,
    pub priority: RtpcCompatibleValue,
    pub spread: Option<RtpcCompatibleValue>,
    pub stream: bool,
    pub loop_config: SoundLoopConfig,
    pub spatialization: Spatialization,
}

/// Default static gain of a new sound.
pub const DEFAULT_GAIN: f32 = 1.0;
/// Default static priority of a new sound.
pub const DEFAULT_PRIORITY: f32 = 128.0;

impl SoundValues {
    /// Fills a patch's gaps with defaults. The audio file and bus have none:
    /// a missing audio file is an error and a missing bus becomes the invalid
    /// id 0, which [`build_sound`] rejects.
    pub fn from_patch(name: &str, patch: &SoundPatch) -> Result<Self, ValidationError> {
        let audio_file = patch.audio_file.clone().ok_or_else(|| {
            ValidationError::type_rule_violation(
                "Sound asset has no audio file path",
                "Sound assets must reference an audio file in the data/ directory",
            )
            .with_field("path")
        })?;
        let loop_config = match (patch.loop_enabled.unwrap_or(false), patch.loop_count) {
            (false, _) => SoundLoopConfig::disabled(),
            (true, None | Some(0)) => SoundLoopConfig::infinite(),
            (true, Some(count)) => SoundLoopConfig::count(count),
        };

        Ok(Self {
            name: name.to_string(),
            audio_file,
            bus: patch.bus.unwrap_or(0),
            attenuation: patch.attenuation.unwrap_or(0),
            effect: patch.effect.unwrap_or(0),
            scope: patch.scope.unwrap_or_default(),
            fader: patch
                .fader
                .clone()
                .unwrap_or_else(|| FaderAlgorithm::Linear.to_string()),
            gain: patch
                .gain
                .clone()
                .unwrap_or_else(|| RtpcCompatibleValue::static_value(DEFAULT_GAIN)),
            pitch: patch.pitch.clone(),
            near_field_gain: patch.near_field_gain.clone(),
            priority: patch
                .priority
                .clone()
                .unwrap_or_else(|| RtpcCompatibleValue::static_value(DEFAULT_PRIORITY)),
            spread: patch.spread.clone(),
            stream: patch.stream.unwrap_or(false),
            loop_config,
            spatialization: patch.spatialization.unwrap_or(Spatialization::None),
        })
    }
}

fn id_collision_error(name: &str, id: u64) -> ValidationError {
    ValidationError::new(
        ValidationLayer::IdUniqueness,
        ERR_ASSET_ALREADY_EXISTS,
        format!("Generated ID {id} for sound '{name}' collides with an existing asset"),
        "All generated ID attempts collided with existing assets in the project",
    )
    .with_suggestion("Try a different name or wait a moment and retry")
    .with_field("id")
}

fn unique_id(name: &str, ctx: &ProjectContext) -> Result<u64, ValidationError> {
    let mut id = generate_unique_id(name);
    for attempt in 0..ID_COLLISION_RETRIES {
        if !ctx.has_id(id) {
            break;
        }
        id = generate_unique_id(&format!("{name}{attempt}"));
    }
    if ctx.has_id(id) {
        return Err(id_collision_error(name, id));
    }
    Ok(id)
}

/// Rejects the "unset" bus even when the context has no validator to resolve ids with.
///
/// Non-zero ids are resolved by `Sound::validate_rules` when a validator is present.
fn require_bus(bus: u64) -> Result<(), ValidationError> {
    if bus != 0 {
        return Ok(());
    }
    let rule = rule_for(AssetType::Sound, "bus")
        .expect("the reference table must describe the sound bus field");
    validate_reference(&rule, 0, &ProjectValidator::empty())
}

/// Builds a validated sound from `values`, generating a unique id.
pub fn build_sound(values: SoundValues, ctx: &ProjectContext) -> Result<Sound, ValidationError> {
    require_bus(values.bus)?;
    let id = unique_id(&values.name, ctx)?;

    let mut sound = Sound::builder(id, &values.name)
        .path(&values.audio_file)
        .bus(values.bus)
        .attenuation(values.attenuation)
        .effect(values.effect)
        .scope(values.scope)
        .gain_rtpc(values.gain)
        .priority_rtpc(values.priority)
        .stream(values.stream)
        .loop_config(values.loop_config)
        .spatialization(values.spatialization)
        .build();
    sound.fader = Some(values.fader);
    sound.pitch = values.pitch;
    sound.near_field_gain = values.near_field_gain;
    sound.spread = values.spread;

    sound.validate_rules(ctx)?;
    Ok(sound)
}

/// The fields of an existing sound a caller wants to change; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SoundPatch {
    pub audio_file: Option<String>,
    pub bus: Option<u64>,
    pub attenuation: Option<u64>,
    pub effect: Option<u64>,
    pub scope: Option<Scope>,
    pub fader: Option<String>,
    pub gain: Option<RtpcCompatibleValue>,
    pub pitch: Option<RtpcCompatibleValue>,
    pub near_field_gain: Option<RtpcCompatibleValue>,
    pub priority: Option<RtpcCompatibleValue>,
    pub spread: Option<RtpcCompatibleValue>,
    pub stream: Option<bool>,
    pub loop_enabled: Option<bool>,
    pub loop_count: Option<u32>,
    pub spatialization: Option<Spatialization>,
}

impl SoundPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("audio_file", self.audio_file.is_some()),
            ("gain", self.gain.is_some()),
            ("bus", self.bus.is_some()),
            ("priority", self.priority.is_some()),
            ("stream", self.stream.is_some()),
            ("loop_enabled", self.loop_enabled.is_some()),
            ("loop_count", self.loop_count.is_some()),
            ("spatialization", self.spatialization.is_some()),
            ("spread", self.spread.is_some()),
            ("attenuation", self.attenuation.is_some()),
            ("effect", self.effect.is_some()),
            ("scope", self.scope.is_some()),
            ("fader", self.fader.is_some()),
            ("pitch", self.pitch.is_some()),
            ("near_field_gain", self.near_field_gain.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

/// Merges `patch` into a copy of `existing`; fields the patch leaves unset are preserved.
pub fn apply_patch(existing: &Sound, patch: &SoundPatch) -> Sound {
    let mut sound = existing.clone();

    if let Some(file) = &patch.audio_file {
        sound.path = Some(file.clone());
    }
    if let Some(bus) = patch.bus {
        sound.bus = bus;
    }
    if let Some(attenuation) = patch.attenuation {
        sound.attenuation = attenuation;
    }
    if let Some(effect) = patch.effect {
        sound.effect = effect;
    }
    if let Some(scope) = patch.scope {
        sound.scope = scope;
    }
    if let Some(fader) = &patch.fader {
        sound.fader = Some(fader.clone());
    }
    if let Some(gain) = &patch.gain {
        sound.gain = Some(gain.clone());
    }
    if let Some(pitch) = &patch.pitch {
        sound.pitch = Some(pitch.clone());
    }
    if let Some(value) = &patch.near_field_gain {
        sound.near_field_gain = Some(value.clone());
    }
    if let Some(priority) = &patch.priority {
        sound.priority = Some(priority.clone());
    }
    if let Some(spread) = &patch.spread {
        sound.spread = Some(spread.clone());
    }
    if let Some(stream) = patch.stream {
        sound.stream = stream;
    }
    if let Some(spatialization) = patch.spatialization {
        sound.spatialization = spatialization;
    }
    sound.loop_ = merged_loop(existing.loop_.as_ref(), patch);
    sound
}

/// Applies the loop flags: a positive count switches looping on, like the CLI always did.
fn merged_loop(current: Option<&SoundLoopConfig>, patch: &SoundPatch) -> Option<SoundLoopConfig> {
    if patch.loop_enabled.is_none() && patch.loop_count.is_none() {
        return current.cloned();
    }
    let mut config = current.cloned().unwrap_or_else(SoundLoopConfig::disabled);
    if let Some(enabled) = patch.loop_enabled {
        config.enabled = enabled;
    }
    if let Some(count) = patch.loop_count {
        config.loop_count = count;
        if count > 0 {
            config.enabled = true;
        }
    }
    Some(config)
}
