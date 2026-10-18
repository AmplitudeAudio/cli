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

//! Flags of `engine-config create` / `engine-config update`.
//!
//! Every scalar and enum field of the engine config schema has a flag named
//! `--<section>-<field>` (kebab-case); top-level fields have no section prefix
//! (`--buses-file`, `--driver`). The flag-to-field mapping lives in
//! [`FIELDS`](crate::assets::engine_config_fields::FIELDS); this struct only
//! carries the raw text so that parsing and errors are shared.
//!
//! Lists and tables with no flag form (the obstruction/occlusion curves) are
//! supplied with `--from-json`.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde_json::Value;

use crate::assets::engine_config_fields::{find_field, parse_value};
use crate::common::errors::{CliError, codes};

/// An override: dotted field key and the JSON value to store (`Null` removes the field).
pub type Override = (String, Value);

/// Every engine config flag; shared by `create` and `update`.
#[derive(Args, Debug, Default, Clone)]
pub struct EngineConfigArgs {
    /// JSON file with a whole config (create) or whole sections (update); flags override it
    #[arg(long, value_name = "FILE")]
    pub from_json: Option<PathBuf>,

    /// Output sampling frequency in samples per second (default 48000)
    #[arg(long, value_name = "UINT")]
    pub output_frequency: Option<String>,

    /// Samples processed per output buffer, ideally a power of 2 (default 1024)
    #[arg(long, value_name = "UINT")]
    pub output_buffer_size: Option<String>,

    /// Output buffer format: Default, UInt8, Int16, Int24, Int32, Float32 (default Float32)
    #[arg(long, value_name = "FORMAT")]
    pub output_format: Option<String>,

    /// Output channel layout: Default, Mono, Stereo, Quad, Surround_5_1, Surround_7_1 (default Stereo)
    #[arg(long, value_name = "LAYOUT")]
    pub output_channels: Option<String>,

    /// Number of active mixer channels (default 0, which leaves the engine with no audible channels)
    #[arg(long, value_name = "UINT")]
    pub mixer_active_channels: Option<String>,

    /// Virtual mixer channels, tracked but not mixed (default 0)
    #[arg(long, value_name = "UINT")]
    pub mixer_virtual_channels: Option<String>,

    /// Panning mode: Stereo, BinauralLowQuality, BinauralMediumQuality, BinauralHighQuality (default Stereo; needs --hrtf-amir-file otherwise)
    #[arg(long, value_name = "MODE")]
    pub mixer_panning_mode: Option<String>,

    /// Pipeline asset to load, by name (required; stored as '<name>.ampipeline')
    #[arg(long, value_name = "PIPELINE")]
    pub mixer_pipeline: Option<String>,

    /// Resampler name, e.g. linear, cubic, sinc, sinc_best (pass '' to use the engine default)
    #[arg(long, value_name = "NAME")]
    pub mixer_resampler: Option<String>,

    /// Maximum per-instance pipelines of a separate-mode layer; 0 disables them (default 32)
    #[arg(long, value_name = "UINT")]
    pub mixer_max_instance_pipelines: Option<String>,

    /// Path of the AMIR asset file (required once HRTF is configured)
    #[arg(long, value_name = "PATH")]
    pub hrtf_amir_file: Option<String>,

    /// HRIR sphere sampling mode: Bilinear, NearestNeighbor (default Bilinear)
    #[arg(long, value_name = "MODE")]
    pub hrtf_hrir_sampling: Option<String>,

    /// How the best listener is fetched: None, Nearest, Farthest, Default, First, Last (default Nearest)
    #[arg(long, value_name = "MODE")]
    pub game_listener_fetch_mode: Option<String>,

    /// Number of listeners to allocate (default 1)
    #[arg(long, value_name = "UINT")]
    pub game_listeners: Option<String>,

    /// Number of entities to allocate (default 4096)
    #[arg(long, value_name = "UINT")]
    pub game_entities: Option<String>,

    /// Number of environments to allocate (default 64)
    #[arg(long, value_name = "UINT")]
    pub game_environments: Option<String>,

    /// Number of rooms to allocate (default 1024)
    #[arg(long, value_name = "UINT")]
    pub game_rooms: Option<String>,

