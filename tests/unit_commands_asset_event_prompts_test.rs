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

//! Tests for the interactive walk-throughs of `event create` and `event update`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::assets::generated::{EventActionRunningMode, EventActionType, Scope};
use am::common::errors::{as_cli_error, codes};
use common::event_helpers::*;
use common::scripted_input::ScriptedInput;

#[tokio::test]
async fn test_interactive_create_walks_actions_and_every_optional_field() {
    let fx = fixture("ev_walk");
    let input = ScriptedInput::new(
        &[
            "Play",                // action 1 type
            "step1 (sound)",       // action 1 target
            "Wait",                // action 2 type
            "Mute bus",            // action 3 type
            "sfx",                 // action 3 target (a bus: no kind suffix)
            "Sequential",          // run mode
            "World",               // action 1 scope
        ],
        &["300"],
    )
    .with_confirms(&[
        false, // another target (action 1)
        true,  // another action
        true,  // another action (after the wait)
        false, // another target (action 3)
        false, // another action
        false, // accept defaults?
        true,  // action 1 active
        false, // action 2 active
        true,  // action 3 active
    ]);

    let (result, _out) = run(&fx, &["create", "ev"], &input).await;
    result.unwrap();
    input.assert_all_consumed();

    for label in [
        "action type",
        "another target",
        "another action",
        "remaining optional fields",
        "run mode",
        "active",
        "scope",
    ] {
        assert!(input.saw_prompt(label), "no prompt mentioned '{label}'");
    }

    let event = read_event(&fx, "ev");
    assert_eq!(event.run_mode, EventActionRunningMode::Sequential);
    let actions = event.actions.unwrap();
    assert_eq!(actions.len(), 3);
    assert_eq!(
        (actions[0].type_, actions[0].targets.clone(), actions[0].scope, actions[0].active),
        (EventActionType::Play, Some(vec![101]), Scope::World, true)
    );
    assert_eq!(
        (actions[1].type_, actions[1].targets.clone(), actions[1].active),
        (EventActionType::Wait, Some(vec![300]), false)
    );
    assert_eq!(actions[2].targets, Some(vec![2]));
    assert_flatc(&fx, "ev");
}

#[tokio::test]
async fn test_interactive_create_accepting_defaults_asks_nothing_more() {
    let fx = fixture("ev_walk_defaults");
    let input = ScriptedInput::new(&["Stop", "crowd (collection)"], &[]).with_confirms(&[
        false, // another target
        false, // another action
        true,  // accept defaults
    ]);
    let (result, _) = run(&fx, &["create", "ev"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("run mode"));
    let event = read_event(&fx, "ev");
    assert_eq!(event.run_mode, EventActionRunningMode::Parallel);
    let action = &event.actions.unwrap()[0];
    assert_eq!((action.targets.clone(), action.active), (Some(vec![201]), true));
}

#[tokio::test]
async fn test_interactive_create_offers_only_the_kinds_the_action_can_target() {
    let fx = fixture("ev_walk_kinds");
    let input = ScriptedInput::new(&["Mute bus", "music_bus"], &[]).with_confirms(&[
        false, false, true,
    ]);
    let (result, _) = run(&fx, &["create", "ev"], &input).await;
    result.unwrap();
    let options = input.options_seen.lock().unwrap();
    let targets = &options[1];
    assert!(targets.iter().all(|o| !o.contains('(')), "{targets:?}");
    assert!(targets.contains(&"master".to_string()));
    assert!(!targets.contains(&"step1".to_string()));
}

#[tokio::test]
async fn test_interactive_create_lists_every_playback_kind_with_its_label() {
    let fx = fixture("ev_walk_labels");
    let input = ScriptedInput::new(&["Play", "music (switch container)"], &[]).with_confirms(&[
        false, false, true,
    ]);
    let (result, _) = run(&fx, &["create", "ev"], &input).await;
    result.unwrap();
    let options = input.options_seen.lock().unwrap();
    for expected in [
        "boom (sound)",
        "boom (collection)",
        "crowd (collection)",
        "music (switch container)",
    ] {
        assert!(options[1].contains(&expected.to_string()), "{:?}", options[1]);
    }
    assert!(!options[1].iter().any(|o| o.starts_with("sfx")));
}

#[tokio::test]
async fn test_interactive_create_keeps_flag_actions_and_asks_only_the_run_mode() {
    let fx = fixture("ev_walk_flags");
    let input = ScriptedInput::new(&["Sequential"], &[]).with_confirms(&[false]);
    let (result, _) = run(&fx, &["create", "ev", "--action", "play:step1"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("action type"));
    assert!(!input.saw_prompt("active"));
    let event = read_event(&fx, "ev");
    assert_eq!(event.run_mode, EventActionRunningMode::Sequential);
    assert_eq!(event.actions.unwrap().len(), 1);
}

#[tokio::test]
async fn test_interactive_create_with_no_buses_available_fails_with_a_suggestion() {
    let fx = fixture("ev_walk_no_bus");
    fx.write_buses_file("main.buses.json", serde_json::json!([])).unwrap();
    let input = ScriptedInput::new(&["Mute bus"], &[]);
    let (result, _) = run(&fx, &["create", "ev"], &input).await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_EMPTY_REFERENCE
    );
    assert!(!event_path(&fx, "ev").exists());
}

#[tokio::test]
async fn test_interactive_update_changes_run_mode_and_adds_an_action() {
    let fx = fixture("ev_up_walk");
    create_ok(&fx, &["--action", "play:step1"]).await;
    let before = read_event(&fx, "ev");
    let input = ScriptedInput::new(
        &[
            "Sequential",     // run mode
            "Add an action",  // menu
            "Unmute bus",     // type
            "music_bus",      // target
            "Done",           // menu
        ],
        &[],
    )
    .with_confirms(&[
        false, // another target
        false, // active?
    ]);
    let (result, _) = run(&fx, &["update", "ev"], &input).await;
    result.unwrap();
    input.assert_all_consumed();

    let after = read_event(&fx, "ev");
    assert_eq!(after.run_mode, EventActionRunningMode::Sequential);
    let actions = after.actions.unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0], before.actions.unwrap()[0]);
    assert_eq!(
        (actions[1].type_, actions[1].targets.clone(), actions[1].active),
        (EventActionType::UnmuteBus, Some(vec![3]), false)
    );
}

#[tokio::test]
async fn test_interactive_update_edits_and_removes_actions() {
    let fx = fixture("ev_up_walk_edit");
    create_ok(
        &fx,
        &["--action", "play:step1", "--action", "stop:step2", "--action", "wait:5"],
    )
    .await;
    let input = ScriptedInput::new(
        &[
            "Parallel",         // run mode: unchanged
            "Edit an action",   // menu
            "0: Play -> step1", // which action
            "Play",             // type: unchanged
            "step2 (sound)",    // new target
            "Entity",           // scope: unchanged
            "Remove an action", // menu
            "2: Wait -> 5 ms",  // which action
            "Done",
        ],
        &[],
    )
    .with_confirms(&[
        true,  // change the targets?
        false, // another target
        true,  // active: unchanged
    ]);
    let (result, _out) = run_with(
        &fx,
        &["update", "ev"],
        &input,
        common::fixtures::CaptureOutput::interactive(),
    )
    .await;
    result.unwrap();
    input.assert_all_consumed();

    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].targets, Some(vec![102]));
    assert_eq!(actions[1].type_, EventActionType::Stop);
}
