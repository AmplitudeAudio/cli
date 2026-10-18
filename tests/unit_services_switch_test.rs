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

//! Tests for the switch service (value building, state patching).

mod common;

use am::assets::generated::SwitchStateDefinition;
use am::assets::{ProjectContext, Switch, ValidationError};
use am::common::errors::codes;
use am::services::switch::{
    StateSpec, StatesPatch, SwitchPatch, SwitchValues, apply_patch, apply_states, build_switch,
    default_state_id, removed_state_ids,
};
use common::fixtures::AssetTestFixture;

fn context(fx: &AssetTestFixture) -> ProjectContext {
    let validator = fx.create_project_validator().unwrap();
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator)
}

fn spec(name: &str, id: Option<u64>) -> StateSpec {
    StateSpec {
        name: name.to_string(),
        id,
    }
}

fn state(id: u64, name: &str) -> SwitchStateDefinition {
    SwitchStateDefinition {
        id,
        name: Some(name.to_string()),
    }
}

fn existing() -> Vec<SwitchStateDefinition> {
    vec![state(11, "wood"), state(12, "stone"), state(13, "metal")]
}

fn replace(specs: Vec<StateSpec>) -> StatesPatch {
    StatesPatch {
        replace: Some(specs),
        ..Default::default()
    }
}

fn build(fx: &AssetTestFixture, patch: &SwitchPatch) -> Result<Switch, ValidationError> {
    build_switch(SwitchValues::from_patch("sw", patch)?, &context(fx))
}

fn names(states: &[SwitchStateDefinition]) -> Vec<&str> {
    states.iter().map(|s| s.name.as_deref().unwrap()).collect()
}

#[test]
fn test_build_assigns_distinct_ids_and_keeps_explicit_ones() {
    let fx = AssetTestFixture::new("svc_sw_build").unwrap();
    let patch = SwitchPatch {
        states: replace(vec![
            spec("wood", Some(5)),
            spec("stone", None),
            spec("metal", None),
        ]),
    };
    let switch = build(&fx, &patch).unwrap();
    assert_ne!(switch.id, 0);
    let states = switch.states.unwrap();
    assert_eq!(states[0].id, 5);
    let mut ids: Vec<u64> = states.iter().map(|s| s.id).collect();
    assert!(ids.iter().all(|id| *id != 0));
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 3);
}

#[test]
fn test_build_without_states_names_the_flags() {
    let err = SwitchValues::from_patch("sw", &SwitchPatch::default()).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_FIELD);
    assert_eq!(err.field.as_deref(), Some("states"));
    let text = format!("{} {}", err.inner.what, err.inner.why);
    assert!(
        text.contains("--states") || text.contains("--state"),
        "{text}"
    );
}

#[test]
fn test_build_rejects_duplicate_names_zero_and_duplicate_ids() {
    let fx = AssetTestFixture::new("svc_sw_invalid").unwrap();
    for specs in [
        vec![spec("wood", None), spec("wood", None)],
        vec![spec("wood", Some(0))],
        vec![spec("wood", Some(4)), spec("stone", Some(4))],
        vec![spec("   ", None)],
    ] {
        let err = build(
            &fx,
            &SwitchPatch {
                states: replace(specs.clone()),
            },
        )
        .unwrap_err();
        assert_eq!(err.field.as_deref(), Some("states"), "{specs:?}");
    }
}

#[test]
fn test_replace_keeps_the_ids_of_states_that_keep_their_name() {
    let out = apply_states(
        "sw",
        &existing(),
        &replace(vec![
            spec("metal", None),
            spec("wood", None),
            spec("grass", None),
        ]),
    )
    .unwrap();
    assert_eq!(names(&out), vec!["metal", "wood", "grass"]);
    assert_eq!(out[0].id, 13);
    assert_eq!(out[1].id, 11);
    assert!(![11, 12, 13, 0].contains(&out[2].id));
}

