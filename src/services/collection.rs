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

//! Collection creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI all produce a [`CollectionValues`]
//! (create) or a [`CollectionPatch`] (update); this module turns them into a
//! validated [`Collection`] without touching the terminal or the file system.
//! Functions return new values and never mutate their inputs.

use crate::assets::generated::{
    CollectionPlayMode, RandomSoundSchedulerConfig, Scope, SequenceSoundSchedulerConfig,
    SequenceSoundSchedulerEndBehavior, SoundSchedulerConfig, SoundSchedulerMode,
    SoundSchedulerSettings, Spatialization,
};
use crate::assets::{
    Asset, AssetType, Collection, FaderAlgorithm, ProjectContext, RtpcCompatibleValue,
    ValidationError, ValidationLayer,
};
use crate::common::errors::codes::ERR_VALIDATION_FIELD;
use crate::services::collection_entries::{EntriesPatch, EntrySpec, apply_entries, retag};
use crate::services::sound::{DEFAULT_GAIN, DEFAULT_PRIORITY, require_bus, unique_id};

/// Repeat count of a Random scheduler config that sets only `avoid_repeat` (the schema default).
const DEFAULT_REPEAT_COUNT: u32 = 1;

/// Every value a new collection needs. Reference fields use `0` for "unset".
#[derive(Debug, Clone, PartialEq)]
pub struct CollectionValues {
    pub name: String,
    pub bus: u64,
    pub effect: u64,
    pub attenuation: u64,
    pub scope: Scope,
    pub fader: String,
    pub gain: RtpcCompatibleValue,
    pub pitch: Option<RtpcCompatibleValue>,
    pub priority: RtpcCompatibleValue,
    pub spatialization: Spatialization,
    pub play_mode: CollectionPlayMode,
    pub scheduler: SoundSchedulerSettings,
    pub entries: Vec<EntrySpec>,
}

impl CollectionValues {
    /// Fills a patch's gaps with defaults. The bus has none: a missing bus becomes
    /// the invalid id 0, which [`build_collection`] rejects.
    pub fn from_patch(name: &str, patch: &CollectionPatch) -> Result<Self, ValidationError> {
        let scheduler = merged_scheduler(None, patch)?;
        let entries = patch
            .entries
            .replace
            .iter()
            .flatten()
            .chain(&patch.entries.add)
            .cloned()
            .collect::<Vec<_>>();
        // Surface a weight on a Sequence collection here, with the other flag errors.
        apply_entries(&[], &patch.entries, scheduler.mode)?;

        Ok(Self {
            name: name.to_string(),
            bus: patch.bus.unwrap_or(0),
            effect: patch.effect.unwrap_or(0),
            attenuation: patch.attenuation.unwrap_or(0),
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
            priority: patch
                .priority
                .clone()
                .unwrap_or_else(|| RtpcCompatibleValue::static_value(DEFAULT_PRIORITY)),
            spatialization: patch.spatialization.unwrap_or(Spatialization::None),
            play_mode: patch.play_mode.unwrap_or_default(),
            scheduler,
            entries,
        })
    }
}

/// Builds a validated collection from `values`, generating a unique id.
pub fn build_collection(
    values: CollectionValues,
    ctx: &ProjectContext,
) -> Result<Collection, ValidationError> {
    require_bus(AssetType::Collection, values.bus)?;
    let id = unique_id("collection", &values.name, ctx)?;
    let mode = values.scheduler.mode;
    let sounds = apply_entries(
        &[],
        &EntriesPatch {
            replace: Some(values.entries),
            ..Default::default()
        },
        mode,
    )?;

    let mut collection = Collection::builder(id, &values.name)
        .bus(values.bus)
        .effect(values.effect)
        .attenuation(values.attenuation)
        .scope(values.scope)
        .gain_rtpc(values.gain)
        .priority_rtpc(values.priority)
        .spatialization(values.spatialization)
        .play_mode(values.play_mode)
        .build();
    collection.fader = Some(values.fader);
    collection.pitch = values.pitch;
    collection.scheduler = Some(values.scheduler);
    collection.sounds = Some(sounds);

    collection.validate_rules(ctx)?;
    Ok(collection)
}

/// The fields of an existing collection a caller wants to change; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CollectionPatch {
    pub bus: Option<u64>,
    pub effect: Option<u64>,
    pub attenuation: Option<u64>,
    pub scope: Option<Scope>,
    pub fader: Option<String>,
    pub gain: Option<RtpcCompatibleValue>,
    pub pitch: Option<RtpcCompatibleValue>,
    pub priority: Option<RtpcCompatibleValue>,
    pub spatialization: Option<Spatialization>,
    pub play_mode: Option<CollectionPlayMode>,
    pub scheduler_mode: Option<SoundSchedulerMode>,
    /// Random scheduler: avoid replaying recent sounds.
    pub avoid_repeat: Option<bool>,
    /// Random scheduler: how many recent sounds `avoid_repeat` remembers.
    pub repeat_count: Option<u32>,
    /// Sequence scheduler: what happens at the end of the list.
    pub end_behavior: Option<SequenceSoundSchedulerEndBehavior>,
    pub entries: EntriesPatch,
}

