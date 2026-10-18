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

//! Interactive prompts for `switch-container create` and `switch-container update`.
//!
//! Every function here is only called when the session can prompt; flag values
//! already present in the patch are never asked for again.

use anyhow::Result;

use crate::assets::extensions::FADER_ALGORITHM_NAMES;
use crate::assets::generated::SwitchContainerUpdateBehavior;
use crate::assets::{AssetType, ProjectValidator, SwitchContainer};
use crate::forms::pick::pick_reference;
use crate::forms::references::rule_for;
use crate::input::{Input, select_index};
use crate::services::switch_container::SwitchContainerPatch;
use crate::services::sound::{DEFAULT_GAIN, DEFAULT_PRIORITY};

use super::sound_prompts::{
    ACCEPT_DEFAULTS_PROMPT, DEFAULT_FADER, DEFAULT_PITCH, SCOPE_LABELS, SCOPES,
    SPATIALIZATION_LABELS, SPATIALIZATIONS, ask_choice, ask_choice_or_unset, ask_reference, ask_rtpc, pick_optional,
    prompt_checked_rtpc, prompt_optional_rtpc, to_labels,
};
use super::switch_container_args::UPDATE_BEHAVIOR_NAMES;
use super::switch_container_entry_prompts::{
    pick_state, prompt_entry_edits, prompt_new_entries,
};

const UPDATE_BEHAVIORS: [SwitchContainerUpdateBehavior; 2] = [
    SwitchContainerUpdateBehavior::UpdateOnPlay,
    SwitchContainerUpdateBehavior::UpdateOnChange,
];

const T: AssetType = AssetType::SwitchContainer;

fn pick_required(input: &dyn Input, validator: &ProjectValidator, field: &str) -> Result<u64> {
    let rule = rule_for(T, field)
        .ok_or_else(|| anyhow::anyhow!("Switch container has no reference rule for '{field}'"))?;
    pick_reference(input, validator, &rule)?
        .ok_or_else(|| anyhow::anyhow!("A required reference cannot be left unset"))
}

/// Picks the bus of a new container; the bus is required, so there is no "(none)".
pub fn prompt_bus(input: &dyn Input, validator: &ProjectValidator) -> Result<u64> {
    pick_required(input, validator, "bus")
}

/// Picks the switch a new container is driven by; required, so there is no "(none)".
pub fn prompt_switch_group(input: &dyn Input, validator: &ProjectValidator) -> Result<u64> {
    pick_required(input, validator, "switch_group")
}

fn is_walkthrough_needed(patch: &SwitchContainerPatch) -> bool {
    patch.effect.is_none()
        || patch.attenuation.is_none()
        || patch.scope.is_none()
        || patch.fader.is_none()
        || patch.gain.is_none()
        || patch.pitch.is_none()
        || patch.priority.is_none()
        || patch.spatialization.is_none()
        || patch.update_behavior.is_none()
        || patch.default_switch_state.is_none()
        || patch.entries.replace.is_none()
}

