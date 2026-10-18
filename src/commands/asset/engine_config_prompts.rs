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

//! Interactive prompts for `engine-config create` and `engine-config update`.
//!
//! Only called when the session can prompt. Create walks the sections in
//! schema order; values already given by a flag or by `--from-json` are never
//! asked again, and each section offers to accept the defaults of its
//! remaining optional fields in one step.

use anyhow::Result;
use inquire::validator::Validation;
use serde_json::Value;

use super::engine_config_args::Override;
use crate::assets::engine_config_fields::{
    FIELDS, FieldSpec, Kind, Need, buses_ref_name, parse_value, pipeline_ref_name,
};
use crate::common::errors::{CliError, codes};
use crate::input::{Input, select_index};

const ACCEPT_DEFAULTS_PROMPT: &str = "Accept defaults for the remaining optional fields";
const HRTF_PROMPT: &str = "Configure HRTF (needed for binaural panning)?";
const SECTIONS: [Option<&str>; 5] = [
    Some("output"),
    Some("mixer"),
    Some("hrtf"),
    Some("game"),
    None,
];
const DONE: &str = "Done";

/// Existing project objects a reference field can point at.
pub struct Choices<'a> {
    pub pipelines: &'a [String],
    pub bus_files: &'a [String],
}

fn section_label(section: Option<&str>) -> &str {
    section.unwrap_or("general")
}

fn raw_text(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// The value of `spec` in `config`, as flag text (reference fields as bare names).
fn current_text(config: &Value, spec: &FieldSpec) -> Option<String> {
    let holder = match spec.section {
        Some(section) => config.get(section)?,
        None => config,
    };
    let text = raw_text(holder.get(spec.field)?)?;
    Some(match spec.key().as_str() {
        "mixer.pipeline" => pipeline_ref_name(&text).to_string(),
        "buses_file" => buses_ref_name(&text).to_string(),
        _ => text,
    })
}

fn validate_for(
    spec: &'static FieldSpec,
) -> impl Fn(&str) -> Result<Validation, inquire::CustomUserError> {
    move |text| {
        Ok(match parse_value(spec, text) {
            Ok(_) => Validation::Valid,
            Err(e) => Validation::Invalid(e.why.into()),
        })
    }
}

fn pick(input: &dyn Input, label: &str, names: &[String], current: Option<&str>) -> Result<String> {
    let mut ordered = names.to_vec();
    if let Some(index) = current.and_then(|c| ordered.iter().position(|n| n == c)) {
        ordered.swap(0, index);
    }
    let index = select_index(input, label, &ordered)?;
    Ok(ordered[index].clone())
}

fn empty_choices(spec: &FieldSpec, what: &str, command: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_EMPTY_REFERENCE,
        format!("No {what} available for '{}'", spec.key()),
        format!("the project has no {what} and {} is required", spec.flag()),
    )
    .with_suggestion(format!("Create one first with `{command}`"))
}

/// Asks for one field; `current` is offered first. Returns the text to parse.
fn ask_text(
    input: &dyn Input,
    spec: &'static FieldSpec,
    current: Option<&str>,
    choices: &Choices,
) -> Result<String> {
    let label = format!("{} ({})", spec.help, spec.key());
    match (spec.key().as_str(), spec.kind) {
        ("mixer.pipeline", _) => {
            if choices.pipelines.is_empty() {
                return Err(empty_choices(spec, "pipeline", "am asset pipeline create").into());
            }
            pick(input, &label, choices.pipelines, current)
        }
        ("buses_file", _) => {
            if choices.bus_files.is_empty() {
                return Err(empty_choices(spec, "bus file", "am asset bus-file create").into());
            }
            pick(input, &label, choices.bus_files, current)
        }
        (_, Kind::Enum(options)) => {
            let names: Vec<String> = options.iter().map(|(n, _)| n.to_string()).collect();
            pick(input, &label, &names, current)
        }
        (_, Kind::Bool) => {
            let default = current.or(spec.default).is_some_and(|d| d == "true");
            Ok(input.confirm(&label, Some(default))?.to_string())
        }
        _ => {
            let validator = validate_for(spec);
            match current.or(spec.default) {
                Some(default) => {
                    Ok(input.prompt_text_with_default(&label, default, Some(&validator))?)
                }
                None => Ok(input.prompt_text(&label, None, None, Some(&validator))?),
            }
        }
    }
}

fn is_given(set: &[Override], base: &Value, spec: &FieldSpec) -> bool {
    set.iter().any(|(key, _)| *key == spec.key()) || current_text(base, spec).is_some()
}