impl CollectionPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Whether any scheduler setting (mode or config) is set.
    fn touches_scheduler(&self) -> bool {
        self.scheduler_mode.is_some()
            || self.avoid_repeat.is_some()
            || self.repeat_count.is_some()
            || self.end_behavior.is_some()
    }

    /// Names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("play_mode", self.play_mode.is_some()),
            ("scheduler_mode", self.scheduler_mode.is_some()),
            ("gain", self.gain.is_some()),
            ("bus", self.bus.is_some()),
            ("priority", self.priority.is_some()),
            ("spatialization", self.spatialization.is_some()),
            ("effect", self.effect.is_some()),
            ("attenuation", self.attenuation.is_some()),
            ("pitch", self.pitch.is_some()),
            ("scope", self.scope.is_some()),
            ("fader", self.fader.is_some()),
            ("avoid_repeat", self.avoid_repeat.is_some()),
            ("repeat_count", self.repeat_count.is_some()),
            ("end_behavior", self.end_behavior.is_some()),
            ("entries", !self.entries.is_empty()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

fn config_mismatch(flags: &str, mode: SoundSchedulerMode) -> ValidationError {
    let (applies_to, other) = match mode {
        SoundSchedulerMode::Random => ("Sequence", "Random"),
        SoundSchedulerMode::Sequence => ("Random", "Sequence"),
    };
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        format!("{flags} only applies to the {applies_to} scheduler"),
        format!("The collection uses the {other} scheduler, which has no such setting"),
    )
    .with_suggestion(format!(
        "Drop {flags}, or switch the collection with --scheduler-mode {applies_to}"
    ))
    .with_field("scheduler")
}

/// The scheduler settings after `patch`.
///
/// A mode change discards the config of the old scheduler (the SDK would read
/// a config of the wrong kind as absent); an unchanged mode keeps the existing
/// config and applies only the flags given.
fn merged_scheduler(
    current: Option<&SoundSchedulerSettings>,
    patch: &CollectionPatch,
) -> Result<SoundSchedulerSettings, ValidationError> {
    let mode = patch
        .scheduler_mode
        .or(current.map(|c| c.mode))
        .unwrap_or(SoundSchedulerMode::Random);
    let base = current
        .filter(|c| c.mode == mode)
        .and_then(|c| c.config.clone());
    let random_flags = patch.avoid_repeat.is_some() || patch.repeat_count.is_some();

    let config = match mode {
        SoundSchedulerMode::Random => {
            if patch.end_behavior.is_some() {
                return Err(config_mismatch("--end-behavior", mode));
            }
            if random_flags {
                let mut config = match base {
                    Some(SoundSchedulerConfig::Random(config)) => config,
                    _ => RandomSoundSchedulerConfig {
                        avoid_repeat: false,
                        repeat_count: DEFAULT_REPEAT_COUNT,
                    },
                };
                config.avoid_repeat = patch.avoid_repeat.unwrap_or(config.avoid_repeat);
                config.repeat_count = patch.repeat_count.unwrap_or(config.repeat_count);
                Some(SoundSchedulerConfig::Random(config))
            } else {
                base
            }
        }
        SoundSchedulerMode::Sequence => {
            if random_flags {
                return Err(config_mismatch("--avoid-repeat/--repeat-count", mode));
            }
            match patch.end_behavior {
                Some(end_behavior) => Some(SoundSchedulerConfig::Sequence(
                    SequenceSoundSchedulerConfig { end_behavior },
                )),
                None => base,
            }
        }
    };
    Ok(SoundSchedulerSettings { mode, config })
}

/// Merges `patch` into a copy of `existing`; fields the patch leaves unset are preserved.
///
/// Changing the scheduler mode re-tags the entries for the new scheduler (see
/// [`retag`]) and drops the old scheduler's config. Nothing else touches the
/// scheduler config or the entries unless the patch names them.
pub fn apply_patch(
    existing: &Collection,
    patch: &CollectionPatch,
) -> Result<Collection, ValidationError> {
    let mut collection = existing.clone();

    if let Some(bus) = patch.bus {
        collection.bus = bus;
    }
    if let Some(effect) = patch.effect {
        collection.effect = effect;
    }
    if let Some(attenuation) = patch.attenuation {
        collection.attenuation = attenuation;
    }
    if let Some(scope) = patch.scope {
        collection.scope = scope;
    }
    if let Some(fader) = &patch.fader {
        collection.fader = Some(fader.clone());
    }
    if let Some(gain) = &patch.gain {
        collection.gain = Some(gain.clone());
    }
    if let Some(pitch) = &patch.pitch {
        collection.pitch = Some(pitch.clone());
    }
    if let Some(priority) = &patch.priority {
        collection.priority = Some(priority.clone());
    }
    if let Some(spatialization) = patch.spatialization {
        collection.spatialization = spatialization;
    }
    if let Some(play_mode) = patch.play_mode {
        collection.play_mode = play_mode;
    }

    if patch.touches_scheduler() {
        collection.scheduler = Some(merged_scheduler(existing.scheduler.as_ref(), patch)?);
    }
    let old_mode = existing.scheduler.as_ref().map(|s| s.mode);
    let mode = collection
        .scheduler
        .as_ref()
        .map_or(SoundSchedulerMode::Random, |s| s.mode);
    let mode_changed = patch.scheduler_mode.is_some() && old_mode != Some(mode);

    if mode_changed || !patch.entries.is_empty() {
        let current = match (&existing.sounds, mode_changed) {
            (Some(sounds), true) => retag(sounds, mode),
            (Some(sounds), false) => sounds.clone(),
            (None, _) => Vec::new(),
        };
        collection.sounds = Some(apply_entries(&current, &patch.entries, mode)?);
    }
    Ok(collection)
}
