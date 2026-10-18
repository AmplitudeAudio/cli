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

//! The scalar and enum fields of `engine_config_definition.fbs`.
//!
//! One table describes every field a flag can set: its section (nested
//! table), schema name, kind, schema default and whether the user must supply
//! it. The flag name is derived from it (`--<section>-<field>` in kebab-case;
//! top-level fields have no section prefix), so flags, prompts, validation and
//! the update logic share one source. Vectors of tables and curves have no
//! flag; they come from `--from-json`.

use serde_json::{Value, json};

use crate::common::errors::{CliError, codes};

/// File suffix of an engine config (`<name>.config.json`).
pub const ENGINE_CONFIG_SUFFIX: &str = ".config.json";
/// Runtime extension of pipeline files (`file_extension` of `pipeline_definition.fbs`).
pub const PIPELINE_EXTENSION: &str = ".ampipeline";
/// Runtime extension of bus files (`file_extension` of `buses_definition.fbs`).
pub const BUSES_EXTENSION: &str = ".ambus";
/// Runtime suffix of bus files (`<name>.buses.json` compiles to `<name>.buses.ambus`).
pub const BUSES_RUNTIME_SUFFIX: &str = ".buses.ambus";

/// `ePlaybackOutputFormat`.
pub const OUTPUT_FORMATS: &[(&str, i64)] = &[
    ("Default", 0),
    ("UInt8", 1),
    ("Int16", 2),
    ("Int24", 3),
    ("Int32", 4),
    ("Float32", 5),
];
/// `ePlaybackOutputChannels`.
pub const OUTPUT_CHANNELS: &[(&str, i64)] = &[
    ("Default", 0),
    ("Mono", 1),
    ("Stereo", 2),
    ("Quad", 4),
    ("Surround_5_1", 6),
    ("Surround_7_1", 8),
];
/// `eListenerFetchMode`.
pub const LISTENER_FETCH_MODES: &[(&str, i64)] = &[
    ("None", 0),
    ("Nearest", 1),
    ("Farthest", 2),
    ("Default", 3),
    ("First", 4),
    ("Last", 5),
];
/// `PanningMode` (common.fbs).
pub const PANNING_MODES: &[(&str, i64)] = &[
    ("Stereo", 0),
    ("BinauralLowQuality", 1),
    ("BinauralMediumQuality", 2),
    ("BinauralHighQuality", 3),
];
/// `HRIRSphereSamplingMode`.
pub const HRIR_SAMPLING_MODES: &[(&str, i64)] = &[("Bilinear", 0), ("NearestNeighbor", 1)];

/// What kind of value a field holds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// `uint` (32-bit).
    Uint,
    /// `float`.
    Float,
    /// `bool`.
    Bool,
    /// An enum: accepted names with their numeric values.
    Enum(&'static [(&'static str, i64)]),
    /// A string.
    Text,
}

/// Whether the user must supply a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Always (a schema `(required)` string, or a count without a usable default).
    Always,
    /// Whenever its (optional) section is present.
    WithSection,
    /// Never: the schema default applies, or the field is left out.
    Optional,
}

/// One field of the engine config that a flag can set.
#[derive(Debug, Clone, Copy)]
pub struct FieldSpec {
    /// Nested table the field lives in; `None` for top-level fields.
    pub section: Option<&'static str>,
    /// Schema field name.
    pub field: &'static str,
    pub kind: Kind,
    /// Schema default, in flag syntax; `None` when the schema has none.
    pub default: Option<&'static str>,
    pub need: Need,
    /// One-line description, used by prompts.
    pub help: &'static str,
}

impl FieldSpec {
    /// Dotted key: `output.frequency`, `buses_file`.
    pub fn key(&self) -> String {
        match self.section {
            Some(section) => format!("{section}.{}", self.field),
            None => self.field.to_string(),
        }
    }

    /// Long flag: `--output-frequency`, `--buses-file`.
    pub fn flag(&self) -> String {
        format!("--{}", self.key().replace(['.', '_'], "-"))
    }
}

