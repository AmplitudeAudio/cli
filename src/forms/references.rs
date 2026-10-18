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

use crate::assets::generated::EventActionType;
use crate::assets::{AssetType, ProjectValidator};

pub use crate::assets::validator::AssetRef;

/// What a reference field points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefTarget {
    /// An asset of the given type under `sources/`.
    Asset(AssetType),
    /// A bus defined in a `*.buses.json` file of the project root.
    Bus,
    /// An object of any one of the listed targets (for example the sound, collection
    /// or switch container an event action plays).
    AnyOf(&'static [RefTarget]),
}

impl RefTarget {
    /// The single targets this one stands for (itself unless it is [`RefTarget::AnyOf`]).
    pub fn members(self) -> Vec<RefTarget> {
        match self {
            RefTarget::AnyOf(targets) => targets.iter().flat_map(|t| t.members()).collect(),
            single => vec![single],
        }
    }

    /// Whether objects of `other` can be pointed at by a reference to `self`.
    pub fn includes(self, other: RefTarget) -> bool {
        self.members().contains(&other)
    }
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

const SOUND: RefTarget = RefTarget::Asset(AssetType::Sound);
const COLLECTION: RefTarget = RefTarget::Asset(AssetType::Collection);
const SWITCH_CONTAINER: RefTarget = RefTarget::Asset(AssetType::SwitchContainer);

/// What Play/Pause/Resume/Stop/Seek actions target: `Engine::Play(id)` resolves a
/// sound, a collection or a switch container, and the other actions match the id
/// of the playing object, which is one of those three.
pub const PLAYBACK_TARGETS: RefTarget = RefTarget::AnyOf(&[SOUND, COLLECTION, SWITCH_CONTAINER]);

/// Every kind some event action can target; `Event::validate_rules` narrows it per action type.
const EVENT_TARGETS: RefTarget = RefTarget::AnyOf(&[SOUND, COLLECTION, SWITCH_CONTAINER, BUS]);

const EVENT_RULES: &[RefRule] = &[
    // MuteBus/UnmuteBus target buses, the other non-Wait actions the playback kinds.
    // Wait targets are durations, not references.
    rule(AssetType::Event, "actions.targets", EVENT_TARGETS, true),
];

const SOUND_RULES: &[RefRule] = &[
    rule(AssetType::Sound, "bus", BUS, true),
    rule(AssetType::Sound, "effect", EFFECT, false),
    rule(AssetType::Sound, "attenuation", ATTENUATION, false),
];

const COLLECTION_RULES: &[RefRule] = &[
    rule(AssetType::Collection, "bus", BUS, true),
    rule(AssetType::Collection, "effect", EFFECT, false),
    rule(AssetType::Collection, "attenuation", ATTENUATION, false),
    // `Collection.cpp` rejects an entry whose sound is 0 or unknown ("specifies an
    // invalid/unknown sound ID"), so the entry sound is required.
    rule(
        AssetType::Collection,
        "sounds.sound",
        RefTarget::Asset(AssetType::Sound),
        true,
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
        AssetType::Event => EVENT_RULES.to_vec(),
        AssetType::Switch
        | AssetType::Soundbank
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

/// Top-level `RtpcCompatibleValue` fields per asset type.
///
/// A value bound to an RTPC is stored as `<field>.rtpc.id`, so these fields
/// are references to [`AssetType::Rtpc`] too. Kept as a table (instead of
/// calling `forms::describe`) so that reference lookups do not need the SDK
/// schemas; a test checks it against the schema. Fields inside nested tables
/// or lists are listed in [`NESTED_RTPC_VALUE_FIELDS`] (effect parameters are
/// not covered yet).
pub const RTPC_VALUE_FIELDS: &[(AssetType, &str)] = &[
    (AssetType::Sound, "gain"),
    (AssetType::Sound, "near_field_gain"),
    (AssetType::Sound, "pitch"),
    (AssetType::Sound, "priority"),
    (AssetType::Sound, "spread"),
    (AssetType::Collection, "gain"),
    (AssetType::Collection, "pitch"),
    (AssetType::Collection, "priority"),
    (AssetType::SwitchContainer, "gain"),
    (AssetType::SwitchContainer, "pitch"),
    (AssetType::SwitchContainer, "priority"),
];

/// `RtpcCompatibleValue` fields inside lists of nested tables, as dotted paths.
///
/// Kept apart from [`RTPC_VALUE_FIELDS`], which mirrors the top-level schema
/// fields exactly. `sounds.gain` is the `gain` of each collection entry.
pub const NESTED_RTPC_VALUE_FIELDS: &[(AssetType, &str)] = &[
    (AssetType::Collection, "sounds.gain"),
    (AssetType::Collection, "sounds.pitch"),
];

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

/// The numbers found at the dotted `path` of `value`.
///
/// Every segment descends into an object field; when the field holds a list,
/// the walk continues in each element (`sounds.sound` yields the `sound` of
/// every entry of `sounds`). Missing fields and non-numbers are skipped.
pub fn ids_at_path(value: &serde_json::Value, path: &str) -> Vec<u64> {
    let mut current = vec![value];
    for segment in path.split('.') {
        current = current
            .into_iter()
            .filter_map(|v| v.get(segment))
            .flat_map(|v| match v.as_array() {
                Some(items) => items.iter().collect(),
                None => vec![v],
            })
            .collect();
    }
    current.into_iter().filter_map(|v| v.as_u64()).collect()
}

/// The ids the reference field `field` of an `asset_type` file holds for `target`.
///
/// Event action targets need care: a Wait action's targets are durations and a
/// bus action's targets are buses, so only actions whose type targets `target` count.
fn rule_ids(
    asset_type: AssetType,
    value: &serde_json::Value,
    field: &str,
    target: RefTarget,
) -> Vec<u64> {
    if asset_type != AssetType::Event || field != "actions.targets" {
        return ids_at_path(value, field);
    }
    value
        .get("actions")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .filter(|action| {
            action
                .get("type")
                .and_then(|t| t.as_str())
                .and_then(|t| EventActionType::from_name(t).ok())
                .and_then(|t| t.target_kinds())
                .is_some_and(|kinds| kinds.includes(target))
        })
        .flat_map(|action| ids_at_path(action, "targets"))
        .collect()
}

/// Whether the RTPC-compatible value at `field` (possibly a dotted path) is bound to RTPC `id`.
fn binds_rtpc(value: &serde_json::Value, field: &str, id: u64) -> bool {
    ids_at_path(value, &format!("{field}.rtpc.id")).contains(&id)
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
            .filter(|r| r.target.includes(target))
            .map(|r| r.field)
            .collect();
        let rtpc_fields: Vec<&str> = if target == RefTarget::Asset(AssetType::Rtpc) {
            RTPC_VALUE_FIELDS
                .iter()
                .chain(NESTED_RTPC_VALUE_FIELDS)
                .filter(|(t, _)| *t == asset_type)
                .map(|(_, f)| *f)
                .collect()
        } else {
            Vec::new()
        };
        if fields.is_empty() && rtpc_fields.is_empty() {
            continue;
        }
        let dir = validator.sources_dir().join(asset_type.directory_name());
        for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
            let Some(value) = read_json(entry.path()) else {
                continue;
            };
            let points_at_target = fields
                .iter()
                .any(|f| rule_ids(asset_type, &value, f, target).contains(&id))
                || rtpc_fields.iter().any(|f| binds_rtpc(&value, f, id));
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
/// applies each [`RefRule`] aimed at `target` (plus the RTPC bindings listed in
/// [`RTPC_VALUE_FIELDS`] and [`NESTED_RTPC_VALUE_FIELDS`] when `target` is an
/// RTPC). Dotted rules (references nested inside lists or tables, such as
/// `sounds.sound`) are followed through every list element, see [`ids_at_path`].
pub fn referrers(validator: &ProjectValidator, target: RefTarget, id: u64) -> Vec<AssetRef> {
    referrers_with_type(validator, target, id)
        .into_iter()
        .map(|(_, asset)| asset)
        .collect()
}
