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
pub fn target_label(target: RefTarget) -> String {
    match target {
        RefTarget::Bus => "bus".to_string(),
        RefTarget::Asset(asset_type) => asset_type.to_string().to_lowercase(),
    }
}

/// The CLI command that creates an object of `target`.
pub fn create_command(target: RefTarget) -> String {
    match target {
        RefTarget::Bus => "am asset bus create".to_string(),
        RefTarget::Asset(asset_type) => format!(
            "am asset {} create",
            asset_type.to_string().to_lowercase().replace(' ', "-")
        ),
    }
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

    let mut labels: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();
    if !rule.required {
        labels.push(NONE_LABEL.to_string());
    }

    let prompt = format!("Select {} for '{}'", target_label(rule.target), rule.field);
    let index = select_index(input, &prompt, &labels)?;
    Ok(candidates.get(index).map(|c| c.id))
}