fn given_text(set: &[Override], base: &Value, spec: &FieldSpec) -> Option<String> {
    set.iter()
        .find(|(key, _)| *key == spec.key())
        .and_then(|(_, v)| raw_text(v))
        .or_else(|| current_text(base, spec))
}

fn section_specs(section: Option<&str>) -> impl Iterator<Item = &'static FieldSpec> {
    FIELDS.iter().filter(move |f| f.section == section)
}

fn push(set: &mut Vec<Override>, spec: &FieldSpec, text: &str) -> Result<()> {
    set.push((spec.key(), parse_value(spec, text)?));
    Ok(())
}

fn complete_section(
    input: &dyn Input,
    section: Option<&str>,
    base: &Value,
    set: &mut Vec<Override>,
    choices: &Choices,
) -> Result<()> {
    let pending: Vec<&'static FieldSpec> = section_specs(section)
        .filter(|s| !is_given(set, base, s))
        .collect();
    for spec in pending.iter().filter(|s| s.need != Need::Optional) {
        let text = ask_text(input, spec, None, choices)?;
        push(set, spec, &text)?;
    }
    let optional: Vec<&&'static FieldSpec> = pending
        .iter()
        .filter(|s| s.need == Need::Optional)
        .collect();
    if optional.is_empty()
        || input.confirm(
            &format!("{ACCEPT_DEFAULTS_PROMPT} ({})?", section_label(section)),
            Some(true),
        )?
    {
        return Ok(());
    }
    for spec in optional {
        let text = ask_text(input, spec, None, choices)?;
        if !text.trim().is_empty() {
            push(set, spec, &text)?;
        }
    }
    Ok(())
}

fn hrtf_wanted(input: &dyn Input, base: &Value, set: &[Override]) -> Result<bool> {
    let given = section_specs(Some("hrtf")).any(|s| is_given(set, base, s));
    let panning = FIELDS
        .iter()
        .find(|f| f.key() == "mixer.panning_mode")
        .and_then(|s| given_text(set, base, s));
    let binaural = panning.is_some_and(|p| !p.eq_ignore_ascii_case("Stereo") && p != "0");
    Ok(given || binaural || input.confirm(HRTF_PROMPT, Some(false))?)
}

/// Completes a new config: asks for every required value and, per section, either
/// accepts the defaults of the optional ones or asks for them.
pub fn complete_create(
    input: &dyn Input,
    base: Option<&Value>,
    mut set: Vec<Override>,
    choices: &Choices,
) -> Result<Vec<Override>> {
    let empty = Value::Null;
    let base = base.unwrap_or(&empty);
    for section in SECTIONS {
        if section == Some("hrtf") && !hrtf_wanted(input, base, &set)? {
            continue;
        }
        complete_section(input, section, base, &mut set, choices)?;
    }
    Ok(set)
}

fn changed(spec: &FieldSpec, current: Option<&str>, answer: &str) -> Result<Option<Override>> {
    let value = parse_value(spec, answer)?;
    let unchanged = match current {
        Some(text) => parse_value(spec, text).is_ok_and(|v| v == value),
        None => value.is_null(),
    };
    Ok((!unchanged).then(|| (spec.key(), value)))
}

fn update_section(
    input: &dyn Input,
    section: Option<&str>,
    existing: &Value,
    choices: &Choices,
) -> Result<Vec<Override>> {
    let mut changes = Vec::new();
    for spec in section_specs(section) {
        let current = current_text(existing, spec);
        let answer = ask_text(input, spec, current.as_deref(), choices)?;
        if let Some(change) = changed(spec, current.as_deref(), &answer)? {
            changes.push(change);
        }
    }
    Ok(changes)
}

/// Lets the user pick sections of `existing` to edit, offering current values;
/// returns only what changed.
pub fn prompt_update(
    input: &dyn Input,
    existing: &Value,
    choices: &Choices,
) -> Result<Vec<Override>> {
    let mut labels: Vec<String> = SECTIONS
        .iter()
        .map(|s| section_label(*s).to_string())
        .collect();
    labels.push(DONE.to_string());
    let mut changes: Vec<Override> = Vec::new();
    loop {
        let index = select_index(input, "Section to edit", &labels)?;
        let Some(section) = SECTIONS.get(index) else {
            break;
        };
        for change in update_section(input, *section, existing, choices)? {
            changes.retain(|(key, _)| *key != change.0);
            changes.push(change);
        }
    }
    Ok(changes)
}
