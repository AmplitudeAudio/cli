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

//! Switch container creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI all produce a [`SwitchContainerValues`]
//! (create) or a [`SwitchContainerPatch`] (update); this module turns them into a
//! validated [`SwitchContainer`] without touching the terminal or the file system.
//! Functions return new values and never mutate their inputs.

use crate::assets::generated::{Scope, Spatialization, SwitchContainerUpdateBehavior};
use crate::assets::{
    Asset, AssetType, FaderAlgorithm, ProjectContext, ProjectValidator, RtpcCompatibleValue,
    SwitchContainer, ValidationError, ValidationLayer,
};
use crate::common::errors::codes::ERR_VALIDATION_FIELD;
use crate::forms::references::rule_for;
use crate::forms::validate_refs::validate_reference;
use crate::services::sound::{DEFAULT_GAIN, DEFAULT_PRIORITY, require_bus, unique_id};
use crate::services::switch_container_entries::{EntriesPatch, EntrySpec, apply_entries};

/// Every value a new switch container needs. Reference fields use `0` for "unset".
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchContainerValues {
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
    pub update_behavior: SwitchContainerUpdateBehavior,
    pub switch_group: u64,
    pub default_switch_state: u64,
    pub entries: Vec<EntrySpec>,
}

impl SwitchContainerValues {
    /// Fills a patch's gaps with defaults.
    ///
    /// The bus and the switch group have none: a missing one becomes the invalid id 0,
    /// which [`build_switch_container`] rejects. The default switch state falls back to
    /// the first state of the switch group, so a container plays something before the
    /// game sets a state.
    pub fn from_patch(
        name: &str,
        patch: &SwitchContainerPatch,
        ctx: &ProjectContext,
    ) -> Result<Self, ValidationError> {
        let switch_group = patch.switch_group.unwrap_or(0);
        let default_switch_state = match patch.default_switch_state {
            Some(state) => state,
            None => first_state(switch_group, ctx)?,
        };
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
            update_behavior: patch.update_behavior.unwrap_or_default(),
            switch_group,
            default_switch_state,
            entries: patch.entries.incoming(),
        })
    }
}

/// The id of the first state of the switch `group`; 0 when there is no group yet.
fn first_state(group: u64, ctx: &ProjectContext) -> Result<u64, ValidationError> {
    let Some(validator) = ctx.validator.as_ref().filter(|_| group != 0) else {
        return Ok(0);
    };
    match validator.switch_states(group).and_then(|s| s.first()) {
        Some(state) => Ok(state.id),
        None if validator.switch_states(group).is_some() => Err(ValidationError::new(
            ValidationLayer::TypeRules,
            ERR_VALIDATION_FIELD,
            "The switch has no states to default to",
            "A switch container needs a default switch state and the switch it uses has no state",
        )
        .with_suggestion(validator.describe_states(group))
        .with_field("default_switch_state")),
        None => Ok(0),
    }
}

/// Rejects an unset (0) required reference even when the context has no validator.
fn require_set(field: &str, id: u64) -> Result<(), ValidationError> {
    if id != 0 {
        return Ok(());
    }
    let rule = rule_for(AssetType::SwitchContainer, field)
        .expect("the reference table must describe every switch container rule");
    validate_reference(&rule, 0, &ProjectValidator::empty())
}

/// Builds a validated switch container from `values`, generating a unique id.
pub fn build_switch_container(
    values: SwitchContainerValues,
    ctx: &ProjectContext,
) -> Result<SwitchContainer, ValidationError> {
    require_bus(AssetType::SwitchContainer, values.bus)?;
    require_set("switch_group", values.switch_group)?;
    require_set("default_switch_state", values.default_switch_state)?;
    let id = unique_id("switch container", &values.name, ctx)?;

    let mut container = SwitchContainer::builder(id, &values.name)
        .bus(values.bus)
        .effect(values.effect)
        .attenuation(values.attenuation)
        .scope(values.scope)
        .spatialization(values.spatialization)
        .update_behavior(values.update_behavior)
        .switch_group(values.switch_group)
        .default_state(values.default_switch_state)
        .build();
    container.fader = Some(values.fader);
    container.gain = Some(values.gain);
    container.priority = Some(values.priority);
    container.pitch = values.pitch;
    container.entries = Some(values.entries.iter().map(EntrySpec::to_entry).collect());

    container.validate_rules(ctx)?;
    Ok(container)
}

/// The fields of an existing switch container a caller wants to change; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SwitchContainerPatch {
    pub bus: Option<u64>,
    pub effect: Option<u64>,
    pub attenuation: Option<u64>,
    pub scope: Option<Scope>,
    pub fader: Option<String>,
    pub gain: Option<RtpcCompatibleValue>,
    pub pitch: Option<RtpcCompatibleValue>,
    pub priority: Option<RtpcCompatibleValue>,
    pub spatialization: Option<Spatialization>,
    pub update_behavior: Option<SwitchContainerUpdateBehavior>,
    pub switch_group: Option<u64>,
    pub default_switch_state: Option<u64>,
    pub entries: EntriesPatch,
}

impl SwitchContainerPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("switch_group", self.switch_group.is_some()),
            ("default_switch_state", self.default_switch_state.is_some()),
            ("update_behavior", self.update_behavior.is_some()),
            ("gain", self.gain.is_some()),
            ("bus", self.bus.is_some()),
            ("priority", self.priority.is_some()),
            ("spatialization", self.spatialization.is_some()),
            ("effect", self.effect.is_some()),
            ("attenuation", self.attenuation.is_some()),
            ("pitch", self.pitch.is_some()),
            ("scope", self.scope.is_some()),
            ("fader", self.fader.is_some()),
            ("entries", !self.entries.is_empty()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

/// Merges `patch` into a copy of `existing`; fields the patch leaves unset are preserved.
///
/// The result is not validated: callers run `validate_rules` on it.
pub fn apply_patch(
    existing: &SwitchContainer,
    patch: &SwitchContainerPatch,
) -> Result<SwitchContainer, ValidationError> {
    let mut container = existing.clone();

    if let Some(bus) = patch.bus {
        container.bus = bus;
    }
    if let Some(effect) = patch.effect {
        container.effect = effect;
    }
    if let Some(attenuation) = patch.attenuation {
        container.attenuation = attenuation;
    }
    if let Some(scope) = patch.scope {
        container.scope = scope;
    }
    if let Some(fader) = &patch.fader {
        container.fader = Some(fader.clone());
    }
    if let Some(gain) = &patch.gain {
        container.gain = Some(gain.clone());
    }
    if let Some(pitch) = &patch.pitch {
        container.pitch = Some(pitch.clone());
    }
    if let Some(priority) = &patch.priority {
        container.priority = Some(priority.clone());
    }
    if let Some(spatialization) = patch.spatialization {
        container.spatialization = spatialization;
    }
    if let Some(behavior) = patch.update_behavior {
        container.update_behavior = behavior;
    }
    if let Some(group) = patch.switch_group {
        container.switch_group = group;
    }
    if let Some(state) = patch.default_switch_state {
        container.default_switch_state = state;
    }
    if !patch.entries.is_empty() {
        let current = existing.entries.as_deref().unwrap_or(&[]);
        container.entries = Some(apply_entries(current, &patch.entries)?);
    }
    Ok(container)
}
