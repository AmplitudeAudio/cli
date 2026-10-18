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

//! Interactive prompts for `collection create` and `collection update`.
//!
//! Every function here is only called when the session can prompt; flag values
//! already present in the patch are never asked for again.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::extensions::{
    COLLECTION_PLAY_MODE_NAMES, FADER_ALGORITHM_NAMES, SOUND_SCHEDULER_MODE_NAMES,
};
use crate::assets::generated::{
    CollectionPlayMode, SequenceSoundSchedulerEndBehavior, SoundSchedulerConfig, SoundSchedulerMode,
};
use crate::assets::{AssetType, Collection, ProjectValidator};
use crate::forms::pick::pick_reference;
use crate::forms::references::rule_for;
use crate::input::{Input, select_index};
use crate::services::collection::CollectionPatch;
use crate::services::sound::{DEFAULT_GAIN, DEFAULT_PRIORITY};

use super::collection_entry_prompts::{prompt_entry_edits, prompt_new_entries};
use super::sound_prompts::{
    ACCEPT_DEFAULTS_PROMPT, DEFAULT_FADER, DEFAULT_PITCH, SCOPE_LABELS, SCOPES,
    SPATIALIZATION_LABELS, SPATIALIZATIONS, ask_bool, ask_choice, ask_choice_or_unset, ask_reference, ask_rtpc,
    pick_optional, prompt_checked_rtpc, prompt_optional_rtpc, to_labels,
};

const PLAY_MODES: [CollectionPlayMode; 2] =
    [CollectionPlayMode::PlayOne, CollectionPlayMode::PlayAll];
const SCHEDULER_MODES: [SoundSchedulerMode; 2] =
    [SoundSchedulerMode::Random, SoundSchedulerMode::Sequence];
const END_BEHAVIORS: [SequenceSoundSchedulerEndBehavior; 2] = [
    SequenceSoundSchedulerEndBehavior::Restart,
    SequenceSoundSchedulerEndBehavior::Reverse,
];
const END_BEHAVIOR_LABELS: [&str; 2] = ["Restart", "Reverse"];
const DEFAULT_REPEAT_COUNT: u32 = 1;

/// Picks the bus of a new collection; the bus is required, so there is no "(none)".
pub fn prompt_bus(input: &dyn Input, validator: &ProjectValidator) -> Result<u64> {
    let rule = rule_for(AssetType::Collection, "bus")
        .ok_or_else(|| anyhow::anyhow!("Collection has no bus reference rule"))?;
    pick_reference(input, validator, &rule)?
        .ok_or_else(|| anyhow::anyhow!("A required reference cannot be left unset"))
}

fn prompt_repeat_count(input: &dyn Input, current: u32) -> Result<u32> {
    let text = input.prompt_text(
        "Repeat count (how many recent sounds to avoid)",
        Some(&current.to_string()),
        None,
        Some(&|value: &str| match value.trim().parse::<u32>() {
            Ok(_) => Ok(Validation::Valid),
            Err(_) => Ok(Validation::Invalid("Must be a non-negative number".into())),
        }),
    )?;
    Ok(text.trim().parse().unwrap_or(current))
}

fn is_walkthrough_needed(patch: &CollectionPatch) -> bool {
    patch.effect.is_none()
        || patch.attenuation.is_none()
        || patch.scope.is_none()
        || patch.fader.is_none()
        || patch.play_mode.is_none()
        || patch.scheduler_mode.is_none()
        || (patch.avoid_repeat.is_none()
            && patch.repeat_count.is_none()
            && patch.end_behavior.is_none())
        || patch.gain.is_none()
        || patch.pitch.is_none()
        || patch.priority.is_none()
        || patch.spatialization.is_none()
        || patch.entries.replace.is_none()
}

/// Asks the scheduler settings the flags left open, for the scheduler `mode`.
fn complete_scheduler_config(
    input: &dyn Input,
    mode: SoundSchedulerMode,
    patch: &CollectionPatch,
) -> Result<(
    Option<bool>,
    Option<u32>,
    Option<SequenceSoundSchedulerEndBehavior>,
)> {
    let nothing_given = patch.avoid_repeat.is_none()
        && patch.repeat_count.is_none()
        && patch.end_behavior.is_none();
    if !nothing_given {
        return Ok((patch.avoid_repeat, patch.repeat_count, patch.end_behavior));
    }
    match mode {
        SoundSchedulerMode::Random => {
            let avoid = input.confirm("Avoid repeating recent sounds?", Some(false))?;
            let count = if avoid {
                Some(prompt_repeat_count(input, DEFAULT_REPEAT_COUNT)?)
            } else {
                None
            };
            Ok((Some(avoid), count, None))
        }
        SoundSchedulerMode::Sequence => {
            let index = select_index(
                input,
                "End behavior of the sequence",
                &to_labels(&END_BEHAVIOR_LABELS),
            )?;
            Ok((None, None, Some(END_BEHAVIORS[index])))
        }
    }
}

