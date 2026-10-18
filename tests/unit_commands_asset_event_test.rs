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

//! Tests for `event create` with the full event field surface.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::assets::generated::{EventActionRunningMode, EventActionType, Scope};
use am::commands::asset::EventCommands;
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::event_helpers::*;
use common::fixtures::CaptureOutput;
use common::scripted_input::ScriptedInput;
use serde_json::json;

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[test]
fn test_parse_create_accepts_every_event_flag() {
    let cmd = parse(&[
        "create",
        "ev",
        "--run-mode",
        "sequential",
        "--action",
        "play:step1,crowd,scope=world,active=false",
        "-a",
        "wait:500",
        "--actions-from-json",
        "actions.json",
    ]);
    match cmd {
        EventCommands::Create {
            run_mode, action, ..
        } => {
            assert_eq!(run_mode.as_deref(), Some("sequential"));
            assert_eq!(action.len(), 2);
        }
        _ => panic!("expected create"),
    }
}

#[test]
fn test_parse_update_accepts_every_event_flag() {
    let cmd = parse(&[
        "update",
        "ev",
        "--run-mode",
        "parallel",
        "--action",
        "stop:step1",
        "--actions-from-json",
        "a.json",
        "--add-action",
        "mute-bus:sfx",
        "--remove-action",
        "0",
        "--remove-action",
        "2",
        "--clear-actions",
    ]);
    match cmd {
        EventCommands::Update {
            remove_action,
            clear_actions,
            add_action,
            ..
        } => {
            assert_eq!(remove_action, vec![0, 2]);
            assert!(clear_actions);
            assert_eq!(add_action, vec!["mute-bus:sfx".to_string()]);
        }
        _ => panic!("expected update"),
    }
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_with_every_action_type_is_accepted_by_flatc_and_round_trips() {
    let fx = fixture("ev_all_types");
    create_ok(
        &fx,
        &[
            "--run-mode",
            "Sequential",
            "--action",
            "play:step1,crowd,music,scope=world",
            "--action",
            "pause:step2,active=false",
            "--action",
            "resume:step2",
            "--action",
            "stop:crowd,scope=entity",
            "--action",
            "seek:step1",
            "--action",
            "mute-bus:sfx,music_bus",
            "--action",
            "unmute-bus:sfx",
            "--action",
            "wait:500",
        ],
    )
    .await;
    assert_flatc(&fx, "ev");

    let event = read_event(&fx, "ev");
    assert_eq!(event.run_mode, EventActionRunningMode::Sequential);
    let actions = event.actions.unwrap();
    let kinds: Vec<_> = actions.iter().map(|a| a.type_).collect();
    assert_eq!(
        kinds,
        vec![
            EventActionType::Play,
            EventActionType::Pause,
            EventActionType::Resume,
            EventActionType::Stop,
            EventActionType::Seek,
            EventActionType::MuteBus,
            EventActionType::UnmuteBus,
            EventActionType::Wait,
        ]
    );
    assert_eq!(actions[0].targets, Some(vec![101, 201, 301]));
    assert_eq!(actions[0].scope, Scope::World);
    assert!(actions[0].active);
    assert!(!actions[1].active);
    assert_eq!(actions[1].scope, Scope::Entity, "scope defaults to Entity");
    assert_eq!(actions[3].scope, Scope::Entity);
    assert_eq!(actions[5].targets, Some(vec![2, 3]));
    assert_eq!(actions[7].targets, Some(vec![500]));

    let raw = read_raw(&fx, "ev");
    assert_eq!(raw["actions"][0]["type"], "Play");
    assert_eq!(raw["actions"][7]["type"], "Wait");
}

#[tokio::test]
async fn test_create_defaults_to_parallel_with_active_entity_actions() {
    let fx = fixture("ev_defaults");
    create_ok(&fx, &["--action", "play:step1"]).await;
    let event = read_event(&fx, "ev");
    assert_eq!(event.run_mode, EventActionRunningMode::Parallel);
    let action = &event.actions.unwrap()[0];
    assert!(action.active);
    assert_eq!(action.scope, Scope::Entity);
    assert_flatc(&fx, "ev");
}

#[tokio::test]
async fn test_create_accepts_ids_and_the_original_action_syntax() {
    let fx = fixture("ev_ids");
    create_ok(&fx, &["-a", "play:101,102", "--action", "stop:201"]).await;
    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions[0].targets, Some(vec![101, 102]));
    assert_eq!(actions[1].targets, Some(vec![201]));
}

