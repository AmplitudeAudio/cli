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

//! Effect creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI all produce an [`EffectValues`] (create)
//! or an [`EffectPatch`] (update); this module turns them into a validated
//! [`Effect`] without touching the terminal or the file system. Functions
//! return new values and never mutate their inputs.

use crate::assets::extensions::EFFECT_FILTER_NAMES;
use crate::assets::{
    Asset, Effect, ProjectContext, RtpcCompatibleValue, ValidationError, ValidationLayer,
};
use crate::common::errors::codes::ERR_VALIDATION_FIELD;
use crate::services::sound::unique_id;

/// Every value a new effect needs.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectValues {
    pub name: String,
    /// The registered filter the effect applies.
    pub effect: String,
    pub parameters: Vec<RtpcCompatibleValue>,
}

fn missing_effect() -> ValidationError {
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        "Missing required --effect-type",
        "An effect must name the filter it applies; the SDK cannot load one without it",
    )
    .with_suggestion(format!(
        "Pass --effect-type with a registered filter, for example one of: {}",
        EFFECT_FILTER_NAMES.join(", ")
    ))
    .with_field("effect")
}

impl EffectValues {
    /// Turns a patch into the values of a new effect. The filter has no default:
    /// a patch without one is an error naming `--effect-type`.
    pub fn from_patch(name: &str, patch: &EffectPatch) -> Result<Self, ValidationError> {
        let effect = patch.effect.clone().ok_or_else(missing_effect)?;
        let parameters = apply_params(&[], &patch.parameters)?;
        Ok(Self {
            name: name.to_string(),
            effect,
            parameters,
        })
    }
}

/// Builds a validated effect from `values`, generating a unique id.
pub fn build_effect(values: EffectValues, ctx: &ProjectContext) -> Result<Effect, ValidationError> {
    let id = unique_id("effect", &values.name, ctx)?;
    let effect = Effect::builder(id, &values.name)
        .effect_type(values.effect)
        .parameters(values.parameters)
        .build();
    effect.validate_rules(ctx)?;
    Ok(effect)
}

/// Changes to the parameter list, applied in order: replace, bind, remove, add.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParamsPatch {
    /// Replaces the whole list (an empty list clears it).
    pub replace: Option<Vec<RtpcCompatibleValue>>,
    /// Sets the parameter at a 0-based index (in the list after `replace`); an index equal
    /// to the list length appends.
    pub bind: Vec<(usize, RtpcCompatibleValue)>,
    /// 0-based indexes, into the list after `replace` and `bind`, of the parameters removed.
    pub remove: Vec<usize>,
    /// Parameters appended at the end.
    pub add: Vec<RtpcCompatibleValue>,
}

impl ParamsPatch {
    /// Whether the patch leaves the list alone.
    pub fn is_empty(&self) -> bool {
        self.replace.is_none()
            && self.bind.is_empty()
            && self.remove.is_empty()
            && self.add.is_empty()
    }

    /// The values the patch brings in, for checking before anything else is asked.
    pub fn incoming(&self) -> impl Iterator<Item = &RtpcCompatibleValue> {
        self.replace
            .iter()
            .flatten()
            .chain(self.bind.iter().map(|(_, v)| v))
            .chain(&self.add)
    }
}

/// The fields of an existing effect a caller wants to change; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectPatch {
    pub effect: Option<String>,
    pub parameters: ParamsPatch,
}

impl EffectPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("effect_type", self.effect.is_some()),
            ("parameters", !self.parameters.is_empty()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

fn index_error(flag: &str, index: usize, len: usize, appendable: bool) -> ValidationError {
    let valid = match (len, appendable) {
        (0, false) => "the effect has no parameters".to_string(),
        (0, true) => "the list is empty, so only index 0 can be set".to_string(),
        (_, false) => format!("valid indexes are 0 to {}", len - 1),
        (_, true) => format!("valid indexes are 0 to {len}"),
    };
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        format!("The effect has no parameter at index {index}"),
        format!("{flag} names a parameter position that does not exist"),
    )
    .with_suggestion(format!("Indexes start at 0 ({valid})"))
    .with_field("parameters")
}

/// The parameter list after `patch`; `existing` stays untouched.
pub fn apply_params(
    existing: &[RtpcCompatibleValue],
    patch: &ParamsPatch,
) -> Result<Vec<RtpcCompatibleValue>, ValidationError> {
    let mut params = patch.replace.clone().unwrap_or_else(|| existing.to_vec());
    for (index, value) in &patch.bind {
        match *index {
            i if i < params.len() => params[i] = value.clone(),
            i if i == params.len() => params.push(value.clone()),
            i => return Err(index_error("--param-rtpc", i, params.len(), true)),
        }
    }
    if let Some(&bad) = patch.remove.iter().find(|&&i| i >= params.len()) {
        return Err(index_error("--remove-param", bad, params.len(), false));
    }
    let kept = params
        .drain(..)
        .enumerate()
        .filter(|(i, _)| !patch.remove.contains(i))
        .map(|(_, value)| value);
    Ok(kept.chain(patch.add.iter().cloned()).collect())
}

/// Merges `patch` into a copy of `existing`; fields the patch leaves unset are preserved.
///
/// The result is not validated: callers run `validate_rules` on it.
pub fn apply_patch(existing: &Effect, patch: &EffectPatch) -> Result<Effect, ValidationError> {
    let mut effect = existing.clone();
    if let Some(name) = &patch.effect {
        effect.effect = Some(name.clone());
    }
    if !patch.parameters.is_empty() {
        let current = existing.parameters.as_deref().unwrap_or(&[]);
        effect.parameters = Some(apply_params(current, &patch.parameters)?);
    }
    Ok(effect)
}

/// A warning when `name` is not one of the filters the SDK registers by itself.
///
/// Plugins register more filters, so an unknown name is not an error; the warning
/// lists the built-in ones and, for a name differing only by case, the likely spelling
/// (the SDK compares names exactly).
pub fn filter_warning(name: &str) -> Option<String> {
    if EFFECT_FILTER_NAMES.contains(&name) {
        return None;
    }
    let hint = EFFECT_FILTER_NAMES
        .iter()
        .find(|known| known.eq_ignore_ascii_case(name.trim()))
        .map(|known| format!(" Did you mean '{known}'? The SDK compares filter names exactly."))
        .unwrap_or_default();
    Some(format!(
        "'{name}' is not a filter the SDK registers by default (built-in: {}); the effect only loads if a plugin registers a filter with that exact name.{hint}",
        EFFECT_FILTER_NAMES.join(", ")
    ))
}
