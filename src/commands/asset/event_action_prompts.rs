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

//! Interactive prompts for single event actions: type, targets, `active` and `scope`.
//!
//! Every function here is only called when the session can prompt.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::event_actions::new_action;
use crate::assets::generated::{EventActionDefinition, EventActionType};
use crate::assets::{AssetType, ProjectValidator};
use crate::forms::pick::pick_reference;
use crate::forms::references::{RefRule, rule_for};
use crate::input::{Input, select_index};

use super::sound_prompts::{SCOPE_LABELS, SCOPES, ask_bool, ask_choice, to_labels};

/// The action types a user can pick (None does nothing, so it is not offered).
const TYPES: [EventActionType; 8] = [
    EventActionType::Play,
    EventActionType::Pause,
    EventActionType::Resume,
    EventActionType::Stop,
    EventActionType::Seek,
    EventActionType::MuteBus,
    EventActionType::UnmuteBus,
    EventActionType::Wait,
];
const TYPE_LABELS: [&str; 8] = [
    "Play",
    "Pause",
    "Resume",
    "Stop",
    "Seek",
    "Mute bus",
    "Unmute bus",
    "Wait",
];

/// Whether the action's scope matters: bus and wait actions ignore it.
fn uses_scope(type_: EventActionType) -> bool {
    matches!(
        type_,
        EventActionType::Play
            | EventActionType::Pause
            | EventActionType::Resume
            | EventActionType::Stop
    )
}

fn target_rule(type_: EventActionType) -> Result<RefRule> {
    let rule = rule_for(AssetType::Event, "actions.targets")
        .ok_or_else(|| anyhow::anyhow!("Event has no reference rule for 'actions.targets'"))?;
    let target = type_
        .target_kinds()
        .ok_or_else(|| anyhow::anyhow!("{type_:?} actions have no reference targets"))?;
    Ok(RefRule { target, ..rule })
}

fn prompt_duration(input: &dyn Input, current: Option<u64>) -> Result<u64> {
    let text = input.prompt_text(
        "Wait duration in milliseconds",
        Some(&current.unwrap_or(500).to_string()),
        None,
        Some(&|value: &str| match value.trim().parse::<u64>() {
            Ok(_) => Ok(Validation::Valid),
            Err(_) => Ok(Validation::Invalid("Must be a non-negative number".into())),
        }),
    )?;
    Ok(text.trim().parse()?)
}

/// Picks the targets of an action of `type_`: one or more objects of the kinds it can
/// target, or the duration of a Wait.
pub fn prompt_targets(
    input: &dyn Input,
    validator: &ProjectValidator,
    type_: EventActionType,
    current_wait: Option<u64>,
) -> Result<Vec<u64>> {
    if type_ == EventActionType::Wait {
        return Ok(vec![prompt_duration(input, current_wait)?]);
    }
    let rule = target_rule(type_)?;
    let mut targets: Vec<u64> = Vec::new();
    loop {
        if let Some(id) = pick_reference(input, validator, &rule)?
            && !targets.contains(&id)
        {
            targets.push(id);
        }
        if !input.confirm("Add another target?", Some(false))? {
            return Ok(targets);
        }
    }
}

/// Asks the type and targets of a new action; `active` and `scope` keep their defaults.
pub fn prompt_action(input: &dyn Input, validator: &ProjectValidator) -> Result<EventActionDefinition> {
    let type_ = TYPES[select_index(input, "Action type", &to_labels(&TYPE_LABELS))?];
    let targets = prompt_targets(input, validator, type_, None)?;
    Ok(new_action(type_, targets))
}

/// Asks the actions of a new event: at least one, then as many as the user wants.
pub fn prompt_new_actions(
    input: &dyn Input,
    validator: &ProjectValidator,
) -> Result<Vec<EventActionDefinition>> {
    let mut actions = vec![prompt_action(input, validator)?];
    while input.confirm("Add another action?", Some(false))? {
        actions.push(prompt_action(input, validator)?);
    }
    Ok(actions)
}

