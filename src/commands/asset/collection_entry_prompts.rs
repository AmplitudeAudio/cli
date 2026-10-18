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

//! Interactive prompts for the sound entries of a collection.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::generated::{CollectionEntry, SoundSchedulerMode};
use crate::assets::{AssetType, ProjectValidator};
use crate::forms::pick::pick_reference;
use crate::forms::references::{RefTarget, rule_for};
use crate::input::{Input, select_index};
use crate::services::collection_entries::{DEFAULT_WEIGHT, EntriesPatch, EntrySpec};

use super::sound_prompts::{prompt_optional_rtpc, to_labels};

const NEUTRAL_VALUE: f32 = 1.0;

const DONE_LABEL: &str = "Done";
const ADD_LABEL: &str = "Add an entry";
const REMOVE_LABEL: &str = "Remove an entry";
const CLEAR_LABEL: &str = "Clear all entries";

fn has_sounds(validator: &ProjectValidator) -> bool {
    !validator
        .list(RefTarget::Asset(AssetType::Sound))
        .is_empty()
}

fn prompt_weight(input: &dyn Input) -> Result<f32> {
    let text = input.prompt_text(
        "Weight of this entry (relative to the others)",
        Some(&DEFAULT_WEIGHT.to_string()),
        None,
        Some(&|value: &str| match value.trim().parse::<f32>() {
            Ok(w) if w.is_finite() && w >= 0.0 => Ok(Validation::Valid),
            _ => Ok(Validation::Invalid(
                "Enter a number that is not negative".into(),
            )),
        }),
    )?;
    Ok(text.trim().parse().unwrap_or(DEFAULT_WEIGHT))
}

/// Asks for one entry: its sound, then the weight (Random only) and the optional gain and pitch.
pub fn prompt_entry(
    input: &dyn Input,
    validator: &ProjectValidator,
    mode: SoundSchedulerMode,
) -> Result<EntrySpec> {
    let rule = rule_for(AssetType::Collection, "sounds.sound")
        .ok_or_else(|| anyhow::anyhow!("Collection has no entry sound rule"))?;
    let sound = pick_reference(input, validator, &rule)?
        .ok_or_else(|| anyhow::anyhow!("An entry needs a sound"))?;
    let weight = match mode {
        SoundSchedulerMode::Random => Some(prompt_weight(input)?),
        SoundSchedulerMode::Sequence => None,
    };
    let gain = prompt_optional_rtpc(input, validator, "Entry gain", NEUTRAL_VALUE, None)?;
    let pitch = prompt_optional_rtpc(input, validator, "Entry pitch", NEUTRAL_VALUE, None)?;
    Ok(EntrySpec {
        sound,
        gain,
        pitch,
        weight,
    })
}

/// Collects entries for a new collection; asks nothing when the project has no sound yet.
pub fn prompt_new_entries(
    input: &dyn Input,
    validator: &ProjectValidator,
    mode: SoundSchedulerMode,
) -> Result<Vec<EntrySpec>> {
    let mut entries = Vec::new();
    if !has_sounds(validator) {
        return Ok(entries);
    }
    let mut question = "Add a sound entry?";
    while input.confirm(question, Some(false))? {
        entries.push(prompt_entry(input, validator, mode)?);
        question = "Add another sound entry?";
    }
    Ok(entries)
}

fn sound_label(validator: &ProjectValidator, id: u64) -> String {
    validator
        .resolve(RefTarget::Asset(AssetType::Sound), id)
        .map_or_else(|| format!("sound {id}"), |s| s.name)
}

fn current_sounds(entries: &[CollectionEntry]) -> Vec<u64> {
    let mut ids: Vec<u64> = Vec::new();
    for sound in entries.iter().map(CollectionEntry::sound) {
        if !ids.contains(&sound) {
            ids.push(sound);
        }
    }
    ids
}

/// Offers to edit the entries of an existing collection; the result is empty when nothing changes.
pub fn prompt_entry_edits(
    input: &dyn Input,
    validator: &ProjectValidator,
    current: &[CollectionEntry],
    mode: SoundSchedulerMode,
) -> Result<EntriesPatch> {
    let question = format!("Edit the sound entries? (current: {})", current.len());
    if !input.confirm(&question, Some(false))? {
        return Ok(EntriesPatch::default());
    }

    let mut patch = EntriesPatch::default();
    let mut remaining = current_sounds(current);
    let actions = to_labels(&[DONE_LABEL, ADD_LABEL, REMOVE_LABEL, CLEAR_LABEL]);
    loop {
        match select_index(input, "Entries: choose an action", &actions)? {
            1 => patch.add.push(prompt_entry(input, validator, mode)?),
            2 if !remaining.is_empty() => {
                let labels: Vec<String> = remaining
                    .iter()
                    .map(|id| sound_label(validator, *id))
                    .collect();
                let index = select_index(input, "Remove the entries of which sound", &labels)?;
                patch.remove.push(remaining.remove(index));
            }
            3 => {
                patch = EntriesPatch {
                    replace: Some(Vec::new()),
                    ..Default::default()
                };
                remaining.clear();
            }
            0 => return Ok(patch),
            _ => {}
        }
    }
}
