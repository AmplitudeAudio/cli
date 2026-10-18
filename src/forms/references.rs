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

use crate::assets::{AssetType, ProjectValidator};

pub use crate::assets::validator::AssetRef;

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

/// Every asset type that can hold references.
const ALL_ASSET_TYPES: [AssetType; 10] = [
    AssetType::Sound,
    AssetType::Collection,
    AssetType::Switch,
    AssetType::SwitchContainer,
    AssetType::Soundbank,
    AssetType::Event,
    AssetType::Effect,
    AssetType::Attenuation,
    AssetType::Rtpc,
    AssetType::Pipeline,
];

fn read_json(path: &std::path::Path) -> Option<serde_json::Value> {
    if !path.is_file() || path.extension().is_none_or(|e| e != "json") {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Like [`referrers`], but also reports each referrer's asset type.
pub fn referrers_with_type(
    validator: &ProjectValidator,
    target: RefTarget,
    id: u64,
) -> Vec<(AssetType, AssetRef)> {
    // 0 means "unset", never a reference.
    if id == 0 {
        return Vec::new();
    }
    let mut found = Vec::new();
    for asset_type in ALL_ASSET_TYPES {
        let fields: Vec<&str> = rules_for(asset_type)
            .into_iter()
            .filter(|r| r.target == target && !r.field.contains('.'))
            .map(|r| r.field)
            .collect();
        if fields.is_empty() {
            continue;
        }
        let dir = validator.sources_dir().join(asset_type.directory_name());
        for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
            let Some(value) = read_json(entry.path()) else {
                continue;
            };
            let points_at_target = fields
                .iter()
                .any(|f| value.get(*f).and_then(|v| v.as_u64()) == Some(id));
            let own_id = value.get("id").and_then(|v| v.as_u64());
            let name = value.get("name").and_then(|v| v.as_str());
            if let (true, Some(own_id), Some(name)) = (points_at_target, own_id, name) {
                found.push((
                    asset_type,
                    AssetRef {
                        id: own_id,
                        name: name.to_string(),
                    },
                ));
            }
        }
    }
    found
}

/// Lists the assets whose reference fields point at object `id` of `target`.
///
/// Scans every asset JSON file under the project's sources directories and
/// applies each [`RefRule`] aimed at `target`. Only top-level fields are
/// checked: rules with a dotted path (references nested inside lists or
/// tables, such as `sounds.sound`) are skipped for now, so assets that hold
/// the reference only in such a nested entry are not reported.
pub fn referrers(validator: &ProjectValidator, target: RefTarget, id: u64) -> Vec<AssetRef> {
    referrers_with_type(validator, target, id)
        .into_iter()
        .map(|(_, asset)| asset)
        .collect()
}
