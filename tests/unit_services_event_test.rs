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

//! Tests for the event service (value building, patching) and the action target rules.

mod common;

use am::assets::generated::{
    EventActionDefinition, EventActionRunningMode, EventActionType, Scope,
};
use am::assets::{Asset, Event, ProjectContext};
use am::common::errors::codes;
use am::services::event::{ActionsPatch, EventPatch, EventValues, apply_patch, build_event};
use common::event_helpers::fixture;
use common::fixtures::AssetTestFixture;

fn context(fx: &AssetTestFixture) -> ProjectContext {
    let validator = fx.create_project_validator().unwrap();
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator)
}

fn action(kind: EventActionType, targets: &[u64]) -> EventActionDefinition {
    EventActionDefinition {
        type_: kind,
        active: true,
        scope: Scope::Entity,
        targets: Some(targets.to_vec()),
    }
}

fn patch_with(actions: Vec<EventActionDefinition>) -> EventPatch {
    EventPatch {
        actions: ActionsPatch {
            replace: Some(actions),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn build(fx: &AssetTestFixture, patch: &EventPatch) -> Result<Event, am::assets::ValidationError> {
    build_event(EventValues::from_patch("ev", patch)?, &context(fx))
}

#[test]
fn test_build_defaults_to_parallel_and_generates_an_id() {
    let fx = fixture("svc_ev_default");
    let event = build(&fx, &patch_with(vec![action(EventActionType::Play, &[101])])).unwrap();
    assert_ne!(event.id, 0);
    assert_eq!(event.name(), "ev");
    assert_eq!(event.run_mode, EventActionRunningMode::Parallel);
    assert_eq!(event.actions.as_ref().unwrap().len(), 1);
}

#[test]
fn test_build_without_actions_is_rejected() {
    let fx = fixture("svc_ev_none");
    let err = build(&fx, &EventPatch::default()).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("actions"));
}

#[test]
fn test_build_keeps_action_settings() {
    let fx = fixture("svc_ev_settings");
    let mut play = action(EventActionType::Play, &[101, 201]);
    play.active = false;
    play.scope = Scope::World;
    let patch = EventPatch {
        run_mode: Some(EventActionRunningMode::Sequential),
        ..patch_with(vec![play.clone(), action(EventActionType::Wait, &[500])])
    };
    let event = build(&fx, &patch).unwrap();
    assert_eq!(event.run_mode, EventActionRunningMode::Sequential);
    assert_eq!(event.actions.unwrap()[0], play);
}

#[test]
fn test_every_action_type_accepts_its_target_kinds() {
    let fx = fixture("svc_ev_kinds");
    for kind in [
        EventActionType::Play,
        EventActionType::Pause,
        EventActionType::Resume,
        EventActionType::Stop,
        EventActionType::Seek,
    ] {
        // sound, collection and switch container are all valid
        build(&fx, &patch_with(vec![action(kind, &[101, 201, 301])])).unwrap();
    }
    for kind in [EventActionType::MuteBus, EventActionType::UnmuteBus] {
        build(&fx, &patch_with(vec![action(kind, &[2])])).unwrap();
    }
    build(&fx, &patch_with(vec![action(EventActionType::Wait, &[0])])).unwrap();
    build(&fx, &patch_with(vec![action(EventActionType::None, &[])])).unwrap();
}

#[test]
fn test_bus_target_on_a_playback_action_is_rejected_with_its_path() {
    let fx = fixture("svc_ev_bus_play");
    let err = build(
        &fx,
        &patch_with(vec![
            action(EventActionType::Play, &[101]),
            action(EventActionType::Play, &[101, 2]),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("actions[1].targets[1]"));
    assert!(err.why().contains("sound, collection or switch container"));
}

#[test]
fn test_sound_target_on_a_bus_action_is_rejected() {
    let fx = fixture("svc_ev_sound_mute");
    let err = build(&fx, &patch_with(vec![action(EventActionType::MuteBus, &[101])])).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("actions[0].targets[0]"));
    assert!(err.why().contains("bus"));
}

#[test]
fn test_dangling_and_zero_targets_are_rejected() {
    let fx = fixture("svc_ev_dangling");
    let dangling = build(&fx, &patch_with(vec![action(EventActionType::Stop, &[999])])).unwrap_err();
    assert_eq!(dangling.code(), codes::ERR_VALIDATION_REFERENCE);
    assert_eq!(dangling.field.as_deref(), Some("actions[0].targets[0]"));

    let zero = build(&fx, &patch_with(vec![action(EventActionType::Play, &[0])])).unwrap_err();
    assert_eq!(zero.code(), codes::ERR_VALIDATION_EMPTY_REFERENCE);
    assert_eq!(zero.field.as_deref(), Some("actions[0].targets[0]"));

    // Zero is rejected even when the context has no validator to resolve ids with.
    let no_validator = ProjectContext::new(fx.project_root().to_path_buf());
    let values =
        EventValues::from_patch("ev", &patch_with(vec![action(EventActionType::Play, &[0])]))
            .unwrap();
    let zero = build_event(values, &no_validator).unwrap_err();
    assert_eq!(zero.code(), codes::ERR_VALIDATION_EMPTY_REFERENCE);

    let dangling_bus = build(
        &fx,
        &patch_with(vec![action(EventActionType::UnmuteBus, &[999])]),
    )
    .unwrap_err();
    assert_eq!(dangling_bus.code(), codes::ERR_VALIDATION_REFERENCE);
}

#[test]
fn test_targets_are_required_except_for_none() {
    let fx = fixture("svc_ev_empty_targets");
    for kind in [
        EventActionType::Play,
        EventActionType::MuteBus,
        EventActionType::Wait,
    ] {
        let err = build(&fx, &patch_with(vec![action(kind, &[])])).unwrap_err();
        assert_eq!(err.field.as_deref(), Some("actions[0].targets"), "{kind:?}");
        assert!(err.what().contains("no targets"));
    }
}

#[test]
fn test_wait_targets_are_milliseconds_not_references() {
    let fx = fixture("svc_ev_wait");
    // 987654 is no asset: for Wait it is just a duration.
    build(&fx, &patch_with(vec![action(EventActionType::Wait, &[987654])])).unwrap();
}

#[test]
fn test_apply_patch_run_mode_only_keeps_the_actions() {
    let fx = fixture("svc_ev_runmode");
    let before = build(&fx, &patch_with(vec![action(EventActionType::Play, &[101])])).unwrap();
    let patch = EventPatch {
        run_mode: Some(EventActionRunningMode::Sequential),
        ..Default::default()
    };
    let after = apply_patch(&before, &patch).unwrap();
    assert_eq!(after.run_mode, EventActionRunningMode::Sequential);
    assert_eq!(after.actions, before.actions);
    assert_eq!((after.id, &after.name), (before.id, &before.name));
}

#[test]
fn test_apply_patch_replace_remove_add_in_order() {
    let fx = fixture("svc_ev_actions");
    let before = build(
        &fx,
        &patch_with(vec![
            action(EventActionType::Play, &[101]),
            action(EventActionType::Stop, &[102]),
            action(EventActionType::Wait, &[5]),
        ]),
    )
    .unwrap();

    let removed = apply_patch(
        &before,
        &EventPatch {
            actions: ActionsPatch {
                remove: vec![1, 0],
                add: vec![action(EventActionType::Pause, &[103])],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let kinds: Vec<_> = removed.actions.unwrap().iter().map(|a| a.type_).collect();
    assert_eq!(kinds, vec![EventActionType::Wait, EventActionType::Pause]);

    let replaced = apply_patch(
        &before,
        &patch_with(vec![action(EventActionType::Resume, &[201])]),
    )
    .unwrap();
    assert_eq!(replaced.actions.unwrap().len(), 1);

    let cleared = apply_patch(&before, &patch_with(vec![])).unwrap();
    assert!(cleared.actions.unwrap().is_empty());
}

#[test]
fn test_apply_patch_remove_out_of_range_is_an_error() {
    let fx = fixture("svc_ev_range");
    let before = build(&fx, &patch_with(vec![action(EventActionType::Play, &[101])])).unwrap();
    let err = apply_patch(
        &before,
        &EventPatch {
            actions: ActionsPatch {
                remove: vec![3],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.what().contains('3'));
    assert!(err.suggestion().contains('0'));
}

#[test]
fn test_changed_fields_lists_what_the_patch_sets() {
    assert!(EventPatch::default().is_empty());
    let patch = EventPatch {
        run_mode: Some(EventActionRunningMode::Sequential),
        actions: ActionsPatch {
            remove: vec![0],
            ..Default::default()
        },
    };
    assert_eq!(patch.changed_fields(), vec!["run_mode", "actions"]);
}
