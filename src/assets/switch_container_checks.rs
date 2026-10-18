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

//! The project-aware checks of `SwitchContainer::validate_rules`.
//!
//! Each rule mirrors what `SwitchContainerImpl::LoadDefinition` and
//! `ChannelInternalState::PlaySwitchContainer` of the SDK do with the definition:
//! - the bus and the switch group must be set and known (load fails otherwise);
//! - effect and attenuation are only looked up when non-zero;
//! - an entry's object must be a known sound or collection (load fails otherwise);
//! - `fade_in`/`fade_out` and their `fader` are dereferenced unconditionally and the
//!   fader name is looked up in the registry, so a missing one crashes the load;
//! - the container `fader` is dereferenced when playback starts;
//! - the default switch state is looked up with `.at()` when the switch has no current
//!   state, so a 0 or foreign state throws at play time. Entry states outside the switch
//!   can never be played, which is why they are rejected too.

use super::generated::{FadeTransitionSettings, SwitchContainerEntry};
use super::{
    FaderAlgorithm, ProjectValidator, RtpcCompatibleValue, SwitchContainer, ValidationError,
    ValidationLayer,
};
use crate::common::errors::codes::{ERR_VALIDATION_FIELD, ERR_VALIDATION_REFERENCE};
use crate::forms::references::{RefRule, rule_for};
use crate::forms::validate_refs::validate_reference;

use super::AssetType;

const TYPE: AssetType = AssetType::SwitchContainer;

fn rule(field: &str) -> RefRule {
    rule_for(TYPE, field).expect("the reference table must describe every switch container rule")
}

fn field_error(field: String, what: String, why: &str, fix: String) -> ValidationError {
    ValidationError::new(ValidationLayer::TypeRules, ERR_VALIDATION_FIELD, what, why)
        .with_suggestion(fix)
        .with_field(field)
}

fn check_value(field: String, value: Option<&RtpcCompatibleValue>) -> Result<(), ValidationError> {
    match value.and_then(|v| v.as_static()) {
        Some(v) if !v.is_finite() => Err(field_error(
            field.clone(),
            format!("Invalid value for '{field}': {v}"),
            "Values must be finite numbers; NaN and infinity are not allowed",
            "Use an ordinary number such as 0.8".to_string(),
        )),
        _ => Ok(()),
    }
}

fn check_fader_name(field: String, fader: Option<&str>) -> Result<(), ValidationError> {
    let names = super::extensions::FADER_ALGORITHM_NAMES.join(", ");
    match fader {
        Some(name) if FaderAlgorithm::from_str(name).is_ok() => Ok(()),
        Some(name) => Err(field_error(
            field,
            format!("Unknown fader algorithm: '{name}'"),
            "The SDK cannot construct a fader with that name",
            format!("Use one of: {names}"),
        )),
        None => Err(field_error(
            field.clone(),
            format!("'{field}' is not set"),
            "The SDK reads the fader name without checking that it exists",
            format!("Set '{field}' to one of: {names}"),
        )),
    }
}

fn check_fade(
    index: usize,
    key: &str,
    fade: Option<&FadeTransitionSettings>,
) -> Result<(), ValidationError> {
    let field = format!("entries[{index}].{key}");
    let Some(fade) = fade else {
        return Err(field_error(
            field.clone(),
            format!("Entry {index} has no {key}"),
            "The SDK reads the fade settings of every entry without checking that they exist",
            format!(
                "Give the entry a '{key}' with a duration and a fader (0 and Linear mean no fade)"
            ),
        ));
    };
    if !fade.duration.is_finite() {
        return Err(field_error(
            format!("{field}.duration"),
            format!(
                "Entry {index} has an invalid {key} duration: {}",
                fade.duration
            ),
            "A fade duration must be a finite number of seconds",
            "Use a number such as 0.5".to_string(),
        ));
    }
    check_fader_name(format!("{field}.fader"), fade.fader.as_deref())
}

fn state_error(
    validator: &ProjectValidator,
    group: u64,
    field: String,
    id: u64,
) -> ValidationError {
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_REFERENCE,
        format!("Switch state with ID {id} is not a state of the container's switch"),
        format!("'{field}' must be one of the states of the switch the container uses (switch_group {group})"),
    )
    .with_suggestion(validator.describe_states(group))
    .with_field(field)
}

/// Checks `field` holds a state of the switch `group`.
fn check_state(
    validator: &ProjectValidator,
    group: u64,
    rule: &RefRule,
    id: u64,
    field: String,
) -> Result<(), ValidationError> {
    validate_reference(rule, id, validator).map_err(|e| e.with_field(field.clone()))?;
    let in_group = validator
        .switch_states(group)
        .is_some_and(|states| states.iter().any(|s| s.id == id));
    if in_group {
        Ok(())
    } else {
        Err(state_error(validator, group, field, id))
    }
}

fn check_entry(
    validator: &ProjectValidator,
    group: u64,
    index: usize,
    entry: &SwitchContainerEntry,
) -> Result<(), ValidationError> {
    validate_reference(&rule("entries.object"), entry.object, validator)
        .map_err(|e| e.with_field(format!("entries[{index}].object")))?;
    let state_rule = rule("entries.switch_states");
    for (j, state) in entry.switch_states.iter().enumerate() {
        check_state(
            validator,
            group,
            &state_rule,
            *state,
            format!("entries[{index}].switch_states[{j}]"),
        )?;
    }
    check_fade(index, "fade_in", entry.fade_in.as_ref())?;
    check_fade(index, "fade_out", entry.fade_out.as_ref())?;
    check_value(format!("entries[{index}].gain"), entry.gain.as_ref())?;
    check_value(format!("entries[{index}].pitch"), entry.pitch.as_ref())
}

/// Runs the cross-asset and SDK-load checks of `container` against the project.
pub(super) fn check_container(
    container: &SwitchContainer,
    validator: &ProjectValidator,
) -> Result<(), ValidationError> {
    validate_reference(&rule("bus"), container.bus, validator)?;
    validate_reference(&rule("switch_group"), container.switch_group, validator)?;
    validate_reference(&rule("effect"), container.effect, validator)?;
    validate_reference(&rule("attenuation"), container.attenuation, validator)?;

    if let Some(gain) = container.gain.as_ref().and_then(|g| g.as_static())
        && (!gain.is_finite() || !(0.0..=1.0).contains(&gain))
    {
        return Err(field_error(
            "gain".to_string(),
            format!("Invalid gain value: {gain}"),
            "Gain must be between 0.0 and 1.0",
            "Set gain to a value between 0.0 (silent) and 1.0 (full volume)".to_string(),
        ));
    }
    check_value("pitch".to_string(), container.pitch.as_ref())?;
    check_value("priority".to_string(), container.priority.as_ref())?;
    check_fader_name("fader".to_string(), container.fader.as_deref())?;

    let group = container.switch_group;
    check_state(
        validator,
        group,
        &rule("default_switch_state"),
        container.default_switch_state,
        "default_switch_state".to_string(),
    )?;
    for (index, entry) in container.entries.iter().flatten().enumerate() {
        check_entry(validator, group, index, entry)?;
    }
    Ok(())
}
