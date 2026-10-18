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

//! Event action vocabulary and the rules for an action's targets.
//!
//! What each action type targets comes from `Event.cpp` of the SDK:
//!
//! - Play/Pause/Resume/Stop/Seek: `Engine::Play(id)` resolves a sound, a
//!   collection or a switch container; Pause/Resume/Stop compare the id with
//!   the id of the playing object (the same three kinds).
//! - MuteBus/UnmuteBus: `Engine::FindBus(id)`.
//! - Wait: `_targets[0]` is the duration in milliseconds (an empty list reads
//!   past the vector); it is not a reference.
//! - None: does nothing.

use super::generated::{EventActionDefinition, EventActionRunningMode, EventActionType, Scope};
use super::{ProjectValidator, ValidationError, ValidationLayer};
use crate::common::errors::codes::ERR_VALIDATION_REFERENCE;
use crate::forms::pick::{list_command, target_label};
use crate::forms::references::{PLAYBACK_TARGETS, RefRule, RefTarget, rule_for};
use crate::forms::validate_refs::validate_reference;

/// Names of the action types, in schema order.
pub const EVENT_ACTION_TYPE_NAMES: &[&str] = &[
    "None",
    "Play",
    "Pause",
    "Resume",
    "Stop",
    "Seek",
    "MuteBus",
    "UnmuteBus",
    "Wait",
];

/// Names of the run modes.
pub const EVENT_RUN_MODE_NAMES: &[&str] = &["Parallel", "Sequential"];

impl std::fmt::Display for EventActionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::fmt::Display for EventActionRunningMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl EventActionType {
    /// Parses a schema name (`MuteBus`), ignoring case, `-` and `_` (`mute-bus`).
    pub fn from_name(name: &str) -> Result<Self, String> {
        let wanted = name.replace(['-', '_'], "").to_ascii_lowercase();
        let all = [
            Self::None,
            Self::Play,
            Self::Pause,
            Self::Resume,
            Self::Stop,
            Self::Seek,
            Self::MuteBus,
            Self::UnmuteBus,
            Self::Wait,
        ];
        all.into_iter()
            .find(|t| t.to_string().to_ascii_lowercase() == wanted)
            .ok_or_else(|| {
                format!(
                    "Unknown action type '{name}'. Valid values: {}",
                    EVENT_ACTION_TYPE_NAMES.join(", ")
                )
            })
    }

    /// What this type's targets point at; `None` when they are no references (Wait, None).
    pub fn target_kinds(self) -> Option<RefTarget> {
        match self {
            Self::Play | Self::Pause | Self::Resume | Self::Stop | Self::Seek => {
                Some(PLAYBACK_TARGETS)
            }
            Self::MuteBus | Self::UnmuteBus => Some(RefTarget::Bus),
            Self::Wait | Self::None => None,
        }
    }
}

impl EventActionRunningMode {
    /// Parses a run mode name, ignoring case.
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name.to_ascii_lowercase().as_str() {
            "parallel" => Ok(Self::Parallel),
            "sequential" => Ok(Self::Sequential),
            _ => Err(format!(
                "Unknown run mode '{name}'. Valid values: {}",
                EVENT_RUN_MODE_NAMES.join(", ")
            )),
        }
    }
}

/// An active, entity-scoped action (the schema defaults) of `type_` on `targets`.
pub fn new_action(type_: EventActionType, targets: Vec<u64>) -> EventActionDefinition {
    EventActionDefinition {
        type_,
        active: true,
        scope: Scope::Entity,
        targets: Some(targets),
    }
}

fn targets_of(action: &EventActionDefinition) -> &[u64] {
    action.targets.as_deref().unwrap_or(&[])
}

/// Checks the shape of an action: types that act on something need targets
/// (Wait needs its duration; the SDK reads `targets[0]` unchecked).
pub fn validate_action_shape(
    action: &EventActionDefinition,
    index: usize,
) -> Result<(), ValidationError> {
    if action.type_ == EventActionType::None || !targets_of(action).is_empty() {
        return Ok(());
    }
    let (why, suggestion) = if action.type_ == EventActionType::Wait {
        (
            "Wait actions need their duration in milliseconds as the single target".to_string(),
            "Write the duration as the target, for example 'wait:500'".to_string(),
        )
    } else {
        (
            format!(
                "{:?} actions must have at least one target ({})",
                action.type_,
                target_label(action.type_.target_kinds().unwrap_or(PLAYBACK_TARGETS))
            ),
            "Add target names or IDs to the action".to_string(),
        )
    };
    Err(ValidationError::type_rule_violation(
        format!("Action {} has no targets", index + 1),
        why,
    )
    .with_suggestion(suggestion)
    .with_field(format!("actions[{index}].targets")))
}

fn wrong_kind_error(
    action: &EventActionDefinition,
    index: usize,
    target_index: usize,
    id: u64,
    actual: &str,
) -> ValidationError {
    let wanted = action.type_.target_kinds().unwrap_or(PLAYBACK_TARGETS);
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_REFERENCE,
        format!(
            "Action {} target {} (ID {id}) is a {actual}",
            index + 1,
            target_index + 1
        ),
        format!(
            "{:?} actions target a {}, not a {actual}",
            action.type_,
            target_label(wanted)
        ),
    )
    .with_suggestion(format!(
        "Choose a {} (see '{}')",
        target_label(wanted),
        list_command(wanted)
    ))
    .with_field(format!("actions[{index}].targets[{target_index}]"))
}

/// Checks every target of `action` against the kinds its type targets: zero and
/// dangling ids are reference errors, and so is an object of the wrong kind.
/// Wait and None actions have no reference targets.
pub fn validate_action_targets(
    action: &EventActionDefinition,
    index: usize,
    validator: Option<&ProjectValidator>,
) -> Result<(), ValidationError> {
    let empty = ProjectValidator::empty();
    let (Some(kinds), Some(rule)) = (
        action.type_.target_kinds(),
        rule_for(super::AssetType::Event, "actions.targets"),
    ) else {
        return Ok(());
    };
    let narrowed = RefRule {
        target: kinds,
        ..rule
    };
    for (target_index, &id) in targets_of(action).iter().enumerate() {
        let field = format!("actions[{index}].targets[{target_index}]");
        // Without a validator there is nothing to resolve ids with, but 0 is never valid.
        let Some(validator) = validator.or_else(|| (id == 0).then_some(&empty)) else {
            continue;
        };
        let valid = validator.resolve(kinds, id).is_some()
            || (kinds.includes(RefTarget::Bus) && validator.bus_outside_active(id));
        if id != 0
            && !valid
            && let Some(actual) = rule
                .target
                .members()
                .into_iter()
                .find(|k| validator.resolve(*k, id).is_some())
        {
            // The object exists but is of a kind this action type cannot target.
            let actual = target_label(actual);
            return Err(wrong_kind_error(action, index, target_index, id, &actual));
        }
        validate_reference(&narrowed, id, validator).map_err(|e| e.with_field(field))?;
    }
    Ok(())
}