    /// Speed of sound in the game (default 343.0)
    #[arg(long, value_name = "NUMBER", allow_hyphen_values = true)]
    pub game_sound_speed: Option<String>,

    /// Doppler factor; 0 disables the effect (default 1.0)
    #[arg(long, value_name = "NUMBER", allow_hyphen_values = true)]
    pub game_doppler_factor: Option<String>,

    /// Whether the game sends environment levels: true or false (default true)
    #[arg(long, value_name = "BOOL")]
    pub game_track_environments: Option<String>,

    /// Whether the game sends the room of each entity: true or false (default false)
    #[arg(long, value_name = "BOOL")]
    pub game_track_rooms: Option<String>,

    /// Bus file the engine loads, by name (required; stored as '<name>.buses.ambus')
    #[arg(long, value_name = "FILE")]
    pub buses_file: Option<String>,

    /// Audio driver name (pass '' to use the engine default)
    #[arg(long, value_name = "NAME")]
    pub driver: Option<String>,
}

impl EngineConfigArgs {
    /// The raw flag text per dotted field key, in schema order.
    fn raw(&self) -> Vec<(&'static str, Option<&str>)> {
        fn o(v: &Option<String>) -> Option<&str> {
            v.as_deref()
        }
        vec![
            ("output.frequency", o(&self.output_frequency)),
            ("output.buffer_size", o(&self.output_buffer_size)),
            ("output.format", o(&self.output_format)),
            ("output.channels", o(&self.output_channels)),
            ("mixer.active_channels", o(&self.mixer_active_channels)),
            ("mixer.virtual_channels", o(&self.mixer_virtual_channels)),
            ("mixer.panning_mode", o(&self.mixer_panning_mode)),
            ("mixer.pipeline", o(&self.mixer_pipeline)),
            ("mixer.resampler", o(&self.mixer_resampler)),
            (
                "mixer.max_instance_pipelines",
                o(&self.mixer_max_instance_pipelines),
            ),
            ("hrtf.amir_file", o(&self.hrtf_amir_file)),
            ("hrtf.hrir_sampling", o(&self.hrtf_hrir_sampling)),
            (
                "game.listener_fetch_mode",
                o(&self.game_listener_fetch_mode),
            ),
            ("game.listeners", o(&self.game_listeners)),
            ("game.entities", o(&self.game_entities)),
            ("game.environments", o(&self.game_environments)),
            ("game.rooms", o(&self.game_rooms)),
            ("game.sound_speed", o(&self.game_sound_speed)),
            ("game.doppler_factor", o(&self.game_doppler_factor)),
            ("game.track_environments", o(&self.game_track_environments)),
            ("game.track_rooms", o(&self.game_track_rooms)),
            ("buses_file", o(&self.buses_file)),
            ("driver", o(&self.driver)),
        ]
    }

    /// The fields set by flags, parsed and checked.
    pub fn overrides(&self) -> Result<Vec<Override>> {
        let mut found = Vec::new();
        for (key, text) in self.raw() {
            let (Some(text), Some(spec)) = (text, find_field(key)) else {
                continue;
            };
            found.push((key.to_string(), parse_value(spec, text)?));
        }
        Ok(found)
    }

    /// Reads the `--from-json` document, which must be a JSON object.
    pub fn json_document(&self) -> Result<Option<Value>> {
        let Some(path) = &self.from_json else {
            return Ok(None);
        };
        let text = std::fs::read_to_string(path).map_err(|e| {
            CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("Cannot read {}", path.display()),
                e.to_string(),
            )
            .with_suggestion("Pass --from-json with the path of a readable JSON file")
        })?;
        let value: Value = serde_json::from_str(&text).map_err(|e| {
            CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("Invalid JSON in {}", path.display()),
                e.to_string(),
            )
        })?;
        if !value.is_object() {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("{} is not a JSON object", path.display()),
                "--from-json takes a whole engine config, or an object holding whole sections",
            )
            .into());
        }
        Ok(Some(value))
    }
}

/// Borrows overrides in the shape the service functions take.
pub fn as_pairs(overrides: &[Override]) -> Vec<(&str, Value)> {
    overrides
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect()
}
