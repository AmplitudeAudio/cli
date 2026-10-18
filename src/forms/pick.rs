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

//! Interactive picker for asset and bus references.

use anyhow::Result;

use crate::assets::ProjectValidator;
use crate::common::errors::{CliError, codes::ERR_VALIDATION_EMPTY_REFERENCE};
use crate::forms::references::{RefRule, RefTarget};
use crate::input::{Input, select_index};

/// Label of the entry that clears an optional reference.
pub const NONE_LABEL: &str = "(none)";

/// Human-readable name of what a reference target holds ("bus", "attenuation", ...).
///
/// An any-of target reads "sound, collection or switch container".
pub fn target_label(target: RefTarget) -> String {
    match target {
        RefTarget::Bus => "bus".to_string(),
        RefTarget::Asset(asset_type) => asset_type.to_string().to_lowercase(),
        RefTarget::AnyOf(_) => {
            let labels: Vec<String> = target.members().into_iter().map(target_label).collect();
            match labels.split_last() {
                Some((last, rest)) if !rest.is_empty() => {
                    format!("{} or {last}", rest.join(", "))
                }
                _ => labels.join(""),
            }
        }
    }
}

fn single_command(target: RefTarget, verb: &str) -> String {
    match target {
        RefTarget::Bus => format!("am asset bus {verb}"),
        RefTarget::Asset(asset_type) => format!(
            "am asset {} {verb}",
            asset_type.to_string().to_lowercase().replace(' ', "-")
        ),
        RefTarget::AnyOf(_) => String::new(),
    }
}

fn commands(target: RefTarget, verb: &str) -> String {
    target
        .members()
        .into_iter()
        .map(|member| single_command(member, verb))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// The CLI command that creates an object of `target`.
pub fn create_command(target: RefTarget) -> String {
    commands(target, "create")
}

/// The CLI command that lists the objects of `target`.
pub fn list_command(target: RefTarget) -> String {
    commands(target, "list")
}

/// The label of the first kind among `kinds` that holds an object with `id`.
fn kind_of(validator: &ProjectValidator, kinds: &[RefTarget], id: u64) -> String {
    kinds
        .iter()
        .find(|kind| validator.resolve(**kind, id).is_some())
        .map_or_else(String::new, |kind| target_label(*kind))
}

/// Lets the user choose the object a reference field points at.
///
/// Returns `Ok(Some(id))` for a chosen object and `Ok(None)` when the optional
/// "(none)" entry is chosen. A required rule with nothing to choose from fails
/// with an error that tells the user how to create one.
pub fn pick_reference(
    input: &dyn Input,
    validator: &ProjectValidator,
    rule: &RefRule,
) -> Result<Option<u64>> {
    let candidates = validator.list(rule.target);

    if candidates.is_empty() && rule.required {
        let label = target_label(rule.target);
        let command = create_command(rule.target);
        return Err(CliError::new(
            ERR_VALIDATION_EMPTY_REFERENCE,
            format!("No {label} available for '{}'", rule.field),
            format!(
                "the project has no {label} and '{}' is required; create one with `{command}`",
                rule.field
            ),
        )
        .with_suggestion(format!("Create one first with `{command}`"))
        .into());
    }

    // With several kinds on offer a name alone is ambiguous, so each label names its kind.
    let kinds = rule.target.members();
    let mut labels: Vec<String> = if kinds.len() > 1 {
        candidates
            .iter()
            .map(|c| format!("{} ({})", c.name, kind_of(validator, &kinds, c.id)))
            .collect()
    } else {
        candidates.iter().map(|c| c.name.clone()).collect()
    };
    if !rule.required {
        labels.push(NONE_LABEL.to_string());
    }

    let prompt = format!("Select {} for '{}'", target_label(rule.target), rule.field);
    let index = select_index(input, &prompt, &labels)?;
    Ok(candidates.get(index).map(|c| c.id))
}
