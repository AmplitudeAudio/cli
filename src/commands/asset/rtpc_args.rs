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

//! Command-line flags of `rtpc create` / `rtpc update` and their resolution
//! into an [`RtpcPatch`] (numbers and fader names checked).
//!
//! Flag names follow the schema fields of `RtpcDefinition` and
//! `RtpcFadingSettings`: `min_value`, `max_value`, `default_value`, and
//! `fade_settings.{enabled, fade_attack, fade_release}.{duration, fader}`.

use anyhow::Result;
use clap::Args;

use crate::assets::extensions::FADER_ALGORITHM_NAMES;
use crate::common::errors::{CliError, codes};
use crate::services::rtpc::{FadePatch, RtpcPatch};

/// Every RTPC flag; shared by `create` and `update`.
#[derive(Args, Debug, Default, Clone)]
pub struct RtpcArgs {
    /// Lowest value the RTPC can take (must be lower than --max-value)
    #[arg(long, allow_hyphen_values = true)]
    pub min_value: Option<f64>,

    /// Highest value the RTPC can take
    #[arg(long, allow_hyphen_values = true)]
    pub max_value: Option<f64>,

    /// Initial value of the RTPC, within the range (default: 0, kept in range)
    #[arg(long, allow_hyphen_values = true)]
    pub default_value: Option<f64>,

    /// Fade between values instead of jumping to the target (true/false)
    #[arg(long)]
    pub fade_settings_enabled: Option<bool>,

    /// Duration in seconds of the fade used when the value increases
    #[arg(long, allow_hyphen_values = true)]
    pub fade_attack_duration: Option<f64>,

    /// Fader algorithm used when the value increases
    #[arg(long)]
    pub fade_attack_fader: Option<String>,

    /// Duration in seconds of the fade used when the value decreases
    #[arg(long, allow_hyphen_values = true)]
    pub fade_release_duration: Option<f64>,

    /// Fader algorithm used when the value decreases
    #[arg(long)]
    pub fade_release_fader: Option<String>,
}

fn check_finite(flag: &str, value: f64) -> Result<f64> {
    if value.is_finite() {
        return Ok(value);
    }
    Err(CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid {flag}: {value}"),
        "RTPC values must be finite numbers",
    )
    .with_suggestion(format!("Pass a finite number to {flag}, e.g. {flag} 1"))
    .into())
}

fn check_duration(flag: &str, value: f64) -> Result<f64> {
    if value.is_finite() && value >= 0.0 {
        return Ok(value);
    }
    Err(CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid {flag}: {value}"),
        "A fade duration must be a finite number of seconds that is not negative",
    )
    .with_suggestion(format!("Use {flag} with a value >= 0, e.g. {flag} 0.5"))
    .into())
}

/// Resolves a fader name to its canonical spelling (case-insensitive).
pub fn parse_fader(flag: &str, text: &str) -> Result<String> {
    FADER_ALGORITHM_NAMES
        .iter()
        .find(|n| n.eq_ignore_ascii_case(text.trim()))
        .map(|n| n.to_string())
        .ok_or_else(|| {
            CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("Invalid {flag}: unknown fader '{}'", text.trim()),
                format!(
                    "The fader must be one of: {}",
                    FADER_ALGORITHM_NAMES.join(", ")
                ),
            )
            .with_suggestion(format!("Use {flag} with one of the listed faders"))
            .into()
        })
}

/// Turns the raw flags into a patch.
pub fn resolve_patch(args: &RtpcArgs) -> Result<RtpcPatch> {
    let number = |flag: &str, v: Option<f64>| v.map(|v| check_finite(flag, v)).transpose();
    let duration = |flag: &str, v: Option<f64>| v.map(|v| check_duration(flag, v)).transpose();
    let fader = |flag: &str, v: &Option<String>| {
        v.as_deref().map(|text| parse_fader(flag, text)).transpose()
    };

    Ok(RtpcPatch {
        min_value: number("--min-value", args.min_value)?,
        max_value: number("--max-value", args.max_value)?,
        default_value: number("--default-value", args.default_value)?,
        fade: FadePatch {
            enabled: args.fade_settings_enabled,
            attack_duration: duration("--fade-attack-duration", args.fade_attack_duration)?,
            attack_fader: fader("--fade-attack-fader", &args.fade_attack_fader)?,
            release_duration: duration("--fade-release-duration", args.fade_release_duration)?,
            release_fader: fader("--fade-release-fader", &args.fade_release_fader)?,
        },
    })
}
