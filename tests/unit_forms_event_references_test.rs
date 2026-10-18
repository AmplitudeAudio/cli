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

//! Tests for the any-of reference target and the event `actions.targets` rule.

mod common;

use am::assets::AssetType;
use am::assets::generated::EventActionType;
use am::forms::references::{RefTarget, referrers, rule_for};
use am::forms::validate_refs::validate_reference;
use common::event_helpers::fixture;
use serde_json::json;

const SOUND: RefTarget = RefTarget::Asset(AssetType::Sound);
const COLLECTION: RefTarget = RefTarget::Asset(AssetType::Collection);
const SWITCH_CONTAINER: RefTarget = RefTarget::Asset(AssetType::SwitchContainer);

#[test]
fn test_event_targets_rule_accepts_any_playable_kind_or_a_bus() {
    let rule = rule_for(AssetType::Event, "actions.targets").expect("event targets rule");
    assert!(rule.required);
    assert_eq!(
        rule.target,
        RefTarget::AnyOf(&[SOUND, COLLECTION, SWITCH_CONTAINER, RefTarget::Bus])
    );
}

#[test]
fn test_any_of_members_and_inclusion() {
    let any = RefTarget::AnyOf(&[SOUND, COLLECTION]);
    assert_eq!(any.members(), vec![SOUND, COLLECTION]);
    assert!(any.includes(SOUND));
    assert!(!any.includes(RefTarget::Bus));
    assert!(SOUND.includes(SOUND));
    assert_eq!(SOUND.members(), vec![SOUND]);
}

#[test]
fn test_each_action_type_names_the_kinds_it_targets() {
    for kind in [
        EventActionType::Play,
        EventActionType::Pause,
        EventActionType::Resume,
        EventActionType::Stop,
        EventActionType::Seek,
    ] {
        assert_eq!(
            kind.target_kinds(),
            Some(RefTarget::AnyOf(&[SOUND, COLLECTION, SWITCH_CONTAINER])),
            "{kind:?}"
        );
    }
    for kind in [EventActionType::MuteBus, EventActionType::UnmuteBus] {
        assert_eq!(kind.target_kinds(), Some(RefTarget::Bus), "{kind:?}");
    }
    assert_eq!(EventActionType::Wait.target_kinds(), None);
    assert_eq!(EventActionType::None.target_kinds(), None);
}

#[test]
fn test_validate_reference_resolves_through_any_of() {
    let fx = fixture("ev_any_of");
    let validator = fx.create_project_validator().unwrap();
    let rule = rule_for(AssetType::Event, "actions.targets").unwrap();
    for id in [101, 201, 301, 2] {
        validate_reference(&rule, id, &validator).unwrap_or_else(|e| panic!("{id}: {e:?}"));
    }
    let missing = validate_reference(&rule, 999, &validator).unwrap_err();
    assert_eq!(
        missing.code(),
        am::common::errors::codes::ERR_VALIDATION_REFERENCE
    );
    assert!(missing.what().contains("999"));
    let zero = validate_reference(&rule, 0, &validator).unwrap_err();
    assert_eq!(
        zero.code(),
        am::common::errors::codes::ERR_VALIDATION_EMPTY_REFERENCE
    );
}

#[test]
fn test_validator_lists_the_union_of_any_of_members_sorted() {
    let fx = fixture("ev_any_of_list");
    let validator = fx.create_project_validator().unwrap();
    let names: Vec<String> = validator
        .list(RefTarget::AnyOf(&[SOUND, COLLECTION]))
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(
        names,
        vec!["boom", "boom", "crowd", "step1", "step2"],
        "sorted by name across kinds"
    );
    assert_eq!(
        validator
            .resolve(RefTarget::AnyOf(&[COLLECTION, RefTarget::Bus]), 2)
            .map(|r| r.name),
        Some("sfx".to_string())
    );
    assert!(
        validator
            .resolve(RefTarget::AnyOf(&[COLLECTION]), 101)
            .is_none()
    );
}

#[test]
fn test_referrers_find_events_targeting_a_sound_collection_switch_container_or_bus() {
    let fx = fixture("ev_referrers");
    fx.write_asset_json(
        "events",
        "uses_all",
        json!({
            "id": 900, "name": "uses_all", "run_mode": "Parallel",
            "actions": [
                {"type": "Play", "targets": [101, 201]},
                {"type": "Stop", "targets": [301]},
                {"type": "MuteBus", "targets": [3]},
                {"type": "Wait", "targets": [102]}
            ]
        }),
    )
    .unwrap();
    fx.write_asset_json(
        "events",
        "only_wait",
        json!({
            "id": 901, "name": "only_wait", "run_mode": "Parallel",
            "actions": [{"type": "Wait", "targets": [101]}, {"type": "UnmuteBus", "targets": [2]}]
        }),
    )
    .unwrap();
    let validator = fx.create_project_validator().unwrap();
    let names = |target: RefTarget, id: u64| -> Vec<String> {
        let mut names: Vec<String> = referrers(&validator, target, id)
            .into_iter()
            .map(|r| r.name)
            .collect();
        names.sort();
        names
    };

    assert_eq!(
        names(SOUND, 101),
        vec!["uses_all"],
        "a Wait duration is not a sound reference"
    );
    assert!(names(SOUND, 102).is_empty());
    assert_eq!(names(COLLECTION, 201), vec!["uses_all"]);
    assert_eq!(names(SWITCH_CONTAINER, 301), vec!["uses_all"]);
    assert_eq!(names(RefTarget::Bus, 3), vec!["uses_all"]);
    let bus_2 = names(RefTarget::Bus, 2);
    assert!(bus_2.contains(&"only_wait".to_string()), "{bus_2:?}");
    assert!(!bus_2.contains(&"uses_all".to_string()), "{bus_2:?}");
    assert!(
        names(RefTarget::Bus, 101).is_empty(),
        "a playback target is never a bus reference"
    );
}