/// Walks a new collection through every optional field that no flag already set.
///
/// After the required bus (handled by the caller) the author may accept
/// defaults for everything that is left in a single answer.
pub fn complete_create_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    patch: CollectionPatch,
) -> Result<CollectionPatch> {
    if !is_walkthrough_needed(&patch) || input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))? {
        return Ok(patch);
    }

    let effect = match patch.effect {
        Some(id) => id,
        None => pick_optional(input, validator, AssetType::Collection, "effect")?,
    };
    let attenuation = match patch.attenuation {
        Some(id) => id,
        None => pick_optional(input, validator, AssetType::Collection, "attenuation")?,
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
    let play_mode = match patch.play_mode {
        Some(mode) => mode,
        None => {
            PLAY_MODES[select_index(input, "Play mode", &to_labels(COLLECTION_PLAY_MODE_NAMES))?]
        }
    };
    let mode = match patch.scheduler_mode {
        Some(mode) => mode,
        None => {
            SCHEDULER_MODES[select_index(
                input,
                "Scheduler mode",
                &to_labels(SOUND_SCHEDULER_MODE_NAMES),
            )?]
        }
    };
    let (avoid_repeat, repeat_count, end_behavior) =
        complete_scheduler_config(input, mode, &patch)?;
    let gain = match &patch.gain {
        Some(v) => v.clone(),
        None => prompt_checked_rtpc(input, validator, "Gain", DEFAULT_GAIN, Some((0.0, 1.0)))?,
    };
    let pitch = match &patch.pitch {
        Some(v) => Some(v.clone()),
        None => prompt_optional_rtpc(input, validator, "Pitch", DEFAULT_PITCH, None)?,
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
    let spatialization = match patch.spatialization {
        Some(mode) => mode,
        None => {
            SPATIALIZATIONS[select_index(
                input,
                "Spatialization mode",
                &to_labels(&SPATIALIZATION_LABELS),
            )?]
        }
    };
    let entries = match patch.entries.replace {
        Some(entries) => entries,
        None => prompt_new_entries(input, validator, mode)?,
    };

    Ok(CollectionPatch {
        effect: Some(effect),
        attenuation: Some(attenuation),
        scope: Some(scope),
        fader: Some(fader),
        gain: Some(gain),
        pitch,
        priority: Some(priority),
        spatialization: Some(spatialization),
        play_mode: Some(play_mode),
        scheduler_mode: Some(mode),
        avoid_repeat,
        repeat_count,
        end_behavior,
        entries: crate::services::collection_entries::EntriesPatch {
            replace: Some(entries),
            ..patch.entries
        },
        ..patch
    })
}

// -----------------------------------------------------------------------------
// Update
// -----------------------------------------------------------------------------

/// The scheduler settings that survive the mode the user ends up with.
struct CurrentConfig {
    avoid_repeat: bool,
    repeat_count: u32,
    end_behavior: SequenceSoundSchedulerEndBehavior,
}

fn current_config(collection: &Collection, mode: SoundSchedulerMode) -> CurrentConfig {
    let mut current = CurrentConfig {
        avoid_repeat: false,
        repeat_count: DEFAULT_REPEAT_COUNT,
        end_behavior: SequenceSoundSchedulerEndBehavior::Restart,
    };
    let same_mode = collection.scheduler.as_ref().filter(|s| s.mode == mode);
    match same_mode.and_then(|s| s.config.as_ref()) {
        Some(SoundSchedulerConfig::Random(c)) => {
            current.avoid_repeat = c.avoid_repeat;
            current.repeat_count = c.repeat_count;
        }
        Some(SoundSchedulerConfig::Sequence(c)) => current.end_behavior = c.end_behavior,
        None => {}
    }
    current
}

/// Offers the settings of the scheduler the collection ends up using.
fn ask_scheduler_config(
    input: &dyn Input,
    collection: &Collection,
    mode: SoundSchedulerMode,
    patch: &mut CollectionPatch,
) -> Result<()> {
    let current = current_config(collection, mode);
    match mode {
        SoundSchedulerMode::Random => {
            patch.avoid_repeat =
                ask_bool(input, "Avoid repeating recent sounds", current.avoid_repeat)?;
            if patch.avoid_repeat.unwrap_or(current.avoid_repeat) {
                let count = prompt_repeat_count(input, current.repeat_count)?;
                patch.repeat_count = (count != current.repeat_count).then_some(count);
            }
        }
        SoundSchedulerMode::Sequence => {
            let position = END_BEHAVIORS
                .iter()
                .position(|b| *b == current.end_behavior)
                .unwrap_or(0);
            patch.end_behavior = ask_choice(
                input,
                "End behavior of the sequence",
                &to_labels(&END_BEHAVIOR_LABELS),
                position,
            )?
            .map(|i| END_BEHAVIORS[i]);
        }
    }
    Ok(())
}

/// Offers every collection field with the current value; unchanged fields stay out of the patch.
pub fn prompt_update_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    collection: &Collection,
) -> Result<CollectionPatch> {
    let mut patch = CollectionPatch {
        bus: ask_reference(
            input,
            validator,
            AssetType::Collection,
            "bus",
            collection.bus,
        )?,
        attenuation: ask_reference(
            input,
            validator,
            AssetType::Collection,
            "attenuation",
            collection.attenuation,
        )?,
        effect: ask_reference(
            input,
            validator,
            AssetType::Collection,
            "effect",
            collection.effect,
        )?,
        ..Default::default()
    };

    let current_scope = SCOPES
        .iter()
        .position(|s| *s == collection.scope)
        .unwrap_or(0);
    patch.scope =
        ask_choice(input, "Scope", &to_labels(&SCOPE_LABELS), current_scope)?.map(|i| SCOPES[i]);

    let faders = to_labels(FADER_ALGORITHM_NAMES);
    let current_fader = faders
        .iter()
        .position(|f| Some(f.as_str()) == collection.fader.as_deref());
    patch.fader = ask_choice_or_unset(input, "Fader", &faders, current_fader)?.map(|i| faders[i].clone());

    let current_play = PLAY_MODES
        .iter()
        .position(|m| *m == collection.play_mode)
        .unwrap_or(0);
    patch.play_mode = ask_choice(
        input,
        "Play mode",
        &to_labels(COLLECTION_PLAY_MODE_NAMES),
        current_play,
    )?
    .map(|i| PLAY_MODES[i]);

    let current_mode = collection
        .scheduler
        .as_ref()
        .map_or(SoundSchedulerMode::Random, |s| s.mode);
    let current_mode_index = SCHEDULER_MODES
        .iter()
        .position(|m| *m == current_mode)
        .unwrap_or(0);
    patch.scheduler_mode = ask_choice(
        input,
        "Scheduler mode",
        &to_labels(SOUND_SCHEDULER_MODE_NAMES),
        current_mode_index,
    )?
    .map(|i| SCHEDULER_MODES[i]);
    let mode = patch.scheduler_mode.unwrap_or(current_mode);
    ask_scheduler_config(input, collection, mode, &mut patch)?;

    patch.gain = ask_rtpc(
        input,
        validator,
        "Gain",
        &collection.gain,
        DEFAULT_GAIN,
        Some((0.0, 1.0)),
    )?;
    patch.pitch = ask_rtpc(
        input,
        validator,
        "Pitch",
        &collection.pitch,
        DEFAULT_PITCH,
        None,
    )?;
    patch.priority = ask_rtpc(
        input,
        validator,
        "Priority",
        &collection.priority,
        DEFAULT_PRIORITY,
        Some((0.0, 255.0)),
    )?;

    let current_spat = SPATIALIZATIONS
        .iter()
        .position(|m| *m == collection.spatialization)
        .unwrap_or(0);
    patch.spatialization = ask_choice(
        input,
        "Spatialization mode",
        &to_labels(&SPATIALIZATION_LABELS),
        current_spat,
    )?
    .map(|i| SPATIALIZATIONS[i]);

    let entries = collection.sounds.as_deref().unwrap_or(&[]);
    patch.entries = prompt_entry_edits(input, validator, entries, mode)?;
    Ok(patch)
}
