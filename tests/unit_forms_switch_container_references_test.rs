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

//! Reference rules, switch-state resolution and `validate_rules` of switch containers.

mod common;

use am::assets::generated::{FadeTransitionSettings, SwitchContainerEntry};
use am::assets::{Asset, AssetType, ProjectContext, ProjectValidator, SwitchContainer};
use am::common::errors::codes::{ERR_VALIDATION_EMPTY_REFERENCE, ERR_VALIDATION_REFERENCE};
use am::forms::references::{RefTarget, referrers, rule_for};
use am::presentation::JsonOutput;
use common::fixtures::AssetTestFixture;
use serde_json::json;

const SOUND: RefTarget = RefTarget::Asset(AssetType::Sound);
const COLLECTION: RefTarget = RefTarget::Asset(AssetType::Collection);
const SWITCH: RefTarget = RefTarget::Asset(AssetType::Switch);

fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.create_test_sound("step", 101).unwrap();
    fx.write_asset_json(
        "collections",
        "crowd",
        json!({"id": 201, "name": "crowd", "bus": 2}),
    )
    .unwrap();
    fx.write_asset_json(
        "switches",
        "surface",
        json!({"id": 10, "name": "surface", "states": [
            {"id": 11, "name": "wood"}, {"id": 12, "name": "stone"}]}),
    )
    .unwrap();
    fx.write_asset_json(
        "switches",
        "weather",
        json!({"id": 20, "name": "weather", "states": [{"id": 21, "name": "rain"}]}),
    )
    .unwrap();
    fx
}

fn validator(fx: &AssetTestFixture) -> ProjectValidator {
    ProjectValidator::new(fx.project_root().to_path_buf(), &JsonOutput::new()).unwrap()
}

fn context(fx: &AssetTestFixture) -> ProjectContext {
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator(fx))
}

fn fade() -> Option<FadeTransitionSettings> {
    Some(FadeTransitionSettings {
        duration: 0.0,
        fader: Some("Linear".to_string()),
    })
}

fn entry(object: u64, states: &[u64]) -> SwitchContainerEntry {
    SwitchContainerEntry {
        object,
        switch_states: states.to_vec(),
        continue_between_states: false,
        fade_in: fade(),
        fade_out: fade(),
        gain: None,
        pitch: None,
    }
}

fn container(entries: Vec<SwitchContainerEntry>) -> SwitchContainer {
    let mut c = SwitchContainer::builder(900, "steps")
        .bus(2)
        .switch_group(10)
        .default_state(11)
        .entries(entries)
        .build();
    c.fader = Some("Linear".to_string());
    c
}

#[test]
fn test_rules_cover_entries_and_default_state() {
    let object = rule_for(AssetType::SwitchContainer, "entries.object").expect("rule");
    assert!(object.required);
    assert!(object.target.includes(SOUND) && object.target.includes(COLLECTION));
    assert!(
        !object
            .target
            .includes(RefTarget::Asset(AssetType::SwitchContainer))
    );
    for field in ["entries.switch_states", "default_switch_state"] {
        let rule = rule_for(AssetType::SwitchContainer, field).expect("rule");
        assert_eq!(rule.target, RefTarget::SwitchState);
        assert!(rule.required);
    }
}

#[test]
fn test_validator_knows_the_states_of_each_switch() {
    let fx = fixture("sc_states");
    let validator = validator(&fx);
    let names: Vec<String> = validator
        .switch_states(10)
        .unwrap()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(names, vec!["wood", "stone"]);
    assert!(validator.switch_states(999).is_none());
    assert_eq!(
        validator.resolve(RefTarget::SwitchState, 21).unwrap().name,
        "rain"
    );
    assert!(validator.validate_switch_state_exists(10, 11).is_ok());
    assert!(validator.validate_switch_state_exists(10, 21).is_err());
}

#[test]
fn test_valid_container_passes() {
    let fx = fixture("sc_valid");
    let c = container(vec![entry(101, &[11]), entry(201, &[11, 12])]);
    c.validate_rules(&context(&fx)).unwrap();
}

