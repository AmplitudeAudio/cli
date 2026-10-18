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
//! meaningful value (static values only for RTPC-compatible fields; partial
//! sub-field coverage is recorded in the field audit document). Every other
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
        // CRUD commands for these types arrive in later tasks.
        AssetType::Attenuation | AssetType::Rtpc | AssetType::Pipeline => &[],
    }
}

/// Schema fields deliberately not yet covered by any create/update flow.
pub const KNOWN_GAPS: &[(AssetType, &str)] = &[
    // Sound
    (AssetType::Sound, "effect"),
    (AssetType::Sound, "near_field_gain"),
    (AssetType::Sound, "pitch"),
    (AssetType::Sound, "attenuation"),
    (AssetType::Sound, "scope"),
    (AssetType::Sound, "fader"),
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
    // Attenuation
    (AssetType::Attenuation, "id"),
    (AssetType::Attenuation, "name"),
    (AssetType::Attenuation, "max_distance"),
    (AssetType::Attenuation, "shape"),
    (AssetType::Attenuation, "gain_curve"),
    (AssetType::Attenuation, "spread_curve"),
    (AssetType::Attenuation, "air_absorption"),
    // Rtpc
    (AssetType::Rtpc, "id"),
    (AssetType::Rtpc, "name"),
    (AssetType::Rtpc, "min_value"),
    (AssetType::Rtpc, "max_value"),
    (AssetType::Rtpc, "default_value"),
    (AssetType::Rtpc, "fade_settings"),
    // Pipeline
    (AssetType::Pipeline, "id"),
    (AssetType::Pipeline, "name"),
    (AssetType::Pipeline, "nodes"),
];
