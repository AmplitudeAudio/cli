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

//! Strict validation of asset reference fields.
//!
//! An ID of 0 is never a valid reference to an existing object: for a required
//! rule it is an error, for an optional rule it means "unset". Any non-zero ID
//! must resolve to an existing object of the rule's target.

use crate::assets::validator::ProjectValidator;
use crate::assets::{ValidationError, ValidationLayer};
use crate::common::errors::codes::{ERR_VALIDATION_EMPTY_REFERENCE, ERR_VALIDATION_REFERENCE};
use crate::forms::pick::{create_command, target_label};
use crate::forms::references::{RefRule, RefTarget};

/// Warning for a bus that exists in the project but not in the bus file the engine
/// configs load, or `None` when `bus_id` is fine (or no bus file is active).
pub fn inactive_bus_warning(validator: &ProjectValidator, bus_id: u64) -> Option<String> {
    if !validator.bus_outside_active(bus_id) {
        return None;
    }
    let bus = validator.find_bus_anywhere(None, Some(bus_id))?;
    let active = validator.active_bus_file()?;
    Some(format!(
        "Bus '{}' (ID {bus_id}) is not in '{active}.buses.json', the bus file your engine config loads; the engine will not find it at runtime",
        bus.name
    ))
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Checks that `id` is a valid value for the reference described by `rule`.
pub fn validate_reference(
    rule: &RefRule,
    id: u64,
    validator: &ProjectValidator,
) -> Result<(), ValidationError> {
    let label = target_label(rule.target);
    let list_command = format!("am asset {label} list");

    if id == 0 {
        if !rule.required {
            return Ok(());
        }
        return Err(ValidationError::new(
            ValidationLayer::TypeRules,
            ERR_VALIDATION_EMPTY_REFERENCE,
            format!("Field '{}' has no {label} reference", rule.field),
            format!("'{}' is required and ID 0 is not a valid {label}", rule.field),
        )
        .with_suggestion(format!(
            "Set '{}' to the ID of an existing {label} (see '{list_command}'), or create one with '{}'",
            rule.field,
            create_command(rule.target)
        ))
        .with_field(rule.field));
    }

    if validator.resolve(rule.target, id).is_some() {
        return Ok(());
    }
    // A bus that exists only outside the active bus file is a warning for the
    // caller (see `inactive_bus_warning`), not a failure: failing would break
    // flows that predate the engine config and cannot know which file is active.
    if rule.target == RefTarget::Bus && validator.bus_outside_active(id) {
        return Ok(());
    }

    Err(ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_REFERENCE,
        format!("{} with ID {id} not found", capitalize(&label)),
        format!(
            "Field '{}' references {label} ID {id}, which does not exist in the project",
            rule.field
        ),
    )
    .with_suggestion(format!(
        "Use the ID of an existing {label} (see '{list_command}'), or create one with '{}'",
        create_command(rule.target)
    ))
    .with_field(rule.field))
}
