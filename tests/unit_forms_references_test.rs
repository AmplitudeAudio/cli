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

mod common;

use am::assets::AssetType;
use am::config::sdk::discover_sdk;
use am::forms::references::{RefTarget, referrers, rule_for, rules_for};
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

#[test]
fn test_referrers_finds_assets_pointing_at_the_target() {
    let fx = common::fixtures::AssetTestFixture::new("referrers").unwrap();
    fx.write_asset_json(
        "attenuators",
        "short",
        serde_json::json!({"id": 55, "name": "short"}),
    )
    .unwrap();
    fx.write_asset_json(
        "sounds",
        "uses",
        serde_json::json!({"id": 1, "name": "uses", "bus": 2, "attenuation": 55}),
    )
    .unwrap();
    fx.write_asset_json(
        "sounds",
        "other",
        serde_json::json!({"id": 2, "name": "other", "bus": 2, "attenuation": 56}),
    )
    .unwrap();
    fx.write_asset_json(
        "sounds",
        "unset",
        serde_json::json!({"id": 3, "name": "unset", "bus": 2}),
    )
    .unwrap();
    let validator = fx.create_project_validator().unwrap();

    let found = referrers(&validator, RefTarget::Asset(AssetType::Attenuation), 55);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "uses");
    assert_eq!(found[0].id, 1);

    assert!(referrers(&validator, RefTarget::Asset(AssetType::Attenuation), 99).is_empty());
    // Zero means "unset" and is never a reference to anything.
    assert!(referrers(&validator, RefTarget::Asset(AssetType::Attenuation), 0).is_empty());
    // Buses are matched through the same rules.
    assert_eq!(referrers(&validator, RefTarget::Bus, 2).len(), 3);
}

fn rtpc_binding(id: u64) -> serde_json::Value {
    serde_json::json!({"kind": "RTPC", "rtpc": {"id": id}})
}

#[test]
fn test_referrers_finds_rtpc_bound_inside_an_rtpc_compatible_value() {
    let fx = common::fixtures::AssetTestFixture::new("referrers_rtpc").unwrap();
    fx.write_asset_json(
        "sounds",
        "bound",
        serde_json::json!({"id": 1, "name": "bound", "bus": 2, "gain": rtpc_binding(77)}),
    )
    .unwrap();
    fx.write_asset_json(
        "sounds",
        "other",
        serde_json::json!({"id": 2, "name": "other", "bus": 2, "gain": rtpc_binding(78)}),
    )
    .unwrap();
    fx.write_asset_json(
        "sounds",
        "static_value",
        serde_json::json!({"id": 3, "name": "static_value", "bus": 2, "gain": {"kind": "Static", "value": 1.0}}),
    )
    .unwrap();
    fx.write_asset_json(
        "collections",
        "pitched",
        serde_json::json!({"id": 4, "name": "pitched", "bus": 2, "pitch": rtpc_binding(77)}),
    )
    .unwrap();
    let validator = fx.create_project_validator().unwrap();

    let mut names: Vec<String> = referrers(&validator, RefTarget::Asset(AssetType::Rtpc), 77)
        .into_iter()
        .map(|r| r.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["bound".to_string(), "pitched".to_string()]);
    assert!(referrers(&validator, RefTarget::Asset(AssetType::Rtpc), 99).is_empty());
}

#[test]
fn test_rtpc_value_field_table_matches_the_schema() {
    use am::forms::describe::describe;
    use am::forms::field::FieldKind;
    use am::forms::references::RTPC_VALUE_FIELDS;

    let sdk = discover_sdk().expect("AM_SDK_PATH must point at an SDK");
    let registry = load_schemas(&sdk, &JsonOutput::new()).expect("schemas must load");
    for t in ALL_TYPES {
        let mut from_schema: Vec<String> = describe(&registry, t)
            .unwrap()
            .into_iter()
            .filter(|f| f.kind == FieldKind::RtpcValue)
            .map(|f| f.name)
            .collect();
        let mut from_table: Vec<String> = RTPC_VALUE_FIELDS
            .iter()
            .filter(|(asset_type, _)| *asset_type == t)
            .map(|(_, field)| field.to_string())
            .collect();
        from_schema.sort();
        from_table.sort();
        assert_eq!(from_schema, from_table, "{t:?}");
    }
}

#[test]
fn test_collection_entry_sound_is_a_required_sound_reference() {
    let rule = rule_for(AssetType::Collection, "sounds.sound").expect("collection entry rule");
    assert_eq!(rule.target, RefTarget::Asset(AssetType::Sound));
    assert!(rule.required);
}

#[test]
fn test_referrers_follow_dotted_rules_through_lists() {
    let fx = common::fixtures::AssetTestFixture::new("referrers_nested").unwrap();
    fx.create_test_sound("step", 101).unwrap();
    fx.write_asset_json(
        "collections",
        "steps",
        serde_json::json!({
            "id": 5, "name": "steps", "bus": 2,
            "sounds_type": ["Random", "Random"],
            "sounds": [{"sound": 101, "weight": 1.0}, {"sound": 102, "weight": 1.0}]
        }),
    )
    .unwrap();
    fx.write_asset_json(
        "collections",
        "other",
        serde_json::json!({
            "id": 6, "name": "other", "bus": 2,
            "sounds_type": ["Random"], "sounds": [{"sound": 103, "weight": 1.0}]
        }),
    )
    .unwrap();
    let validator = fx.create_project_validator().unwrap();

    let sound = RefTarget::Asset(AssetType::Sound);
    let names = |id| -> Vec<String> {
        referrers(&validator, sound, id)
            .into_iter()
            .map(|r| r.name)
            .collect()
    };
    assert_eq!(names(101), vec!["steps".to_string()]);
    assert_eq!(names(102), vec!["steps".to_string()]);
    assert_eq!(names(103), vec!["other".to_string()]);
    assert!(names(999).is_empty());
}

#[test]
fn test_referrers_find_rtpc_bound_in_a_collection_entry() {
    let fx = common::fixtures::AssetTestFixture::new("referrers_nested_rtpc").unwrap();
    fx.write_asset_json(
        "collections",
        "steps",
        serde_json::json!({
            "id": 5, "name": "steps", "bus": 2,
            "sounds_type": ["Random"],
            "sounds": [{"sound": 101, "pitch": rtpc_binding(77)}]
        }),
    )
    .unwrap();
    let validator = fx.create_project_validator().unwrap();
    let found = referrers(&validator, RefTarget::Asset(AssetType::Rtpc), 77);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "steps");
    assert!(referrers(&validator, RefTarget::Asset(AssetType::Rtpc), 78).is_empty());
}

#[test]
fn test_ids_at_path_walks_lists() {
    use am::forms::references::ids_at_path;
    let value = serde_json::json!({"a": [{"b": 1}, {"b": 2}, {"c": 3}], "d": 4});
    assert_eq!(ids_at_path(&value, "a.b"), vec![1, 2]);
    assert_eq!(ids_at_path(&value, "d"), vec![4]);
    assert!(ids_at_path(&value, "a.x").is_empty());
    assert!(ids_at_path(&value, "zzz.b").is_empty());
}