#[test]
fn test_replace_with_an_explicit_id_overrides_the_existing_one() {
    let out = apply_states("sw", &existing(), &replace(vec![spec("wood", Some(99))])).unwrap();
    assert_eq!(out[0].id, 99);
}

#[test]
fn test_rename_keeps_the_id_and_resolves_names_or_ids() {
    let patch = StatesPatch {
        rename: vec![
            ("wood".to_string(), "timber".to_string()),
            ("12".to_string(), "rock".to_string()),
        ],
        ..Default::default()
    };
    let out = apply_states("sw", &existing(), &patch).unwrap();
    assert_eq!(names(&out), vec!["timber", "rock", "metal"]);
    assert_eq!(
        out.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![11, 12, 13]
    );
}

#[test]
fn test_rename_unknown_state_lists_the_states() {
    let patch = StatesPatch {
        rename: vec![("grass".to_string(), "lawn".to_string())],
        ..Default::default()
    };
    let err = apply_states("sw", &existing(), &patch).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_FIELD);
    assert!(
        err.inner.suggestion.contains("wood"),
        "{}",
        err.inner.suggestion
    );
}

#[test]
fn test_remove_by_name_or_id_then_add_appends_with_a_fresh_id() {
    let patch = StatesPatch {
        remove: vec!["wood".to_string(), "13".to_string()],
        add: vec![spec("grass", None), spec("dirt", Some(40))],
        ..Default::default()
    };
    let out = apply_states("sw", &existing(), &patch).unwrap();
    assert_eq!(names(&out), vec!["stone", "grass", "dirt"]);
    assert_eq!(out[0].id, 12);
    assert!(![11, 12, 13, 0, 40].contains(&out[1].id));
    assert_eq!(out[2].id, 40);
}

#[test]
fn test_remove_unknown_state_fails() {
    let patch = StatesPatch {
        remove: vec!["grass".to_string()],
        ..Default::default()
    };
    let err = apply_states("sw", &existing(), &patch).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_FIELD);
}

#[test]
fn test_apply_order_is_replace_rename_remove_add() {
    let patch = StatesPatch {
        replace: Some(vec![spec("a", Some(1)), spec("b", Some(2))]),
        rename: vec![("a".to_string(), "c".to_string())],
        remove: vec!["b".to_string()],
        add: vec![spec("d", Some(4))],
    };
    let out = apply_states("sw", &existing(), &patch).unwrap();
    assert_eq!(names(&out), vec!["c", "d"]);
}

#[test]
fn test_apply_patch_leaves_the_original_and_other_fields_alone() {
    let original = Switch::builder(7, "sw").state(1, "a").state(2, "b").build();
    let patch = SwitchPatch {
        states: StatesPatch {
            add: vec![spec("c", Some(3))],
            ..Default::default()
        },
    };
    let out = apply_patch(&original, &patch).unwrap();
    assert_eq!(out.id, 7);
    assert_eq!(out.states.as_ref().unwrap().len(), 3);
    assert_eq!(original.states.as_ref().unwrap().len(), 2);
}

#[test]
fn test_removed_state_ids_lists_what_a_patch_drops_or_changes() {
    let before = existing();
    let after = vec![state(11, "wood"), state(99, "stone")];
    assert_eq!(removed_state_ids(&before, &after), vec![12, 13]);
    assert!(removed_state_ids(&before, &before).is_empty());
}

#[test]
fn test_default_state_id_avoids_taken_ids_and_zero() {
    let id = default_state_id("sw", "wood", &[11, 12]);
    assert!(![0, 11, 12].contains(&id));
}

#[test]
fn test_patch_reports_the_fields_it_sets() {
    assert!(SwitchPatch::default().is_empty());
    let p = SwitchPatch {
        states: StatesPatch {
            remove: vec!["x".to_string()],
            ..Default::default()
        },
    };
    assert_eq!(p.changed_fields(), vec!["states"]);
}