#[tokio::test]
async fn test_create_resolves_names_by_the_kind_the_action_needs() {
    let fx = fixture("ev_kinds");
    create_ok(
        &fx,
        &[
            "--action",
            "play:music",
            "--action",
            "mute-bus:music_bus",
            "--action",
            "stop:collection:crowd,sound:step1",
        ],
    )
    .await;
    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions[0].targets, Some(vec![301]));
    assert_eq!(actions[1].targets, Some(vec![3]));
    assert_eq!(actions[2].targets, Some(vec![201, 101]));
}

#[tokio::test]
async fn test_create_name_shared_by_two_kinds_asks_for_a_kind_prefix() {
    let fx = fixture("ev_ambiguous");
    let (result, _) = run(
        &fx,
        &["create", "ev", "--action", "play:boom"],
        &NonInteractiveInput,
    )
    .await;
    let message = format!("{:#}", result.unwrap_err());
    assert!(message.contains("sound:boom"), "{message}");
    assert!(message.contains("collection:boom"), "{message}");
    assert!(!event_path(&fx, "ev").exists());

    create_ok(&fx, &["--action", "play:sound:boom", "--action", "stop:collection:boom"]).await;
    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions[0].targets, Some(vec![103]));
    assert_eq!(actions[1].targets, Some(vec![203]));
}

#[tokio::test]
async fn test_create_wrong_target_kind_for_the_action_type_is_rejected() {
    let fx = fixture("ev_wrong_kind");
    for spec in [
        "mute-bus:step1",
        "unmute-bus:crowd",
        "play:sfx",
        "play:bus:sfx",
        "mute-bus:sound:step1",
        "mute-bus:101",
        "play:2",
    ] {
        let (result, _) = run(&fx, &["create", "ev", "--action", spec], &NonInteractiveInput).await;
        let err = result.expect_err(spec);
        assert_eq!(
            as_cli_error(&err).unwrap().code,
            codes::ERR_VALIDATION_REFERENCE,
            "{spec}: {err:#}"
        );
        assert!(!event_path(&fx, "ev").exists(), "{spec}");
    }
    let (result, _) = run(
        &fx,
        &["create", "ev", "--action", "mute-bus:step1"],
        &NonInteractiveInput,
    )
    .await;
    let message = format!("{:#}", result.unwrap_err());
    assert!(message.contains("sound"), "{message}");
    assert!(message.contains("bus"), "{message}");
}

#[tokio::test]
async fn test_create_dangling_and_zero_targets_are_rejected_before_writing() {
    let fx = fixture("ev_dangling");
    let (dangling, _) = run(
        &fx,
        &["create", "ev", "--action", "play:step1,999"],
        &NonInteractiveInput,
    )
    .await;
    let err = dangling.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert!(format!("{err:#}").contains("actions[0].targets[1]"));

    let (zero, _) = run(
        &fx,
        &["create", "ev", "--action", "stop:0"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&zero.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_EMPTY_REFERENCE
    );

    let (unknown, _) = run(
        &fx,
        &["create", "ev", "--action", "play:nope"],
        &NonInteractiveInput,
    )
    .await;
    let err = unknown.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert!(format!("{err:#}").contains("step1"), "lists what exists");
    assert!(!event_path(&fx, "ev").exists());
}

#[tokio::test]
async fn test_create_wait_takes_milliseconds() {
    let fx = fixture("ev_wait");
    create_ok(&fx, &["--action", "wait:1500", "--action", "wait:0"]).await;
    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions[0].targets, Some(vec![1500]));
    assert_eq!(actions[1].targets, Some(vec![0]));

    for bad in ["wait:soon", "wait:-5", "wait:"] {
        let (result, _) = run(&fx, &["create", "bad", "--action", bad], &NonInteractiveInput).await;
        let err = result.expect_err(bad);
        assert_eq!(
            as_cli_error(&err).unwrap().code,
            codes::ERR_VALIDATION_FIELD,
            "{bad}"
        );
    }
}

