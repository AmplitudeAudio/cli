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

//! Interactive prompts for `switch create` and `switch update`.
//!
//! Every function here is only called when the session can prompt; flag values
//! already present in the patch are never asked for again.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::Switch;
use crate::assets::generated::SwitchStateDefinition;
use crate::input::{Input, select_index};
use crate::services::switch::{StateSpec, StatesPatch, SwitchPatch, default_state_id};

use super::sound_prompts::{ACCEPT_DEFAULTS_PROMPT, to_labels};

const MENU: [&str; 5] = [
    "Add a state",
    "Rename a state",
    "Change a state ID",
    "Remove a state",
    "Done",
];
const UNNAMED: &str = "(unnamed)";

fn name_validator(taken: Vec<String>) -> impl Fn(&str) -> Result<Validation, inquire::CustomUserError> {
    move |value: &str| {
        let value = value.trim();
        Ok(if value.is_empty() {
            Validation::Invalid("A state needs a name".into())
        } else if taken.iter().any(|t| t == value) {
            Validation::Invalid("Another state already has this name".into())
        } else {
            Validation::Valid
        })
    }
}

fn id_validator(taken: Vec<u64>) -> impl Fn(&str) -> Result<Validation, inquire::CustomUserError> {
    move |value: &str| {
        Ok(match value.trim().parse::<u64>() {
            Ok(0) | Err(_) => Validation::Invalid("Enter a non-zero whole number".into()),
            Ok(id) if taken.contains(&id) => {
                Validation::Invalid("Another state already has this ID".into())
            }
            Ok(_) => Validation::Valid,
        })
    }
}

/// Asks for state names until the author is done.
fn prompt_state_names(input: &dyn Input) -> Result<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    loop {
        let validator = name_validator(names.clone());
        let name = input.prompt_text(
            "State name (e.g. wood, stone)",
            None,
            None,
            Some(&validator),
        )?;
        names.push(name.trim().to_string());
        if !input.confirm("Add another state?", Some(true))? {
            return Ok(names);
        }
    }
}

/// Walks a new switch through its states and, unless the author accepts the defaults,
/// the id of each.
pub fn complete_create_patch(
    input: &dyn Input,
    switch_name: &str,
    patch: SwitchPatch,
) -> Result<SwitchPatch> {
    if patch.states.replace.is_some() || !patch.states.add.is_empty() {
        return Ok(patch);
    }
    let names = prompt_state_names(input)?;
    let accept_defaults = input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))?;
    let mut specs: Vec<StateSpec> = Vec::with_capacity(names.len());
    for name in names {
        let id = if accept_defaults {
            None
        } else {
            let taken: Vec<u64> = specs.iter().filter_map(|s| s.id).collect();
            let default = default_state_id(switch_name, &name, &taken);
            let validator = id_validator(taken);
            let text = input.prompt_text_with_default(
                &format!("ID of state '{name}'"),
                &default.to_string(),
                Some(&validator),
            )?;
            Some(text.trim().parse::<u64>()?)
        };
        specs.push(StateSpec { name, id });
    }
    Ok(SwitchPatch {
        states: StatesPatch {
            replace: Some(specs),
            ..patch.states
        },
    })
}

fn label(state: &SwitchStateDefinition) -> String {
    format!("{} (ID {})", state.name.as_deref().unwrap_or(UNNAMED), state.id)
}

fn prompt_add(
    input: &dyn Input,
    switch_name: &str,
    states: &[SwitchStateDefinition],
) -> Result<SwitchStateDefinition> {
    let taken_names = states.iter().filter_map(|s| s.name.clone()).collect();
    let validator = name_validator(taken_names);
    let name = input
        .prompt_text("New state name", None, None, Some(&validator))?
        .trim()
        .to_string();
    let ids: Vec<u64> = states.iter().map(|s| s.id).collect();
    Ok(SwitchStateDefinition {
        id: default_state_id(switch_name, &name, &ids),
        name: Some(name),
    })
}

fn prompt_rename(input: &dyn Input, state: &SwitchStateDefinition, others: Vec<String>) -> Result<String> {
    let validator = name_validator(others);
    let current = state.name.clone().unwrap_or_default();
    Ok(input
        .prompt_text_with_default("New name", &current, Some(&validator))?
        .trim()
        .to_string())
}

fn prompt_new_id(input: &dyn Input, state: &SwitchStateDefinition, others: Vec<u64>) -> Result<u64> {
    let validator = id_validator(others);
    let text = input.prompt_text_with_default("New ID", &state.id.to_string(), Some(&validator))?;
    Ok(text.trim().parse::<u64>()?)
}

/// Lets the user add, rename, re-number and remove states; unchanged lists stay out of the patch.
///
/// A state with no name or with id 0 (a file the SDK would reject) shows as such and can
/// be fixed here.
pub fn prompt_update_patch(input: &dyn Input, switch: &Switch) -> Result<SwitchPatch> {
    let current = switch.states.clone().unwrap_or_default();
    let mut states = current.clone();
    let switch_name = switch.name.as_deref().unwrap_or("");
    loop {
        let choice = select_index(input, "What to do with the states?", &to_labels(&MENU))?;
        let labels: Vec<String> = states.iter().map(label).collect();
        match choice {
            0 => {
                let added = prompt_add(input, switch_name, &states)?;
                states.push(added);
            }
            1..=3 if states.is_empty() => {}
            1 => {
                let index = select_index(input, "State to rename", &labels)?;
                let others = other_names(&states, index);
                states[index].name = Some(prompt_rename(input, &states[index], others)?);
            }
            2 => {
                let index = select_index(input, "State to change", &labels)?;
                let others = states
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != index)
                    .map(|(_, s)| s.id)
                    .collect();
                states[index].id = prompt_new_id(input, &states[index], others)?;
            }
            3 => {
                let index = select_index(input, "State to remove", &labels)?;
                states.remove(index);
            }
            _ => break,
        }
    }
    if states == current {
        return Ok(SwitchPatch::default());
    }
    let specs = states
        .into_iter()
        .map(|s| StateSpec {
            name: s.name.unwrap_or_default(),
            id: Some(s.id),
        })
        .collect();
    Ok(SwitchPatch {
        states: StatesPatch {
            replace: Some(specs),
            ..Default::default()
        },
    })
}

fn other_names(states: &[SwitchStateDefinition], index: usize) -> Vec<String> {
    states
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .filter_map(|(_, s)| s.name.clone())
        .collect()
}
