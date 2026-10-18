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

//! Command-line flags of `bus create` / `bus update` and their resolution
//! into a [`BusPatch`] (numbers, faders and duck syntax checked).
//!
//! Flag names follow the schema fields of `BusDefinition`: `gain`, `fader`,
//! `duck_buses` (`--duck`), and `child_buses` (kept from `--parent`).

use anyhow::Result;
use clap::Args;

use super::rtpc_args::parse_fader;
use crate::assets::bus::DEFAULT_FADER;
use crate::common::errors::{CliError, codes};
use crate::services::bus::{BusPatch, DuckSpec};

/// Syntax of one `--duck` value, shown in `--help` and in errors.
pub const DUCK_SYNTAX: &str =
    "<bus>:<target_gain>:<fade_in>:<fade_out>[:<fade_in_fader>[:<fade_out_fader>]]";

/// Every bus flag; shared by `create` and `update`.
#[derive(Args, Debug, Default, Clone)]
pub struct BusArgs {
    /// Bus file (name without '.buses.json'); required when the project has several
    #[arg(long)]
    pub file: Option<String>,

    /// Linear gain applied to every sound played on this bus (0 or more, default 1)
    #[arg(long, allow_hyphen_values = true)]
    pub gain: Option<f64>,

    /// Name of the parent bus, in the same file (default: master)
    #[arg(long)]
    pub parent: Option<String>,

    /// Fader algorithm used when fading the bus gain (default: Linear)
    #[arg(long)]
    pub fader: Option<String>,

    /// Duck another bus of the file while this bus plays; repeatable.
    /// Format: <bus>:<target_gain>:<fade_in>:<fade_out>[:<fade_in_fader>[:<fade_out_fader>]]
    /// (gain the ducked bus fades to, fade durations in seconds, faders default to Linear)
    #[arg(long, value_name = "BUS:GAIN:FADE_IN:FADE_OUT")]
    pub duck: Vec<String>,
}

fn invalid(flag: &str, text: String, why: String, suggestion: String) -> anyhow::Error {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid {flag}: {text}"),
        why,
    )
    .with_suggestion(suggestion)
    .into()
}

/// A finite number that is 0 or more.
fn non_negative(flag: &str, text: &str) -> Result<f64> {
    match text.trim().parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => Ok(v),
        _ => Err(invalid(
            flag,
            text.to_string(),
            "The value must be a finite number that is 0 or more".to_string(),
            format!("Use {flag} with a value >= 0, e.g. {flag} 0.5"),
        )),
    }
}

/// Parses one `--duck` value (see [`DUCK_SYNTAX`]).
pub fn parse_duck(text: &str) -> Result<DuckSpec> {
    let parts: Vec<&str> = text.split(':').map(str::trim).collect();
    if !(4..=6).contains(&parts.len()) || parts[0].is_empty() {
        return Err(invalid(
            "--duck",
            text.to_string(),
            format!("Expected {DUCK_SYNTAX}"),
            "Example: --duck music:0.25:1:2 (duck 'music' to 0.25, fading in 1s and out 2s)"
                .to_string(),
        ));
    }
    let fader = |index: usize| -> Result<String> {
        parts.get(index).map_or_else(
            || Ok(DEFAULT_FADER.to_string()),
            |f| parse_fader("--duck", f),
        )
    };
    Ok(DuckSpec {
        bus: parts[0].to_string(),
        target_gain: non_negative("--duck target_gain", parts[1])?,
        fade_in: non_negative("--duck fade_in", parts[2])?,
        fade_out: non_negative("--duck fade_out", parts[3])?,
        fade_in_fader: fader(4)?,
        fade_out_fader: fader(5)?,
    })
}

/// Turns the raw flags into a patch (`ducks` is `Some` only when `--duck` was given).
pub fn resolve_patch(args: &BusArgs) -> Result<BusPatch> {
    let ducks = if args.duck.is_empty() {
        None
    } else {
        Some(
            args.duck
                .iter()
                .map(|d| parse_duck(d))
                .collect::<Result<Vec<_>>>()?,
        )
    };
    Ok(BusPatch {
        gain: args
            .gain
            .map(|g| non_negative("--gain", &g.to_string()))
            .transpose()?,
        parent: args.parent.clone(),
        fader: args
            .fader
            .as_deref()
            .map(|f| parse_fader("--fader", f))
            .transpose()?,
        ducks,
    })
}
