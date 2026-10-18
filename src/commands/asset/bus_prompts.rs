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

//! Interactive prompts for `bus create` and `bus update`.
//!
//! Every function here is only called when the session can prompt; values
//! already present in the patch are never asked for again. Parent and duck
//! targets are chosen among the buses of the file being edited, because the
//! SDK loads one file as one bus tree.

use anyhow::Result;
use serde_json::Value;

use super::rtpc_prompts::{ask, ask_fader, validate_duration};
use crate::assets::bus::{DEFAULT_FADER, MASTER_BUS_NAME, buses_of};
use crate::input::{Input, select_index};
use crate::services::bus::{BusPatch, BusValues, DuckSpec, find_bus, parent_name};

const ACCEPT_DEFAULTS_PROMPT: &str = "Accept defaults for remaining optional fields?";
const DEFAULT_DUCK_GAIN: f64 = 0.5;
const DEFAULT_DUCK_FADE: f64 = 1.0;

fn bus_names(file: &Value) -> Vec<String> {
    buses_of(file)
        .iter()
        .filter_map(|b| b.get("name").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// `names` with `first` moved to the front, so confirming keeps it.
fn first_then_rest(names: &[String], first: &str) -> Vec<String> {
    let mut ordered = names.to_vec();
    if let Some(index) = ordered.iter().position(|n| n == first) {
        ordered.swap(0, index);
    }
    ordered
}

fn pick_bus(input: &dyn Input, label: &str, names: &[String]) -> Result<String> {
    let index = select_index(input, label, names)?;
    Ok(names[index].clone())
}

fn prompt_duck(input: &dyn Input, targets: &[String]) -> Result<DuckSpec> {
    let bus = pick_bus(input, "Bus to duck", targets)?;
    let target_gain = ask(
        input,
        "Gain the ducked bus fades to",
        Some(DEFAULT_DUCK_GAIN),
        validate_duration,
    )?;
    let fade_in = ask(
        input,
        "Fade-in duration (seconds)",
        Some(DEFAULT_DUCK_FADE),
        validate_duration,
    )?;
    let fade_in_fader = ask_fader(input, "Fade-in fader", DEFAULT_FADER)?;
    let fade_out = ask(
        input,
        "Fade-out duration (seconds)",
        Some(DEFAULT_DUCK_FADE),
        validate_duration,
    )?;
    let fade_out_fader = ask_fader(input, "Fade-out fader", DEFAULT_FADER)?;
    Ok(DuckSpec {
        bus,
        target_gain,
        fade_in,
        fade_out,
        fade_in_fader,
        fade_out_fader,
    })
}

/// Asks for duck buses until the user is done.
fn prompt_ducks(input: &dyn Input, targets: &[String]) -> Result<Vec<DuckSpec>> {
    let mut ducks = Vec::new();
    let mut question = "Add a duck bus?";
    while !targets.is_empty() && input.confirm(question, Some(false))? {
        ducks.push(prompt_duck(input, targets)?);
        question = "Add another duck bus?";
    }
    Ok(ducks)
}

/// Completes a new bus: flags win, the rest is asked for (or defaulted in one step).
pub fn complete_create(
    input: &dyn Input,
    name: &str,
    patch: BusPatch,
    file: &Value,
) -> Result<BusValues> {
    let defaults = BusValues::with_defaults(name);
    let targets = bus_names(file);
    let all_given = patch.gain.is_some()
        && patch.parent.is_some()
        && patch.fader.is_some()
        && patch.ducks.is_some();
    let accept = all_given || input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(true))?;
    if accept {
        return Ok(BusValues {
            gain: patch.gain.unwrap_or(defaults.gain),
            parent: patch.parent.unwrap_or(defaults.parent),
            fader: patch.fader.unwrap_or(defaults.fader),
            ducks: patch.ducks.unwrap_or(defaults.ducks),
            ..defaults
        });
    }

    let gain = match patch.gain {
        Some(g) => g,
        None => ask(input, "Gain", Some(defaults.gain), validate_duration)?,
    };
    let parent = match patch.parent {
        Some(p) => p,
        None => pick_bus(
            input,
            "Parent bus",
            &first_then_rest(&targets, MASTER_BUS_NAME),
        )?,
    };
    let fader = match patch.fader {
        Some(f) => f,
        None => ask_fader(input, "Fader", &defaults.fader)?,
    };
    let ducks = match patch.ducks {
        Some(d) => d,
        None => prompt_ducks(input, &targets)?,
    };
    Ok(BusValues {
        gain,
        parent,
        fader,
        ducks,
        ..defaults
    })
}

/// Walks every editable field of the bus `name`, offering the current value
/// first; returns only what changed.
pub fn prompt_update_patch(input: &dyn Input, name: &str, file: &Value) -> Result<BusPatch> {
    let Some(current) = find_bus(file, name) else {
        return Ok(BusPatch::default());
    };
    let current_gain = current.get("gain").and_then(Value::as_f64).unwrap_or(1.0);
    let current_fader = current
        .get("fader")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_FADER)
        .to_string();
    let mut patch = BusPatch::default();

    let gain = ask(input, "Gain", Some(current_gain), validate_duration)?;
    patch.gain = (gain != current_gain).then_some(gain);

    let fader = ask_fader(input, "Fader", &current_fader)?;
    patch.fader = (fader != current_fader).then_some(fader);

    if name != MASTER_BUS_NAME {
        let current_parent = parent_name(file, name).unwrap_or_else(|| MASTER_BUS_NAME.to_string());
        let candidates: Vec<String> = bus_names(file).into_iter().filter(|n| n != name).collect();
        let parent = pick_bus(
            input,
            "Parent bus",
            &first_then_rest(&candidates, &current_parent),
        )?;
        patch.parent = (parent != current_parent).then_some(parent);
    }

    let targets: Vec<String> = bus_names(file).into_iter().filter(|n| n != name).collect();
    if input.confirm("Change the duck buses?", Some(false))? {
        patch.ducks = Some(prompt_ducks(input, &targets)?);
    }
    Ok(patch)
}
