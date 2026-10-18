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

//! Tests for `event update` with the full event field surface.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::assets::generated::{EventActionRunningMode, EventActionType, Scope};
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::event_helpers::*;
use common::fixtures::CaptureOutput;
use common::scripted_input::ScriptedInput;
use serde_json::json;

#[tokio::test]
async fn test_update_run_mode_changes_only_the_run_mode() {
    let fx = fixture("ev_up_mode");
    rich_event(&fx).await;
    let before = read_event(&fx, "ev");
    let (result, out) = run_with(
        &fx,
        &["update", "ev", "--run-mode", "parallel"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    let after = read_event(&fx, "ev");

    let mut expected = before.clone();
    expected.run_mode = EventActionRunningMode::Parallel;
    assert_eq!(after, expected);
    assert_eq!(
        out.last_success().unwrap()["updated_fields"],
        json!(["run_mode"])
    );
    assert_flatc(&fx, "ev");
}

#[tokio::test]
async fn test_update_add_and_remove_actions_keep_the_others() {
    let fx = fixture("ev_up_add_remove");
    rich_event(&fx).await;
    let before = read_event(&fx, "ev").actions.unwrap();

    let (result, out) = run_with(
        &fx,
        &[
            "update",
            "ev",
            "--remove-action",
            "1",
            "--add-action",
            "unmute-bus:music_bus,active=false",
            "--add-action",
            "seek:collection:boom,scope=world",
        ],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();

    let after = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(after.len(), 5);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[2]);
    assert_eq!(after[2], before[3]);
    assert_eq!(after[3].type_, EventActionType::UnmuteBus);
    assert_eq!(after[3].targets, Some(vec![3]));
    assert!(!after[3].active);
    assert_eq!(after[4].targets, Some(vec![203]));
    assert_eq!(after[4].scope, Scope::World);
    assert_eq!(
        out.last_success().unwrap()["updated_fields"],
        json!(["actions"])
    );
    assert_flatc(&fx, "ev");
}

#[tokio::test]
async fn test_update_action_flags_replace_all_actions() {
    let fx = fixture("ev_up_replace");
    rich_event(&fx).await;
    let (result, _) = run(
        &fx,
        &[
            "update",
            "ev",
            "--action",
            "pause:step2",
            "--action",
            "wait:5",
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].type_, EventActionType::Pause);
    assert_eq!(actions[1].targets, Some(vec![5]));

    let file = fx.project_root().join("actions.json");
    std::fs::write(
        &file,
        json!([{"type": "Resume", "targets": ["crowd"]}]).to_string(),
    )
    .unwrap();
    let (result, _) = run(
        &fx,
        &[
            "update",
            "ev",
            "--actions-from-json",
            file.to_str().unwrap(),
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].targets, Some(vec![201]));
}

#[tokio::test]
async fn test_update_clear_actions_with_new_ones_replaces_the_list() {
    let fx = fixture("ev_up_clear_add");
    rich_event(&fx).await;
    let (result, _) = run(
        &fx,
        &[
            "update",
            "ev",
            "--clear-actions",
            "--add-action",
            "play:step2",
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].targets, Some(vec![102]));
}

#[tokio::test]
async fn test_update_leaving_no_actions_is_rejected_and_writes_nothing() {
    let fx = fixture("ev_up_empty");
    rich_event(&fx).await;
    let before = std::fs::read_to_string(event_path(&fx, "ev")).unwrap();
    let (result, _) = run(
        &fx,
        &["update", "ev", "--clear-actions"],
        &NonInteractiveInput,
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("no actions"));
    assert_eq!(
        std::fs::read_to_string(event_path(&fx, "ev")).unwrap(),
        before
    );
}

#[tokio::test]
async fn test_update_rejects_bad_edits_and_writes_nothing() {
    let fx = fixture("ev_up_bad");
    rich_event(&fx).await;
    let before = std::fs::read_to_string(event_path(&fx, "ev")).unwrap();

    let cases: [(&[&str], i32); 4] = [
        (
            &["--add-action", "play:999"],
            codes::ERR_VALIDATION_REFERENCE,
        ),
        (
            &["--add-action", "mute-bus:step1"],
            codes::ERR_VALIDATION_REFERENCE,
        ),
        (
            &["--add-action", "stop:0"],
            codes::ERR_VALIDATION_EMPTY_REFERENCE,
        ),
        (&["--remove-action", "9"], codes::ERR_VALIDATION_FIELD),
    ];
    for (flags, code) in cases {
        let mut args = vec!["update", "ev"];
        args.extend_from_slice(flags);
        let (result, _) = run(&fx, &args, &NonInteractiveInput).await;
        let err = result.expect_err(&format!("{flags:?}"));
        assert_eq!(
            as_cli_error(&err).map(|e| e.code),
            Some(code),
            "{flags:?}: {err:#}"
        );
        assert_eq!(
            std::fs::read_to_string(event_path(&fx, "ev")).unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn test_update_conflicting_replace_flags_are_rejected() {
    let fx = fixture("ev_up_conflict");
    rich_event(&fx).await;
    let (result, _) = run(
        &fx,
        &["update", "ev", "--action", "play:step1", "--clear-actions"],
        &NonInteractiveInput,
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--clear-actions"));
}

#[tokio::test]
async fn test_update_preserves_unknown_keys_and_their_order() {
    let fx = fixture("ev_up_unknown");
    rich_event(&fx).await;
    let mut raw = read_raw(&fx, "ev");
    // `_a` sorts before every modelled key; it must still end up after them.
    raw["_a"] = json!("hand written");
    raw["x_extra"] = json!({"a": [1, 2]});
    raw["actions"][0]["x_note"] = json!("keep me");
    std::fs::write(
        event_path(&fx, "ev"),
        serde_json::to_string_pretty(&raw).unwrap(),
    )
    .unwrap();

    let (result, _) = run(
        &fx,
        &["update", "ev", "--run-mode", "parallel"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();

    let after = read_raw(&fx, "ev");
    assert_eq!(after["_a"], "hand written");
    assert_eq!(after["x_extra"], json!({"a": [1, 2]}));
    assert_eq!(
        after["actions"], raw["actions"],
        "untouched actions stay as written"
    );
    assert_eq!(after["run_mode"], "Parallel");
    let keys: Vec<&String> = after.as_object().unwrap().keys().collect();
    let position = |name: &str| keys.iter().position(|k| k.as_str() == name).unwrap();
    assert!(position("actions") < position("_a"), "{keys:?}");
    assert!(position("run_mode") < position("_a"), "{keys:?}");
    assert!(
        position("x_extra") > position("_a"),
        "unknown keys keep their relative order"
    );
    assert_eq!(after["id"], raw["id"]);
    assert_eq!(after["name"], raw["name"]);
}

#[tokio::test]
async fn test_update_editing_actions_still_keeps_unknown_top_level_keys() {
    let fx = fixture("ev_up_unknown_actions");
    rich_event(&fx).await;
    let mut raw = read_raw(&fx, "ev");
    raw["_a"] = json!(1);
    std::fs::write(
        event_path(&fx, "ev"),
        serde_json::to_string_pretty(&raw).unwrap(),
    )
    .unwrap();
    let (result, _) = run(
        &fx,
        &["update", "ev", "--add-action", "play:step2"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let after = read_raw(&fx, "ev");
    assert_eq!(after["_a"], 1);
    assert_eq!(after["run_mode"], raw["run_mode"]);
    assert_eq!(after["actions"].as_array().unwrap().len(), 5);
    // (no flatc check: flatc rejects the hand-written unknown key by design)
}

#[tokio::test]
async fn test_update_without_changes_in_a_non_interactive_session_is_a_no_op() {
    let fx = fixture("ev_up_noop");
    rich_event(&fx).await;
    let before = read_event(&fx, "ev");
    let (result, out) = run_with(
        &fx,
        &["update", "ev"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert_eq!(read_event(&fx, "ev"), before);
    assert_eq!(out.last_success().unwrap()["updated_fields"], json!([]));
}

#[tokio::test]
async fn test_update_json_mode_never_prompts() {
    let fx = fixture("ev_up_json");
    rich_event(&fx).await;
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run_with(&fx, &["update", "ev"], &input, CaptureOutput::json()).await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_update_missing_event_is_not_found() {
    let fx = fixture("ev_up_missing");
    let (result, _) = run(
        &fx,
        &["update", "nope", "--run-mode", "parallel"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_ASSET_NOT_FOUND
    );
}

#[tokio::test]
async fn test_update_bus_target_outside_the_active_bus_file_warns() {
    let fx = fixture("ev_up_inactive_bus");
    rich_event(&fx).await;
    fx.write_buses_file("extra.buses.json", json!([{"id": 9, "name": "ambient"}]))
        .unwrap();
    std::fs::write(
        fx.project_root().join("pc.config.json"),
        json!({"buses_file": "main.buses.json"}).to_string(),
    )
    .unwrap();
    let (result, out) = run_with(
        &fx,
        &["update", "ev", "--add-action", "mute-bus:ambient"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert!(out.all_warnings().iter().any(|w| w.contains("ambient")));
    assert!(
        out.last_success().unwrap()["warnings"][0]
            .as_str()
            .unwrap()
            .contains("ambient")
    );
}
