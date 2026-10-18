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

//! Tracks which top-level schema fields the create/update commands cover.
//!
//! A field is "supported" when a create or update flow can set it to a
//! meaningful value (RTPC-compatible fields accept a static value or an RTPC
//! binding; partial sub-field coverage is recorded in the field audit document). Every other
//! schema field must be listed in [`KNOWN_GAPS`]. The coverage test fails when
//! a schema field is in neither list, and when a gap is not a real field or is
//! also supported, so the gap list can only shrink.

use crate::assets::AssetType;

/// Returns the schema field names that create/update cover today for `asset_type`.
pub fn supported_fields(asset_type: AssetType) -> &'static [&'static str] {
    match asset_type {
        AssetType::Sound => &[
            "id",
            "name",
            "gain",
            "bus",
            "priority",
            "stream",
            "loop",
            "spatialization",
            "path",
            "spread",
            "effect",
            "near_field_gain",
            "pitch",
            "attenuation",
            "scope",
            "fader",
        ],
        AssetType::Collection => &[
            "id",
            "name",
            "priority",
            "gain",
            "bus",
            "spatialization",
            "play_mode",
            "scheduler",
        ],
        AssetType::Switch => &["id", "name", "states"],
        AssetType::SwitchContainer => &[
            "id",
            "name",
            "switch_group",
            "default_switch_state",
            "entries",
        ],
        AssetType::Soundbank => &[
            "id",
            "name",
            "switch_containers",
            "collections",
            "sounds",
            "events",
            "attenuators",
            "switches",
            "rtpc",
            "effects",
        ],
        AssetType::Event => &["id", "name", "run_mode", "actions"],
        AssetType::Effect => &["id", "name", "effect", "parameters"],
        AssetType::Attenuation => &[
            "id",
            "name",
            "max_distance",
            "shape",
            "gain_curve",
            "air_absorption",
            "spread_curve",
        ],
        AssetType::Rtpc => &[
            "id",
            "name",
            "min_value",
            "max_value",
            "default_value",
            "fade_settings",
        ],
        AssetType::Pipeline => &["id", "name", "nodes"],
    }
}

/// Schema fields deliberately not yet covered by any create/update flow.
pub const KNOWN_GAPS: &[(AssetType, &str)] = &[
    // Collection
    (AssetType::Collection, "effect"),
    (AssetType::Collection, "pitch"),
    (AssetType::Collection, "sounds"),
    // Union discriminator vector that accompanies `sounds`; closes with it.
    (AssetType::Collection, "sounds_type"),
    (AssetType::Collection, "attenuation"),
    (AssetType::Collection, "scope"),
    (AssetType::Collection, "fader"),
    // Switch container
    (AssetType::SwitchContainer, "effect"),
    (AssetType::SwitchContainer, "priority"),
    (AssetType::SwitchContainer, "gain"),
    (AssetType::SwitchContainer, "pitch"),
    (AssetType::SwitchContainer, "bus"),
    (AssetType::SwitchContainer, "spatialization"),
    (AssetType::SwitchContainer, "attenuation"),
    (AssetType::SwitchContainer, "scope"),
    (AssetType::SwitchContainer, "fader"),
    (AssetType::SwitchContainer, "update_behavior"),
];

/// Fields of `PipelineNodeDefinition` the `--node` flag syntax covers.
pub const PIPELINE_NODE_SUPPORTED_FIELDS: &[&str] = &["id", "name", "consume", "parameters"];

/// `PipelineNodeDefinition` fields not covered by any pipeline command (none today).
pub const PIPELINE_NODE_KNOWN_GAPS: &[&str] = &[];

/// Fields of `BusDefinition` the bus commands cover.
///
/// Buses are entries of `*.buses.json` files, not an [`AssetType`], so they
/// have their own lists; `unit_schema_coverage_test` checks them against the schema.
pub const BUS_SUPPORTED_FIELDS: &[&str] =
    &["id", "name", "gain", "child_buses", "duck_buses", "fader"];

/// Fields of `DuckBusDefinition` the `--duck` flag covers.
pub const DUCK_BUS_SUPPORTED_FIELDS: &[&str] = &["id", "target_gain", "fade_in", "fade_out"];

/// `BusDefinition` fields not covered by any bus command (none today).
pub const BUS_KNOWN_GAPS: &[&str] = &[];

/// `DuckBusDefinition` fields not covered by any bus command (none today).
pub const DUCK_BUS_KNOWN_GAPS: &[&str] = &[];

/// Tables of `engine_config_definition.fbs`, as `(table, supported fields, known gaps)`.
///
/// Engine configs are files, not an [`AssetType`], so they have their own
/// lists. Scalar and enum fields have a `--<section>-<field>` flag; the two
/// obstruction/occlusion curves are set with `--from-json` (and default to the
/// template curves). `unit_schema_coverage_test` checks every list against the schema.
pub const ENGINE_CONFIG_TABLES: &[(&str, &[&str], &[&str])] = &[
    (
        "EngineConfigDefinition",
        &["output", "mixer", "hrtf", "game", "buses_file", "driver"],
        &[],
    ),
    (
        "PlaybackOutputConfig",
        &["frequency", "buffer_size", "format", "channels"],
        &[],
    ),
    (
        "AudioMixerConfig",
        &[
            "active_channels",
            "virtual_channels",
            "panning_mode",
            "pipeline",
            "resampler",
            "max_instance_pipelines",
        ],
        &[],
    ),
    (
        "GameSyncConfig",
        &[
            "listener_fetch_mode",
            "listeners",
            "entities",
            "environments",
            "rooms",
            "sound_speed",
            "doppler_factor",
            "obstruction",
            "occlusion",
            "track_environments",
            "track_rooms",
        ],
        &[],
    ),
    (
        "ObstructionOcclusionConfig",
        &["lpf_curve", "gain_curve"],
        &[],
    ),
    ("HRTFConfig", &["amir_file", "hrir_sampling"], &[]),
];

/// Returns every schema field name of `asset_type`, covered or not.
///
/// Covered fields come from [`supported_fields`]; gaps come from [`KNOWN_GAPS`]. The
/// coverage test keeps those two lists complete. Union tag companions
/// (`<field>_type`) of covered fields are included too, so a stale tag is not carried over.
pub fn known_field_names(asset_type: AssetType) -> Vec<String> {
    let covered = supported_fields(asset_type);
    let gaps = KNOWN_GAPS
        .iter()
        .filter(|(t, _)| *t == asset_type)
        .map(|(_, name)| *name);

    let mut names: Vec<String> = covered
        .iter()
        .copied()
        .chain(gaps)
        .map(str::to_string)
        .collect();
    names.extend(covered.iter().map(|name| format!("{name}_type")));
    names.sort();
    names.dedup();
    names
}