const fn spec(
    section: Option<&'static str>,
    field: &'static str,
    kind: Kind,
    default: Option<&'static str>,
    need: Need,
    help: &'static str,
) -> FieldSpec {
    FieldSpec {
        section,
        field,
        kind,
        default,
        need,
        help,
    }
}

const OUT: Option<&str> = Some("output");
const MIX: Option<&str> = Some("mixer");
const GAME: Option<&str> = Some("game");
const HRTF: Option<&str> = Some("hrtf");

/// Every flag-settable field, in schema order.
pub const FIELDS: &[FieldSpec] = &[
    spec(
        OUT,
        "frequency",
        Kind::Uint,
        Some("48000"),
        Need::Optional,
        "Output sampling frequency (samples per second)",
    ),
    spec(
        OUT,
        "buffer_size",
        Kind::Uint,
        Some("1024"),
        Need::Optional,
        "Samples processed per output buffer",
    ),
    spec(
        OUT,
        "format",
        Kind::Enum(OUTPUT_FORMATS),
        Some("Float32"),
        Need::Optional,
        "Output buffer format",
    ),
    spec(
        OUT,
        "channels",
        Kind::Enum(OUTPUT_CHANNELS),
        Some("Stereo"),
        Need::Optional,
        "Output channel layout",
    ),
    spec(
        MIX,
        "active_channels",
        Kind::Uint,
        None,
        Need::Always,
        "Number of active mixer channels",
    ),
    spec(
        MIX,
        "virtual_channels",
        Kind::Uint,
        Some("0"),
        Need::Optional,
        "Number of virtual channels in addition to the active ones",
    ),
    spec(
        MIX,
        "panning_mode",
        Kind::Enum(PANNING_MODES),
        Some("Stereo"),
        Need::Optional,
        "Panning mode (anything but Stereo needs HRTF)",
    ),
    spec(
        MIX,
        "pipeline",
        Kind::Text,
        None,
        Need::Always,
        "Pipeline asset to load",
    ),
    spec(
        MIX,
        "resampler",
        Kind::Text,
        None,
        Need::Optional,
        "Resampler name (empty: engine default)",
    ),
    spec(
        MIX,
        "max_instance_pipelines",
        Kind::Uint,
        Some("32"),
        Need::Optional,
        "Maximum per-instance pipelines of a separate-mode layer",
    ),
    spec(
        HRTF,
        "amir_file",
        Kind::Text,
        None,
        Need::WithSection,
        "Path to the AMIR asset file",
    ),
    spec(
        HRTF,
        "hrir_sampling",
        Kind::Enum(HRIR_SAMPLING_MODES),
        Some("Bilinear"),
        Need::Optional,
        "HRIR sphere sampling mode",
    ),
    spec(
        GAME,
        "listener_fetch_mode",
        Kind::Enum(LISTENER_FETCH_MODES),
        Some("Nearest"),
        Need::Optional,
        "How the best listener is fetched",
    ),
    spec(
        GAME,
        "listeners",
        Kind::Uint,
        Some("1"),
        Need::Optional,
        "Number of listeners to allocate",
    ),
    spec(
        GAME,
        "entities",
        Kind::Uint,
        Some("4096"),
        Need::Optional,
        "Number of entities to allocate",
    ),
    spec(
        GAME,
        "environments",
        Kind::Uint,
        Some("64"),
        Need::Optional,
        "Number of environments to allocate",
    ),
    spec(
        GAME,
        "rooms",
        Kind::Uint,
        Some("1024"),
        Need::Optional,
        "Number of rooms to allocate",
    ),
    spec(
        GAME,
        "sound_speed",
        Kind::Float,
        Some("343.0"),
        Need::Optional,
        "Speed of sound in the game",
    ),
    spec(
        GAME,
        "doppler_factor",
        Kind::Float,
        Some("1.0"),
        Need::Optional,
        "Doppler factor (0 disables the effect)",
    ),
    spec(
        GAME,
        "track_environments",
        Kind::Bool,
        Some("true"),
        Need::Optional,
        "Whether the game sends environment levels",
    ),
    spec(
        GAME,
        "track_rooms",
        Kind::Bool,
        Some("false"),
        Need::Optional,
        "Whether the game sends the room of each entity",
    ),
    spec(
        None,
        "buses_file",
        Kind::Text,
        None,
        Need::Always,
        "Bus file the engine loads",
    ),
    spec(
        None,
        "driver",
        Kind::Text,
        None,
        Need::Optional,
        "Audio driver name (empty: engine default)",
    ),
];

