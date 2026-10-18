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

//! Interactive prompts for `effect create` and `effect update`.
//!
//! Every function here is only called when the session can prompt; flag values
//! already present in the patch are never asked for again.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::extensions::EFFECT_FILTER_NAMES;
use crate::assets::{AssetType, Effect, ProjectValidator, RtpcCompatibleValue};
use crate::forms::references::RefTarget;
use crate::forms::rtpc_value::prompt_rtpc_value;
use crate::input::{Input, select_index};
use crate::services::effect::{EffectPatch, ParamsPatch};

use super::sound_prompts::{ACCEPT_DEFAULTS_PROMPT, ask_choice_or_unset, to_labels};

const OTHER_LABEL: &str = "Other (plugin filter)";
const FILTER_PROMPT: &str = "Effect filter";
const PLUGIN_NAME_PROMPT: &str = "Plugin filter name";
const RTPC_TARGET: RefTarget = RefTarget::Asset(AssetType::Rtpc);

const MENU: [&str; 4] = [
    "Add a parameter",
    "Edit a parameter",
    "Remove a parameter",
    "Done",
];

fn filter_options() -> Vec<String> {
    let mut options = to_labels(EFFECT_FILTER_NAMES);
    options.push(OTHER_LABEL.to_string());
    options
}

fn other_index() -> usize {
    EFFECT_FILTER_NAMES.len()
}

fn non_empty(value: &str) -> Result<Validation, inquire::CustomUserError> {
    Ok(if value.trim().is_empty() {
        Validation::Invalid("The filter name cannot be empty".into())
    } else {
        Validation::Valid
    })
}

fn ask_plugin_name(input: &dyn Input, current: Option<&str>) -> Result<String> {
    let name = match current {
        Some(current) => {
            input.prompt_text_with_default(PLUGIN_NAME_PROMPT, current, Some(&non_empty))?
        }
        None => input.prompt_text(PLUGIN_NAME_PROMPT, Some("MyFilter"), None, Some(&non_empty))?,
    };
    Ok(name.trim().to_string())
}

/// Asks which filter a new effect applies (the filter is required).
fn prompt_new_filter(input: &dyn Input) -> Result<String> {
    let chosen = select_index(input, FILTER_PROMPT, &filter_options())?;
    if chosen == other_index() {
        return ask_plugin_name(input, None);
    }
    Ok(EFFECT_FILTER_NAMES[chosen].to_string())
}

/// Offers the filters with the current one marked; `None` when it stays.
///
/// A missing name (a file the SDK cannot load) shows as "unset", so any pick repairs it.
fn ask_filter(input: &dyn Input, current: Option<&str>) -> Result<Option<String>> {
    let current = current.filter(|c| !c.trim().is_empty());
    let current_index = current.map(|name| {
        EFFECT_FILTER_NAMES
            .iter()
            .position(|known| *known == name)
            .unwrap_or_else(other_index)
    });
    // A plugin filter is shown by its own name.
    let options = filter_options();
    let prompt_label = match current {
        Some(name) if current_index == Some(other_index()) => {
            format!("{FILTER_PROMPT} (plugin: {name})")
        }
        _ => FILTER_PROMPT.to_string(),
    };
    match ask_choice_or_unset(input, &prompt_label, &options, current_index)? {
        Some(i) if i < other_index() => Ok(Some(EFFECT_FILTER_NAMES[i].to_string())),
        Some(_) => ask_plugin_name(input, None).map(Some),
        None if current_index == Some(other_index()) => {
            let name = ask_plugin_name(input, current)?;
            Ok((Some(name.as_str()) != current).then_some(name))
        }
        None => Ok(None),
    }
}

/// Asks for parameters one at a time until the author is done.
fn prompt_new_parameters(
    input: &dyn Input,
    validator: &ProjectValidator,
) -> Result<Vec<RtpcCompatibleValue>> {
    let mut parameters = Vec::new();
    loop {
        let question = if parameters.is_empty() {
            "Add a parameter?"
        } else {
            "Add another parameter?"
        };
        if !input.confirm(question, Some(parameters.is_empty()))? {
            return Ok(parameters);
        }
        let label = format!("Parameter {}", parameters.len());
        parameters.push(prompt_rtpc_value(input, validator, &label, 0.0)?);
    }
}

/// Walks a new effect through the filter and every optional field no flag already set.
///
/// The filter comes first (it is required); the author may then accept defaults for
/// what is left (no parameters) in a single answer.
pub fn complete_create_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    patch: EffectPatch,
) -> Result<EffectPatch> {
    let effect = match patch.effect {
        Some(effect) => effect,
        None => prompt_new_filter(input)?,
    };
    let parameters =
        if !patch.parameters.is_empty() || input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))? {
            patch.parameters
        } else {
            ParamsPatch {
                replace: Some(prompt_new_parameters(input, validator)?),
                ..Default::default()
            }
        };
    Ok(EffectPatch {
        effect: Some(effect),
        parameters,
    })
}

fn describe(validator: &ProjectValidator, index: usize, value: &RtpcCompatibleValue) -> String {
    let shown = match (value.as_static(), value.rtpc.as_ref()) {
        (Some(v), _) => v.to_string(),
        (None, Some(rtpc)) => {
            let name = validator
                .resolve(RTPC_TARGET, rtpc.id)
                .map_or_else(|| format!("#{}", rtpc.id), |r| r.name);
            format!("RTPC {name}")
        }
        (None, None) => "unset".to_string(),
    };
    format!("{index}: {shown}")
}

/// Lets the user add, edit and remove parameters; returns the new list when it changed.
fn prompt_parameter_edits(
    input: &dyn Input,
    validator: &ProjectValidator,
    current: &[RtpcCompatibleValue],
) -> Result<Option<Vec<RtpcCompatibleValue>>> {
    let mut parameters = current.to_vec();
    loop {
        let choice = select_index(input, "What to do with the parameters?", &to_labels(&MENU))?;
        let labels: Vec<String> = parameters
            .iter()
            .enumerate()
            .map(|(i, p)| describe(validator, i, p))
            .collect();
        match choice {
            0 => {
                let label = format!("Parameter {}", parameters.len());
                parameters.push(prompt_rtpc_value(input, validator, &label, 0.0)?);
            }
            1 | 2 if parameters.is_empty() => {}
            1 => {
                let index = select_index(input, "Parameter to edit", &labels)?;
                let start = parameters[index].as_static().unwrap_or(0.0);
                parameters[index] =
                    prompt_rtpc_value(input, validator, &format!("Parameter {index}"), start)?;
            }
            2 => {
                let index = select_index(input, "Parameter to remove", &labels)?;
                parameters.remove(index);
            }
            _ => break,
        }
    }
    Ok((parameters != current).then_some(parameters))
}

/// Offers every effect field with the current value; unchanged fields stay out of the patch.
pub fn prompt_update_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    effect: &Effect,
) -> Result<EffectPatch> {
    let filter = ask_filter(input, effect.effect.as_deref())?;
    let replace = prompt_parameter_edits(
        input,
        validator,
        effect.parameters.as_deref().unwrap_or(&[]),
    )?;
    Ok(EffectPatch {
        effect: filter,
        parameters: ParamsPatch {
            replace,
            ..Default::default()
        },
    })
}