#[test]
fn test_zero_bus_is_an_empty_reference() {
    let fx = fixture("sc_bus0");
    let mut c = container(vec![]);
    c.bus = 0;
    let err = c.validate_rules(&context(&fx)).unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_EMPTY_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("bus"));
}

#[test]
fn test_dangling_entry_object_names_the_entry() {
    let fx = fixture("sc_obj");
    let c = container(vec![entry(101, &[11]), entry(999, &[11])]);
    let err = c.validate_rules(&context(&fx)).unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("entries[1].object"));
}

#[test]
fn test_zero_entry_object_is_rejected() {
    let fx = fixture("sc_obj0");
    let err = container(vec![entry(0, &[11])])
        .validate_rules(&context(&fx))
        .unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_EMPTY_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("entries[0].object"));
}

#[test]
fn test_a_state_of_another_switch_is_a_dangling_state() {
    let fx = fixture("sc_state_other");
    let err = container(vec![entry(101, &[11, 21])])
        .validate_rules(&context(&fx))
        .unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("entries[0].switch_states[1]"));
    let text = format!("{} {}", err.why(), err.suggestion());
    assert!(text.contains("wood") && text.contains("stone"), "{text}");
}

#[test]
fn test_zero_and_unknown_states_are_rejected() {
    let fx = fixture("sc_state_bad");
    let ctx = context(&fx);
    let err = container(vec![entry(101, &[0])])
        .validate_rules(&ctx)
        .unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_EMPTY_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("entries[0].switch_states[0]"));
    let err = container(vec![entry(101, &[555])])
        .validate_rules(&ctx)
        .unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_REFERENCE);
}

#[test]
fn test_default_state_must_belong_to_the_switch_group() {
    let fx = fixture("sc_default");
    let ctx = context(&fx);
    let mut c = container(vec![]);
    c.default_switch_state = 0;
    let err = c.validate_rules(&ctx).unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_EMPTY_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("default_switch_state"));
    c.default_switch_state = 21;
    let err = c.validate_rules(&ctx).unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("default_switch_state"));
}

#[test]
fn test_sdk_fatal_fader_settings_are_rejected() {
    let fx = fixture("sc_fader");
    let ctx = context(&fx);
    let mut c = container(vec![]);
    c.fader = None;
    assert_eq!(
        c.validate_rules(&ctx).unwrap_err().field.as_deref(),
        Some("fader")
    );
    c.fader = Some("Wobbly".to_string());
    assert_eq!(
        c.validate_rules(&ctx).unwrap_err().field.as_deref(),
        Some("fader")
    );

    let mut e = entry(101, &[11]);
    e.fade_in = None;
    let err = container(vec![e]).validate_rules(&ctx).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("entries[0].fade_in"));
    let mut e = entry(101, &[11]);
    e.fade_out = Some(FadeTransitionSettings {
        duration: 1.0,
        fader: Some("Wobbly".to_string()),
    });
    let err = container(vec![e]).validate_rules(&ctx).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("entries[0].fade_out.fader"));
}

#[test]
fn test_referrers_cover_switch_sound_collection_and_state() {
    let fx = fixture("sc_referrers");
    fx.write_asset_json(
        "switch_containers",
        "steps",
        json!({
            "id": 900, "name": "steps", "bus": 2, "switch_group": 10, "default_switch_state": 11,
            "entries": [
                {"object": 101, "switch_states": [11]},
                {"object": 201, "switch_states": [12]}
            ]
        }),
    )
    .unwrap();
    let validator = validator(&fx);
    for (target, id) in [
        (SWITCH, 10),
        (SOUND, 101),
        (COLLECTION, 201),
        (RefTarget::SwitchState, 12),
        (RefTarget::SwitchState, 11),
    ] {
        let names: Vec<String> = referrers(&validator, target, id)
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["steps"], "{target:?} {id}");
    }
    assert!(referrers(&validator, SWITCH, 20).is_empty());
}
