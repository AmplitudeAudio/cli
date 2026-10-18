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

//! Event creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI all produce an [`EventValues`] (create)
//! or an [`EventPatch`] (update); this module turns them into a validated
//! [`Event`] without touching the terminal or the file system. Functions
//! return new values and never mutate their inputs.

use crate::assets::generated::{EventActionDefinition, EventActionRunningMode};
use crate::assets::{Asset, Event, ProjectContext, ValidationError, ValidationLayer};
use crate::common::errors::codes::ERR_VALIDATION_FIELD;
use crate::services::sound::unique_id;

/// Every value a new event needs.
#[derive(Debug, Clone, PartialEq)]
pub struct EventValues {
    pub name: String,
    pub run_mode: EventActionRunningMode,
    pub actions: Vec<EventActionDefinition>,
}

impl EventValues {
    /// Fills a patch's gaps with defaults (Parallel run mode). Actions have no
    /// default: an event without any is rejected by [`build_event`].
    pub fn from_patch(name: &str, patch: &EventPatch) -> Result<Self, ValidationError> {
        let actions = patch
            .actions
            .replace
            .iter()
            .flatten()
            .chain(&patch.actions.add)
            .cloned()
            .collect();
        Ok(Self {
            name: name.to_string(),
            run_mode: patch.run_mode.unwrap_or(EventActionRunningMode::Parallel),
            actions,
        })
    }
}

/// Builds a validated event from `values`, generating a unique id.
pub fn build_event(values: EventValues, ctx: &ProjectContext) -> Result<Event, ValidationError> {
    let id = unique_id("event", &values.name, ctx)?;
    let mut event = Event::builder(id, &values.name)
        .run_mode(values.run_mode)
        .build();
    event.actions = Some(values.actions);
    event.validate_rules(ctx)?;
    Ok(event)
}

/// Changes to the action list, applied in order: replace, remove, add.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActionsPatch {
    /// Replaces the whole list (an empty list clears it).
    pub replace: Option<Vec<EventActionDefinition>>,
    /// 0-based indexes, into the list after `replace`, of the actions removed.
    pub remove: Vec<usize>,
    /// Actions appended at the end.
    pub add: Vec<EventActionDefinition>,
}

impl ActionsPatch {
    /// Whether the patch leaves the list alone.
    pub fn is_empty(&self) -> bool {
        self.replace.is_none() && self.remove.is_empty() && self.add.is_empty()
    }
}

/// The fields of an existing event a caller wants to change; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventPatch {
    pub run_mode: Option<EventActionRunningMode>,
    pub actions: ActionsPatch,
}

impl EventPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("run_mode", self.run_mode.is_some()),
            ("actions", !self.actions.is_empty()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

fn remove_out_of_range(index: usize, len: usize) -> ValidationError {
    let valid = if len == 0 {
        "the event has no actions".to_string()
    } else {
        format!("valid indexes are 0 to {}", len - 1)
    };
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        format!("The event has no action at index {index}"),
        "--remove-action names an action position that does not exist",
    )
    .with_suggestion(format!(
        "Indexes start at 0 ({valid}); check them with the event file"
    ))
    .with_field("actions")
}

/// The action list after `patch`; `existing` stays untouched.
pub fn apply_actions(
    existing: &[EventActionDefinition],
    patch: &ActionsPatch,
) -> Result<Vec<EventActionDefinition>, ValidationError> {
    let mut actions = patch
        .replace
        .clone()
        .unwrap_or_else(|| existing.to_vec());
    if let Some(&bad) = patch.remove.iter().find(|&&i| i >= actions.len()) {
        return Err(remove_out_of_range(bad, actions.len()));
    }
    let kept = actions
        .drain(..)
        .enumerate()
        .filter(|(i, _)| !patch.remove.contains(i))
        .map(|(_, action)| action);
    Ok(kept.chain(patch.add.iter().cloned()).collect())
}

/// Merges `patch` into a copy of `existing`; fields the patch leaves unset are preserved.
///
/// The result is not validated: callers run `validate_rules` on it, which also
/// rejects an event left without actions.
pub fn apply_patch(existing: &Event, patch: &EventPatch) -> Result<Event, ValidationError> {
    let mut event = existing.clone();
    if let Some(run_mode) = patch.run_mode {
        event.run_mode = run_mode;
    }
    if !patch.actions.is_empty() {
        let current = existing.actions.as_deref().unwrap_or(&[]);
        event.actions = Some(apply_actions(current, &patch.actions)?);
    }
    Ok(event)
}