/// Asks `active` and, when it matters, `scope` of a new action.
pub fn prompt_settings(
    input: &dyn Input,
    action: &EventActionDefinition,
    number: usize,
) -> Result<EventActionDefinition> {
    let mut action = action.clone();
    action.active = input.confirm(&format!("Is action {number} active?"), Some(true))?;
    if uses_scope(action.type_) {
        action.scope = SCOPES[select_index(
            input,
            &format!("Scope of action {number}"),
            &to_labels(&SCOPE_LABELS),
        )?];
    }
    Ok(action)
}

/// One-line description of an action for the pick lists.
pub fn describe_action(
    validator: &ProjectValidator,
    index: usize,
    action: &EventActionDefinition,
) -> String {
    let targets = action.targets.as_deref().unwrap_or(&[]);
    let shown = if action.type_ == EventActionType::Wait {
        targets.iter().map(|ms| format!("{ms} ms")).collect::<Vec<_>>()
    } else {
        targets
            .iter()
            .map(|id| {
                action
                    .type_
                    .target_kinds()
                    .and_then(|kinds| validator.resolve(kinds, *id))
                    .map_or_else(|| id.to_string(), |r| r.name)
            })
            .collect()
    };
    let state = if action.active { "" } else { " (inactive)" };
    format!("{index}: {:?} -> {}{state}", action.type_, shown.join(", "))
}

/// Walks one existing action field by field with its current values.
pub fn edit_action(
    input: &dyn Input,
    validator: &ProjectValidator,
    action: &EventActionDefinition,
) -> Result<EventActionDefinition> {
    let current_type = TYPES.iter().position(|t| *t == action.type_).unwrap_or(0);
    let new_type = ask_choice(input, "Action type", &to_labels(&TYPE_LABELS), current_type)?
        .map_or(action.type_, |i| TYPES[i]);

    let wait = action.targets.as_deref().and_then(|t| t.first().copied());
    // Other kinds of target are invalid for another type, so a type change re-asks them.
    let change_targets = new_type != action.type_
        || input.confirm("Change the targets?", Some(false))?;
    let targets = if change_targets {
        prompt_targets(input, validator, new_type, wait.filter(|_| action.type_ == EventActionType::Wait))?
    } else {
        action.targets.clone().unwrap_or_default()
    };

    let active = ask_bool(input, "Active", action.active)?.unwrap_or(action.active);
    let scope = if uses_scope(new_type) {
        let current = SCOPES.iter().position(|s| *s == action.scope).unwrap_or(0);
        ask_choice(input, "Scope", &to_labels(&SCOPE_LABELS), current)?
            .map_or(action.scope, |i| SCOPES[i])
    } else {
        action.scope
    };

    Ok(EventActionDefinition {
        type_: new_type,
        active,
        scope,
        targets: Some(targets),
    })
}

const MENU: [&str; 4] = [
    "Add an action",
    "Edit an action",
    "Remove an action",
    "Done",
];

/// Lets the user add, edit and remove actions; returns the new list when it changed.
pub fn prompt_action_edits(
    input: &dyn Input,
    validator: &ProjectValidator,
    current: &[EventActionDefinition],
) -> Result<Option<Vec<EventActionDefinition>>> {
    let mut actions = current.to_vec();
    loop {
        let choice = select_index(input, "What to do with the actions?", &to_labels(&MENU))?;
        let labels: Vec<String> = actions
            .iter()
            .enumerate()
            .map(|(i, a)| describe_action(validator, i, a))
            .collect();
        match choice {
            0 => {
                let action = prompt_action(input, validator)?;
                let number = actions.len() + 1;
                actions.push(prompt_settings(input, &action, number)?);
            }
            1 | 2 if actions.is_empty() => {}
            1 => {
                let index = select_index(input, "Action to edit", &labels)?;
                actions[index] = edit_action(input, validator, &actions[index])?;
            }
            2 => {
                let index = select_index(input, "Action to remove", &labels)?;
                actions.remove(index);
            }
            _ => break,
        }
    }
    Ok((actions != current).then_some(actions))
}