/// Walks a new container through every optional field that no flag already set.
///
/// After the required bus and switch group (handled by the caller) the author may
/// accept defaults for everything that is left in a single answer. `group` is the
/// switch the states are picked from.
pub fn complete_create_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    patch: SwitchContainerPatch,
    group: u64,
) -> Result<SwitchContainerPatch> {
    if !is_walkthrough_needed(&patch) || input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))? {
        return Ok(patch);
    }

    let effect = match patch.effect {
        Some(id) => id,
        None => pick_optional(input, validator, T, "effect")?,
    };
    let attenuation = match patch.attenuation {
        Some(id) => id,
        None => pick_optional(input, validator, T, "attenuation")?,
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
    let update_behavior = match patch.update_behavior {
        Some(behavior) => behavior,
        None => {
            UPDATE_BEHAVIORS[select_index(
                input,
                "Update behavior",
                &to_labels(UPDATE_BEHAVIOR_NAMES),
            )?]
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
    let default_switch_state = match patch.default_switch_state {
        Some(state) => state,
        None => pick_state(input, validator, group, "Default switch state", &[])?,
    };
    let entries = match patch.entries.replace {
        Some(entries) => entries,
        None => prompt_new_entries(input, validator, group)?,
    };

    Ok(SwitchContainerPatch {
        effect: Some(effect),
        attenuation: Some(attenuation),
        scope: Some(scope),
        fader: Some(fader),
        gain: Some(gain),
        pitch,
        priority: Some(priority),
        spatialization: Some(spatialization),
        update_behavior: Some(update_behavior),
        default_switch_state: Some(default_switch_state),
        entries: crate::services::switch_container_entries::EntriesPatch {
            replace: Some(entries),
            ..patch.entries
        },
        ..patch
    })
}

// -----------------------------------------------------------------------------
// Update
// -----------------------------------------------------------------------------

fn ask_default_state(
    input: &dyn Input,
    validator: &ProjectValidator,
    group: u64,
    current: u64,
) -> Result<Option<u64>> {
    let states = validator.switch_states(group).unwrap_or(&[]);
    if states.is_empty() {
        return Ok(None);
    }
    let labels: Vec<String> = states.iter().map(|s| s.name.clone()).collect();
    let position = states.iter().position(|s| s.id == current);
    Ok(ask_choice_or_unset(input, "Default switch state", &labels, position)?.map(|i| states[i].id))
}

/// Offers every container field with the current value; unchanged fields stay out of the patch.
pub fn prompt_update_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    container: &SwitchContainer,
) -> Result<SwitchContainerPatch> {
    let mut patch = SwitchContainerPatch {
        bus: ask_reference(input, validator, T, "bus", container.bus)?,
        switch_group: ask_reference(input, validator, T, "switch_group", container.switch_group)?,
        attenuation: ask_reference(input, validator, T, "attenuation", container.attenuation)?,
        effect: ask_reference(input, validator, T, "effect", container.effect)?,
        ..Default::default()
    };

    let current_scope = SCOPES.iter().position(|s| *s == container.scope).unwrap_or(0);
    patch.scope =
        ask_choice(input, "Scope", &to_labels(&SCOPE_LABELS), current_scope)?.map(|i| SCOPES[i]);

    let faders = to_labels(FADER_ALGORITHM_NAMES);
    let current_fader = faders
        .iter()
        .position(|f| Some(f.as_str()) == container.fader.as_deref());
    patch.fader = ask_choice_or_unset(input, "Fader", &faders, current_fader)?.map(|i| faders[i].clone());

    let current_behavior = UPDATE_BEHAVIORS
        .iter()
        .position(|b| *b == container.update_behavior)
        .unwrap_or(0);
    patch.update_behavior = ask_choice(
        input,
        "Update behavior",
        &to_labels(UPDATE_BEHAVIOR_NAMES),
        current_behavior,
    )?
    .map(|i| UPDATE_BEHAVIORS[i]);

    patch.gain = ask_rtpc(input, validator, "Gain", &container.gain, DEFAULT_GAIN, Some((0.0, 1.0)))?;
    patch.pitch = ask_rtpc(input, validator, "Pitch", &container.pitch, DEFAULT_PITCH, None)?;
    patch.priority = ask_rtpc(
        input,
        validator,
        "Priority",
        &container.priority,
        DEFAULT_PRIORITY,
        Some((0.0, 255.0)),
    )?;

    let current_spat = SPATIALIZATIONS
        .iter()
        .position(|m| *m == container.spatialization)
        .unwrap_or(0);
    patch.spatialization = ask_choice(
        input,
        "Spatialization mode",
        &to_labels(&SPATIALIZATION_LABELS),
        current_spat,
    )?
    .map(|i| SPATIALIZATIONS[i]);

    let group = patch.switch_group.unwrap_or(container.switch_group);
    if patch.switch_group.is_some() {
        // The old states and entries mean nothing in another switch: ask them again.
        patch.default_switch_state = Some(pick_state(input, validator, group, "Default switch state", &[])?);
        patch.entries.replace = Some(prompt_new_entries(input, validator, group)?);
    } else {
        patch.default_switch_state =
            ask_default_state(input, validator, group, container.default_switch_state)?;
        let entries = container.entries.as_deref().unwrap_or(&[]);
        patch.entries = prompt_entry_edits(input, validator, group, entries)?;
    }
    Ok(patch)
}