#[tokio::test]
async fn test_create_rejects_malformed_action_specs() {
    let fx = fixture("ev_malformed");
    for (spec, needle) in [
        ("jump:step1", "play"),
        ("play", "target"),
        ("play:step1,volume=3", "volume"),
        ("play:step1,scope=sideways", "scope"),
        ("play:step1,active=maybe", "active"),
        ("play:step1,scope=world,scope=entity", "twice"),
        ("play:", "target"),
    ] {
        let (result, _) = run(&fx, &["create", "ev", "--action", spec], &NonInteractiveInput).await;
        let err = result.expect_err(spec);
        let message = format!("{err:#}");
        assert!(message.contains(needle), "{spec}: {message}");
        assert_eq!(
            as_cli_error(&err).unwrap().code,
            codes::ERR_VALIDATION_FIELD,
            "{spec}"
        );
        assert!(!event_path(&fx, "ev").exists(), "{spec}");
    }
}

#[tokio::test]
async fn test_create_invalid_run_mode_is_rejected() {
    let fx = fixture("ev_run_mode");
    let (result, _) = run(
        &fx,
        &["create", "ev", "--run-mode", "random", "--action", "play:step1"],
        &NonInteractiveInput,
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("parallel, sequential"));
}

#[tokio::test]
async fn test_create_missing_action_names_the_flag_and_writes_nothing() {
    let fx = fixture("ev_no_action");
    let (result, _) = run(&fx, &["create", "ev"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("--action"));
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_FIELD
    );
    assert!(!event_path(&fx, "ev").exists());
}

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = fixture("ev_json_no_prompt");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run_with(&fx, &["create", "ev"], &input, CaptureOutput::json()).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--action"));
    assert!(input.prompts.lock().unwrap().is_empty());
    assert!(!event_path(&fx, "ev").exists());

    // Unresolvable flag values fail instead of falling back to a picker.
    let (result, _) = run_with(
        &fx,
        &["create", "ev", "--action", "play:nope"],
        &input,
        CaptureOutput::json(),
    )
    .await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_create_existing_name_is_rejected() {
    let fx = fixture("ev_exists");
    create_ok(&fx, &["--action", "play:step1"]).await;
    let (result, _) = run(
        &fx,
        &["create", "ev", "--action", "play:step1"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_ASSET_ALREADY_EXISTS
    );
}

#[tokio::test]
async fn test_create_json_output_reports_the_event() {
    let fx = fixture("ev_json_out");
    let (result, out) = run_with(
        &fx,
        &["create", "ev", "--action", "play:step1", "--action", "wait:10"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    let data = out.last_success().unwrap();
    assert_eq!(data["name"], "ev");
    assert_eq!(data["action_count"], 2);
    assert_eq!(data["run_mode"], "Parallel");
    assert!(data["id"].as_u64().unwrap() > 0);
}

// ---------------------------------------------------------------------------
// --actions-from-json
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_from_json_resolves_names_ids_and_settings() {
    let fx = fixture("ev_from_json");
    let file = fx.project_root().join("actions.json");
    std::fs::write(
        &file,
        json!([
            {"type": "Play", "targets": ["step1", 102, "collection:boom"], "scope": "World"},
            {"type": "MuteBus", "targets": ["sfx"], "active": false},
            {"type": "Wait", "targets": [250]}
        ])
        .to_string(),
    )
    .unwrap();
    create_ok(&fx, &["--actions-from-json", file.to_str().unwrap()]).await;
    let actions = read_event(&fx, "ev").actions.unwrap();
    assert_eq!(actions[0].targets, Some(vec![101, 102, 203]));
    assert_eq!(actions[0].scope, Scope::World);
    assert!(!actions[1].active);
    assert_eq!(actions[2].targets, Some(vec![250]));
    assert_flatc(&fx, "ev");
}

#[tokio::test]
async fn test_create_from_json_accepts_the_sdk_event_shape() {
    let fx = fixture("ev_from_sdk_json");
    let file = fx.project_root().join("actions.json");
    std::fs::write(
        &file,
        json!({"id": 1, "name": "x", "actions": [
            {"type": "Stop", "active": true, "scope": "World", "targets": [101]}
        ]})
        .to_string(),
    )
    .unwrap();
    create_ok(&fx, &["--actions-from-json", file.to_str().unwrap()]).await;
    assert_eq!(
        read_event(&fx, "ev").actions.unwrap()[0].type_,
        EventActionType::Stop
    );
}

#[tokio::test]
async fn test_create_from_json_rejects_bad_files() {
    let fx = fixture("ev_bad_json");
    let file = fx.project_root().join("actions.json");
    for (body, needle) in [
        ("not json", "JSON"),
        (r#"[{"targets": [101]}]"#, "type"),
        (r#"[{"type": "Play", "targets": [101], "volume": 1}]"#, "volume"),
        (r#"[{"type": "Jump", "targets": [101]}]"#, "Jump"),
        (r#"[{"type": "Play", "targets": [true]}]"#, "target"),
        (r#"[{"type": "Play", "targets": ["nope"]}]"#, "nope"),
        (r#"[{"type": "Play", "targets": [999]}]"#, "999"),
    ] {
        std::fs::write(&file, body).unwrap();
        let (result, _) = run(
            &fx,
            &["create", "ev", "--actions-from-json", file.to_str().unwrap()],
            &NonInteractiveInput,
        )
        .await;
        let message = format!("{:#}", result.expect_err(body));
        assert!(message.contains(needle), "{body}: {message}");
        assert!(!event_path(&fx, "ev").exists(), "{body}");
    }
    let (missing, _) = run(
        &fx,
        &["create", "ev", "--actions-from-json", "/nonexistent/a.json"],
        &NonInteractiveInput,
    )
    .await;
    assert!(missing.is_err());
}

#[tokio::test]
async fn test_action_and_actions_from_json_conflict() {
    let fx = fixture("ev_conflict");
    let file = fx.project_root().join("actions.json");
    std::fs::write(&file, "[]").unwrap();
    let (result, _) = run(
        &fx,
        &[
            "create",
            "ev",
            "--action",
            "play:step1",
            "--actions-from-json",
            file.to_str().unwrap(),
        ],
        &NonInteractiveInput,
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--actions-from-json"));
    assert!(!event_path(&fx, "ev").exists());
}

// ---------------------------------------------------------------------------
// Buses outside the active bus file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_bus_target_outside_the_active_bus_file_warns() {
    let fx = fixture("ev_inactive_bus");
    fx.write_buses_file("extra.buses.json", json!([{"id": 9, "name": "ambient"}]))
        .unwrap();
    std::fs::write(
        fx.project_root().join("pc.config.json"),
        json!({"buses_file": "main.buses.json"}).to_string(),
    )
    .unwrap();

    let (result, out) = run_with(
        &fx,
        &[
            "create",
            "ev",
            "--action",
            "mute-bus:ambient,sfx",
            "--action",
            "play:step1",
        ],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert_eq!(
        read_event(&fx, "ev").actions.unwrap()[0].targets,
        Some(vec![9, 2])
    );
    let warnings = out.all_warnings();
    assert_eq!(warnings.len(), 1, "only the outside bus warns: {warnings:?}");
    assert!(warnings[0].contains("main.buses.json"), "{warnings:?}");
    let data = out.last_success().unwrap();
    assert!(data["warnings"][0].as_str().unwrap().contains("ambient"));
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_reports_the_created_events() {
    let fx = fixture("ev_list");
    rich_event(&fx).await;
    let (result, out) = run_with(&fx, &["list"], &NonInteractiveInput, CaptureOutput::json()).await;
    result.unwrap();
    let data = out.last_success().unwrap();
    assert_eq!(data["count"], 1);
    assert_eq!(data["events"][0]["name"], "ev");
    assert_eq!(data["events"][0]["action_count"], 4);
    assert_eq!(data["events"][0]["primary_target"], 101);
}
