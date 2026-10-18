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

//! Interactive prompts for the entries of a switch container.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::extensions::FADER_ALGORITHM_NAMES;
use crate::assets::generated::{FadeTransitionSettings, SwitchContainerEntry};
use crate::assets::{AssetType, ProjectValidator};
use crate::forms::pick::pick_reference;
use crate::forms::references::{RefTarget, rule_for};
use crate::input::{Input, select_index};
use crate::services::switch_container_entries::{EntriesPatch, EntrySpec};

use super::sound_prompts::{DEFAULT_FADER, DEFAULT_PITCH, prompt_optional_rtpc, to_labels};

const NEUTRAL_VALUE: f32 = 1.0;
const SUGGESTED_FADE_SECONDS: f32 = 0.5;

const DONE_LABEL: &str = "Done";
const ADD_LABEL: &str = "Add an entry";
const REMOVE_LABEL: &str = "Remove an entry";
const CLEAR_LABEL: &str = "Clear all entries";

/// The question that lets the author skip an entry's optional settings.
const ENTRY_DEFAULTS_PROMPT: &str = "Accept defaults for this entry's optional settings?";

/// Picks one state of the switch `group`, not offering the ones in `taken`.
pub fn pick_state(
    input: &dyn Input,
    validator: &ProjectValidator,
    group: u64,
    prompt: &str,
    taken: &[u64],
) -> Result<u64> {
    let states: Vec<_> = validator
        .switch_states(group)
        .unwrap_or(&[])
        .iter()
        .filter(|s| !taken.contains(&s.id))
        .collect();
    if states.is_empty() {
        return Err(anyhow::anyhow!("{}", validator.describe_states(group)));
    }
    let labels: Vec<String> = states.iter().map(|s| s.name.clone()).collect();
    let index = select_index(input, prompt, &labels)?;
    Ok(states[index].id)
}

fn prompt_states(input: &dyn Input, validator: &ProjectValidator, group: u64) -> Result<Vec<u64>> {
    let mut states = vec![pick_state(input, validator, group, "Switch state of this entry", &[])?];
    let available = validator.switch_states(group).map_or(0, |s| s.len());
    while states.len() < available && input.confirm("Play it in another state too?", Some(false))? {
        states.push(pick_state(input, validator, group, "Another switch state", &states)?);
    }
    Ok(states)
}

fn prompt_seconds(input: &dyn Input, label: &str) -> Result<f64> {
    let text = input.prompt_text(
        label,
        Some(&SUGGESTED_FADE_SECONDS.to_string()),
        None,
        Some(&|value: &str| match value.trim().parse::<f64>() {
            Ok(v) if v.is_finite() && v >= 0.0 => Ok(Validation::Valid),
            _ => Ok(Validation::Invalid("Enter a number of seconds, not negative".into())),
        }),
    )?;
    Ok(text.trim().parse().unwrap_or(f64::from(SUGGESTED_FADE_SECONDS)))
}

/// Asks whether the entry fades and, if so, for the duration and the fader.
fn prompt_fade(input: &dyn Input, label: &str) -> Result<Option<FadeTransitionSettings>> {
    if !input.confirm(&format!("Set a {label} fade?"), Some(false))? {
        return Ok(None);
    }
    let duration = prompt_seconds(input, &format!("Duration of the {label} fade in seconds"))?;
    // Linear first so accepting the first option matches the default fader.
    let mut names = to_labels(FADER_ALGORITHM_NAMES);
    names.sort_by_key(|name| name != DEFAULT_FADER);
    let fader = names[select_index(input, &format!("Fader of the {label} fade"), &names)?].clone();
    Ok(Some(FadeTransitionSettings {
        duration,
        fader: Some(fader),
    }))
}

/// Asks for one entry: its object, the states it plays in, then the optional settings.
pub fn prompt_entry(
    input: &dyn Input,
    validator: &ProjectValidator,
    group: u64,
) -> Result<EntrySpec> {
    let rule = rule_for(AssetType::SwitchContainer, "entries.object")
        .ok_or_else(|| anyhow::anyhow!("Switch container has no entry object rule"))?;
    let object = pick_reference(input, validator, &rule)?
        .ok_or_else(|| anyhow::anyhow!("An entry needs a sound or a collection"))?;
    let mut entry = EntrySpec::new(object, prompt_states(input, validator, group)?);

    if input.confirm(ENTRY_DEFAULTS_PROMPT, Some(false))? {
        return Ok(entry);
    }
    entry.continue_between_states = input.confirm(
        "Keep playing when the state changes between this entry's states?",
        Some(false),
    )?;
    entry.fade_in = prompt_fade(input, "fade-in")?;
    entry.fade_out = prompt_fade(input, "fade-out")?;
    entry.gain = prompt_optional_rtpc(input, validator, "Entry gain", NEUTRAL_VALUE, None)?;
    entry.pitch = prompt_optional_rtpc(input, validator, "Entry pitch", DEFAULT_PITCH, None)?;
    Ok(entry)
}

fn has_objects(validator: &ProjectValidator) -> bool {
    [AssetType::Sound, AssetType::Collection]
        .into_iter()
        .any(|t| !validator.list(RefTarget::Asset(t)).is_empty())
}

/// Collects entries for a new container; asks nothing when the project has no sound or collection yet.
pub fn prompt_new_entries(
    input: &dyn Input,
    validator: &ProjectValidator,
    group: u64,
) -> Result<Vec<EntrySpec>> {
    let mut entries = Vec::new();
    if !has_objects(validator) {
        return Ok(entries);
    }
    let mut question = "Add an entry?";
    while input.confirm(question, Some(false))? {
        entries.push(prompt_entry(input, validator, group)?);
        question = "Add another entry?";
    }
    Ok(entries)
}

fn object_label(validator: &ProjectValidator, id: u64) -> String {
    let target = rule_for(AssetType::SwitchContainer, "entries.object")
        .map_or(RefTarget::Asset(AssetType::Sound), |r| r.target);
    validator
        .resolve(target, id)
        .map_or_else(|| format!("object {id}"), |o| o.name)
}

fn current_objects(entries: &[SwitchContainerEntry]) -> Vec<u64> {
    let mut ids: Vec<u64> = Vec::new();
    for object in entries.iter().map(|e| e.object) {
        if !ids.contains(&object) {
            ids.push(object);
        }
    }
    ids
}

/// Offers to edit the entries of an existing container; the result is empty when nothing changes.
pub fn prompt_entry_edits(
    input: &dyn Input,
    validator: &ProjectValidator,
    group: u64,
    current: &[SwitchContainerEntry],
) -> Result<EntriesPatch> {
    let question = format!("Edit the entries? (current: {})", current.len());
    if !input.confirm(&question, Some(false))? {
        return Ok(EntriesPatch::default());
    }

    let mut patch = EntriesPatch::default();
    let mut remaining = current_objects(current);
    let actions = to_labels(&[DONE_LABEL, ADD_LABEL, REMOVE_LABEL, CLEAR_LABEL]);
    loop {
        match select_index(input, "Entries: choose an action", &actions)? {
            1 => patch.add.push(prompt_entry(input, validator, group)?),
            2 if !remaining.is_empty() => {
                let labels: Vec<String> = remaining
                    .iter()
                    .map(|id| object_label(validator, *id))
                    .collect();
                let index = select_index(input, "Remove the entries of which object", &labels)?;
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
