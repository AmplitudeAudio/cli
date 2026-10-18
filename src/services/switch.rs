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

//! Switch creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI all produce a [`SwitchValues`] (create) or a
//! [`SwitchPatch`] (update); this module turns them into a validated [`Switch`] without
//! touching the terminal or the file system. Functions return new values and never mutate
//! their inputs.
//!
//! State ids are what switch containers store, so an update keeps the id of every state
//! that keeps its name (or is renamed) and only generates ids for new states.

use crate::assets::generated::SwitchStateDefinition;
use crate::assets::{Asset, ProjectContext, Switch, ValidationError, ValidationLayer};
use crate::common::errors::codes::ERR_VALIDATION_FIELD;
use crate::common::utils::generate_unique_id;
use crate::services::sound::unique_id;

/// Attempts at a fresh state id before falling back to counting up.
const ID_RETRIES: u32 = 8;

/// A state as a caller describes it: a name, and an id when the caller picks one.
#[derive(Debug, Clone, PartialEq)]
pub struct StateSpec {
    pub name: String,
    pub id: Option<u64>,
}

/// Every value a new switch needs.
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchValues {
    pub name: String,
    pub states: Vec<SwitchStateDefinition>,
}

impl SwitchValues {
    /// Turns a patch into the values of a new switch. A switch without states is an
    /// error naming the flags that set them.
    pub fn from_patch(name: &str, patch: &SwitchPatch) -> Result<Self, ValidationError> {
        let states = apply_states(name, &[], &patch.states)?;
        if states.is_empty() {
            return Err(ValidationError::new(
                ValidationLayer::TypeRules,
                ERR_VALIDATION_FIELD,
                "Missing required --states",
                "A switch needs at least one state and cannot prompt in non-interactive or JSON mode",
            )
            .with_suggestion(
                "Pass --states wood,stone, --state NAME[,id=N] (repeatable) or --states-from-json <file>",
            )
            .with_field("states"));
        }
        Ok(Self {
            name: name.to_string(),
            states,
        })
    }
}

/// Builds a validated switch from `values`, generating a unique id.
pub fn build_switch(values: SwitchValues, ctx: &ProjectContext) -> Result<Switch, ValidationError> {
    let id = unique_id("switch", &values.name, ctx)?;
    let switch = Switch::builder(id, &values.name)
        .states(values.states)
        .build();
    switch.validate_rules(ctx)?;
    Ok(switch)
}

/// Changes to the state list, applied in order: replace, rename, remove, add.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatesPatch {
    /// Replaces the whole list (an empty list clears it). A state without an explicit id
    /// keeps the id of the existing state of the same name.
    pub replace: Option<Vec<StateSpec>>,
    /// `(state, new name)` pairs; the state is a name or an id, and keeps its id.
    pub rename: Vec<(String, String)>,
    /// States removed, each a name or an id.
    pub remove: Vec<String>,
    /// States appended at the end.
    pub add: Vec<StateSpec>,
}

impl StatesPatch {
    /// Whether the patch leaves the list alone.
    pub fn is_empty(&self) -> bool {
        self.replace.is_none()
            && self.rename.is_empty()
            && self.remove.is_empty()
            && self.add.is_empty()
    }
}

/// The fields of an existing switch a caller wants to change; unset parts are kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SwitchPatch {
    pub states: StatesPatch,
}

impl SwitchPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [("states", !self.states.is_empty())]
            .into_iter()
            .filter_map(|(name, set)| set.then_some(name))
            .collect()
    }
}

/// A fresh id for state `state_name` of switch `switch_name`, never 0 and never in `taken`.
pub fn default_state_id(switch_name: &str, state_name: &str, taken: &[u64]) -> u64 {
    let usable = |id: u64| id != 0 && !taken.contains(&id);
    let mut id = generate_unique_id(&format!("{switch_name}_{state_name}"));
    for attempt in 0..ID_RETRIES {
        if usable(id) {
            return id;
        }
        id = generate_unique_id(&format!("{switch_name}_{state_name}_{attempt}"));
    }
    while !usable(id) {
        id = id.wrapping_add(1);
    }
    id
}

