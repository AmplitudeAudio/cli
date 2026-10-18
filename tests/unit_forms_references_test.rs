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

//! Tests for the asset reference annotation table.

use am::assets::AssetType;
use am::config::sdk::discover_sdk;
use am::forms::references::{RefTarget, rule_for, rules_for};
use am::presentation::JsonOutput;
use am::schema::loader::load_schemas;

const ALL_TYPES: [AssetType; 10] = [
    AssetType::Attenuation,
    AssetType::Rtpc,
    AssetType::Pipeline,
    AssetType::Sound,
    AssetType::Collection,
    AssetType::Switch,
    AssetType::SwitchContainer,
    AssetType::Soundbank,
    AssetType::Event,
    AssetType::Effect,
];

#[test]
fn test_sound_bus_is_required_bus_reference() {
    let rule = rule_for(AssetType::Sound, "bus").expect("sound.bus rule");
    assert_eq!(rule.target, RefTarget::Bus);
    assert!(rule.required);
}

#[test]
fn test_sound_attenuation_and_effect_are_optional() {
    let att = rule_for(AssetType::Sound, "attenuation").expect("sound.attenuation rule");
    assert_eq!(att.target, RefTarget::Asset(AssetType::Attenuation));
    assert!(!att.required);
    let fx = rule_for(AssetType::Sound, "effect").expect("sound.effect rule");
    assert_eq!(fx.target, RefTarget::Asset(AssetType::Effect));
    assert!(!fx.required);
}

#[test]
fn test_non_reference_field_has_no_rule() {
    assert!(rule_for(AssetType::Sound, "gain").is_none());
}

#[test]
fn test_switch_container_references() {
    let sg = rule_for(AssetType::SwitchContainer, "switch_group").expect("rule");
    assert_eq!(sg.target, RefTarget::Asset(AssetType::Switch));
    assert!(sg.required);
    assert!(
        rule_for(AssetType::SwitchContainer, "bus")
            .expect("rule")
            .required
    );
}

#[test]
fn test_rules_for_covers_every_asset_type_with_matching_owner() {
    for t in ALL_TYPES {
        for rule in rules_for(t) {
            assert_eq!(rule.asset_type, t);
        }
    }
    assert!(rules_for(AssetType::Switch).is_empty());
    assert!(!rules_for(AssetType::Collection).is_empty());
}

#[test]
fn test_every_rule_field_exists_in_schema() {
    let sdk = discover_sdk().expect("AM_SDK_PATH must point at an SDK");
    let registry = load_schemas(&sdk, &JsonOutput::new()).expect("schemas must load");
    for t in ALL_TYPES {
        let schema = registry
            .get(t)
            .unwrap_or_else(|| panic!("no schema for {t:?}"));
        for rule in rules_for(t) {
            // Nested rules use dotted paths; the registry only describes
            // top-level fields, so check the first segment.
            let top = rule.field.split('.').next().unwrap();
            assert!(
                schema.has_field(top),
                "{t:?} rule field '{}' not in schema",
                rule.field
            );
        }
    }
}