/// Looks a field up by dotted key.
pub fn find_field(key: &str) -> Option<&'static FieldSpec> {
    FIELDS.iter().find(|f| f.key() == key)
}

fn invalid(spec: &FieldSpec, text: &str, why: String) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid {}: '{text}'", spec.flag()),
        why,
    )
    .with_suggestion(format!(
        "Pass {} with a valid value (see --help)",
        spec.flag()
    ))
}

/// Names accepted for an enum field, comma separated.
pub fn enum_names(options: &[(&str, i64)]) -> String {
    options
        .iter()
        .map(|(n, _)| *n)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Turns the pipeline name or file name into the stored `<name>.ampipeline`.
pub fn pipeline_ref(text: &str) -> String {
    let name = text.strip_suffix(PIPELINE_EXTENSION).unwrap_or(text);
    format!("{name}{PIPELINE_EXTENSION}")
}

/// Turns a bus file name (`main`, `main.buses.json`, `main.buses.ambus`) into the
/// runtime name `main.buses.ambus` (the build replaces only the `.json` of `main.buses.json`).
pub fn buses_ref(text: &str) -> String {
    format!("{}{BUSES_RUNTIME_SUFFIX}", buses_ref_name(text))
}

/// The bus file name (without `.buses.json`) a `buses_file` value points at.
pub fn buses_ref_name(value: &str) -> &str {
    let name = value
        .strip_suffix(BUSES_EXTENSION)
        .or_else(|| value.strip_suffix(".json"))
        .unwrap_or(value);
    name.strip_suffix(".buses").unwrap_or(name)
}

/// The pipeline name an engine config's `mixer.pipeline` value points at.
pub fn pipeline_ref_name(value: &str) -> &str {
    value.strip_suffix(PIPELINE_EXTENSION).unwrap_or(value)
}

/// Parses flag text into the JSON value stored in the file.
///
/// Enums are matched case-insensitively and stored under their schema name.
/// `mixer.pipeline` and `buses_file` are stored in the runtime form the SDK
/// loads (`<name>.ampipeline`, `<name>.ambus`). An empty optional string means
/// "remove the field" and yields `Value::Null`.
pub fn parse_value(spec: &FieldSpec, text: &str) -> Result<Value, CliError> {
    let text = text.trim();
    match spec.kind {
        Kind::Uint => text.parse::<u32>().map(|v| json!(v)).map_err(|_| {
            invalid(
                spec,
                text,
                "Expected a whole number from 0 to 4294967295".into(),
            )
        }),
        Kind::Float => match text.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok(json!(v)),
            _ => Err(invalid(spec, text, "Expected a finite number".into())),
        },
        Kind::Bool => match text.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" => Ok(json!(true)),
            "false" | "no" | "off" => Ok(json!(false)),
            _ => Err(invalid(spec, text, "Expected true or false".into())),
        },
        Kind::Enum(options) => options
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(text))
            .map(|(name, _)| json!(name))
            .ok_or_else(|| {
                invalid(
                    spec,
                    text,
                    format!("Expected one of: {}", enum_names(options)),
                )
            }),
        Kind::Text if text.is_empty() && spec.need == Need::Optional => Ok(Value::Null),
        Kind::Text if text.is_empty() => {
            Err(invalid(spec, text, "The value cannot be empty".into()))
        }
        Kind::Text => Ok(json!(match spec.key().as_str() {
            "mixer.pipeline" => pipeline_ref(text),
            "buses_file" => buses_ref(text),
            _ => text.to_string(),
        })),
    }
}