fn describe_states(states: &[SwitchStateDefinition]) -> String {
    if states.is_empty() {
        return "the switch has no states".to_string();
    }
    let list = states
        .iter()
        .map(|s| format!("{} (ID {})", s.name.as_deref().unwrap_or("(unnamed)"), s.id))
        .collect::<Vec<_>>()
        .join(", ");
    format!("available: {list}")
}

fn unknown_state(flag: &str, key: &str, states: &[SwitchStateDefinition]) -> ValidationError {
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        format!("The switch has no state '{key}'"),
        format!("{flag} names a state that does not exist"),
    )
    .with_suggestion(format!(
        "Use a state name or ID ({})",
        describe_states(states)
    ))
    .with_field("states")
}

/// The position of the state a name or an id designates (the name wins).
fn find_state(states: &[SwitchStateDefinition], key: &str) -> Option<usize> {
    let key = key.trim();
    states
        .iter()
        .position(|s| s.name.as_deref() == Some(key))
        .or_else(|| {
            let id = key.parse::<u64>().ok()?;
            states.iter().position(|s| s.id == id)
        })
}

fn ids_of(states: &[SwitchStateDefinition]) -> Vec<u64> {
    states.iter().map(|s| s.id).collect()
}

fn replaced(
    switch_name: &str,
    existing: &[SwitchStateDefinition],
    specs: &[StateSpec],
) -> Vec<SwitchStateDefinition> {
    let mut taken: Vec<u64> = specs.iter().filter_map(|s| s.id).collect();
    taken.extend(ids_of(existing));
    let mut out: Vec<SwitchStateDefinition> = Vec::with_capacity(specs.len());
    for spec in specs {
        let kept = existing
            .iter()
            .find(|s| s.name.as_deref() == Some(spec.name.as_str()))
            .map(|s| s.id);
        let id = spec
            .id
            .or(kept)
            .unwrap_or_else(|| default_state_id(switch_name, &spec.name, &taken));
        taken.push(id);
        out.push(SwitchStateDefinition {
            id,
            name: Some(spec.name.clone()),
        });
    }
    out
}

/// The state list after `patch`; `existing` stays untouched.
pub fn apply_states(
    switch_name: &str,
    existing: &[SwitchStateDefinition],
    patch: &StatesPatch,
) -> Result<Vec<SwitchStateDefinition>, ValidationError> {
    let mut states = match &patch.replace {
        Some(specs) => replaced(switch_name, existing, specs),
        None => existing.to_vec(),
    };
    for (key, new_name) in &patch.rename {
        let index = find_state(&states, key)
            .ok_or_else(|| unknown_state("--rename-state", key, &states))?;
        states[index].name = Some(new_name.trim().to_string());
    }
    for key in &patch.remove {
        let index = find_state(&states, key)
            .ok_or_else(|| unknown_state("--remove-state", key, &states))?;
        states.remove(index);
    }
    for spec in &patch.add {
        let mut taken = ids_of(&states);
        taken.extend(patch.add.iter().filter_map(|s| s.id));
        let id = spec
            .id
            .unwrap_or_else(|| default_state_id(switch_name, &spec.name, &taken));
        states.push(SwitchStateDefinition {
            id,
            name: Some(spec.name.clone()),
        });
    }
    Ok(states)
}

/// Merges `patch` into a copy of `existing`; fields the patch leaves unset are preserved.
///
/// The result is not validated: callers run `validate_rules` on it.
pub fn apply_patch(existing: &Switch, patch: &SwitchPatch) -> Result<Switch, ValidationError> {
    let mut switch = existing.clone();
    if !patch.states.is_empty() {
        let current = existing.states.as_deref().unwrap_or(&[]);
        switch.states = Some(apply_states(existing.name(), current, &patch.states)?);
    }
    Ok(switch)
}

/// The ids of the states of `before` that `after` no longer has: removed, or given another id.
pub fn removed_state_ids(
    before: &[SwitchStateDefinition],
    after: &[SwitchStateDefinition],
) -> Vec<u64> {
    let kept = ids_of(after);
    before
        .iter()
        .map(|s| s.id)
        .filter(|id| *id != 0 && !kept.contains(id))
        .collect()
}
