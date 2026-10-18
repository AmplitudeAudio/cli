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

//! Hand-maintained table of `uint64` schema fields that reference other assets.
//!
//! The SDK schemas carry no machine-readable reference annotation, so this
//! table is derived from the field audit (`docs/superpowers/specs/2026-10-18-asset-field-audit.md`).
//! A test checks that every rule's field still exists in the schema.
//!
//! Field naming convention: top-level fields use their schema name
//! (`"bus"`); fields inside a nested table or list use a dotted path from the
//! top-level field (`"sounds.sound"` is the `sound` of each entry in
//! `sounds`).

use crate::assets::AssetType;

/// What a reference field points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefTarget {
    /// An asset of the given type under `sources/`.
    Asset(AssetType),
    /// A bus defined in a `*.buses.json` file of the project root.
    Bus,
}

/// Declares that `field` of `asset_type` holds the ID of another object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefRule {
    pub asset_type: AssetType,
    pub field: &'static str,
    pub target: RefTarget,
    /// When true, 0 (unset) is invalid; otherwise 0 means "none".
    pub required: bool,
}

const fn rule(
    asset_type: AssetType,
    field: &'static str,
    target: RefTarget,
    required: bool,
) -> RefRule {
    RefRule {
        asset_type,
        field,
        target,
        required,
    }
}

const BUS: RefTarget = RefTarget::Bus;
const EFFECT: RefTarget = RefTarget::Asset(AssetType::Effect);
const ATTENUATION: RefTarget = RefTarget::Asset(AssetType::Attenuation);

const SOUND_RULES: &[RefRule] = &[
    rule(AssetType::Sound, "bus", BUS, true),
    rule(AssetType::Sound, "effect", EFFECT, false),
    rule(AssetType::Sound, "attenuation", ATTENUATION, false),
];

const COLLECTION_RULES: &[RefRule] = &[
    rule(AssetType::Collection, "bus", BUS, true),
    rule(AssetType::Collection, "effect", EFFECT, false),
    rule(AssetType::Collection, "attenuation", ATTENUATION, false),
    // Zero handling of entry sounds is unconfirmed in the audit; kept optional.
    rule(
        AssetType::Collection,
        "sounds.sound",
        RefTarget::Asset(AssetType::Sound),
        false,
    ),
];

const SWITCH_CONTAINER_RULES: &[RefRule] = &[
    rule(AssetType::SwitchContainer, "bus", BUS, true),
    rule(
        AssetType::SwitchContainer,
        "switch_group",
        RefTarget::Asset(AssetType::Switch),
        true,
    ),
    rule(AssetType::SwitchContainer, "effect", EFFECT, false),
    rule(
        AssetType::SwitchContainer,
        "attenuation",
        ATTENUATION,
        false,
    ),
];

/// Returns every reference rule of `asset_type` (empty if it has none).
pub fn rules_for(asset_type: AssetType) -> Vec<RefRule> {
    match asset_type {
        AssetType::Sound => SOUND_RULES.to_vec(),
        AssetType::Collection => COLLECTION_RULES.to_vec(),
        AssetType::SwitchContainer => SWITCH_CONTAINER_RULES.to_vec(),
        AssetType::Switch
        | AssetType::Soundbank
        | AssetType::Event
        | AssetType::Effect
        | AssetType::Attenuation
        | AssetType::Rtpc
        | AssetType::Pipeline => Vec::new(),
    }
}

/// Returns the reference rule for `field` of `asset_type`, if it is a reference.
pub fn rule_for(asset_type: AssetType, field: &str) -> Option<RefRule> {
    rules_for(asset_type).into_iter().find(|r| r.field == field)
}
