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

//! Tests for `switch create` / `switch update` with the full switch field surface.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::commands::asset::SwitchCommands;
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::fixtures::{AssetTestFixture, CaptureOutput};
use common::scripted_input::ScriptedInput;
use common::switch_helpers::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// Flag parsing
// ---------------------------------------------------------------------------

#[test]
fn test_create_parses_the_state_flags() {
    match parse(&[
        "create", "sw", "--state", "wood,id=5", "--state", "stone", "--states-from-json", "s.json",
    ]) {
        SwitchCommands::Create { extra, states, .. } => {
            assert!(states.is_none());
            assert_eq!(extra.state, vec!["wood,id=5".to_string(), "stone".to_string()]);
            assert_eq!(extra.states_from_json.unwrap().to_str(), Some("s.json"));
        }
        _ => panic!("expected create"),
    }
}

#[test]
fn test_update_parses_the_edit_flags() {
    match parse(&[
        "update", "sw", "--add-state", "grass", "--remove-state", "wood", "--remove-state", "12",
        "--rename-state", "stone=rock", "--clear-states",
    ]) {
        SwitchCommands::Update { edit, .. } => {
            assert_eq!(edit.add_state, vec!["grass".to_string()]);
            assert_eq!(edit.remove_state, vec!["wood".to_string(), "12".to_string()]);
            assert_eq!(edit.rename_state, vec!["stone=rock".to_string()]);
            assert!(edit.clear_states);
        }
        _ => panic!("expected update"),
    }
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_with_states_names_keeps_working() {
    let fx = fixture("sw_create_names");
    create_ok(&fx, &["--states", "wood,stone,metal"]).await;
    let states = states_of(&fx, "sw");
    assert_eq!(
        states.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
        vec!["wood", "stone", "metal"]
    );
    assert!(states.iter().all(|(id, _)| *id != 0));
    assert_flatc(&fx, "sw");
}

#[tokio::test]
async fn test_create_with_state_flag_accepts_explicit_ids() {
    let fx = fixture("sw_create_state");
    create_ok(&fx, &["--state", "wood,id=5", "--state", "stone", "--state", "metal,id=9"]).await;
    let states = states_of(&fx, "sw");
    assert_eq!(states[0], (5, "wood".to_string()));
    assert_eq!(states[2], (9, "metal".to_string()));
    assert!(![0, 5, 9].contains(&states[1].0));
    assert_flatc(&fx, "sw");
}

#[tokio::test]
async fn test_create_from_json_accepts_names_objects_and_sdk_switches() {
    let fx = fixture("sw_create_json");
    let file = fx.project_root().join("s.json");
    std::fs::write(&file, r#"["wood", {"name": "stone", "id": 7}, {"name": "metal"}]"#).unwrap();
    create_ok(&fx, &["--states-from-json", file.to_str().unwrap()]).await;
    let states = states_of(&fx, "sw");
    assert_eq!(states.len(), 3);
    assert_eq!(states[1], (7, "stone".to_string()));
    assert_flatc(&fx, "sw");

    let fx = fixture("sw_create_json_sdk");
    let file = fx.project_root().join("s.json");
    std::fs::write(
        &file,
        r#"{"id": 1, "name": "old", "states": [{"id": 3, "name": "day"}, {"id": 4, "name": "night"}]}"#,
    )
    .unwrap();
    create_ok(&fx, &["--states-from-json", file.to_str().unwrap()]).await;
    assert_eq!(states_of(&fx, "sw")[1], (4, "night".to_string()));
}

#[tokio::test]
async fn test_create_missing_states_names_the_flags_and_writes_nothing() {
    let fx = fixture("sw_no_states");
    let (result, _) = run(&fx, &["create", "sw"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("--states") && text.contains("--state"), "{text}");
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_VALIDATION_FIELD);
    assert!(!switch_path(&fx, "sw").exists());
}

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = fixture("sw_json_no_prompt");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run_with(&fx, &["create", "sw"], &input, CaptureOutput::json()).await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
    assert!(!switch_path(&fx, "sw").exists());

    create_ok(&fx, &["--states", "a,b"]).await;
    let before = std::fs::read_to_string(switch_path(&fx, "sw")).unwrap();
    let (result, _) = run_with(&fx, &["update", "sw"], &input, CaptureOutput::json()).await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(switch_path(&fx, "sw")).unwrap(), before);
}

#[tokio::test]
async fn test_create_rejects_bad_state_syntax() {
    let fx = fixture("sw_bad_syntax");
    for (bad, needle) in [
        ("wood,id=abc", "id"),
        ("wood,id=0", "id"),
        ("wood,color=red", "color"),
        (",id=3", "name"),
        ("wood,id=3,id=4", "twice"),
        ("wood,id", "KEY=VALUE"),
    ] {
        let (result, _) = run(&fx, &["create", "sw", "--state", bad], &NonInteractiveInput).await;
        let err = result.unwrap_err();
        assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_VALIDATION_FIELD, "{bad}");
        assert!(format!("{err:#}").contains(needle), "{bad}: {err:#}");
        assert!(!switch_path(&fx, "sw").exists());
    }
}

#[tokio::test]
async fn test_create_duplicate_names_and_ids_are_rejected() {
    let fx = fixture("sw_dups");
    for args in [
        vec!["--state", "wood", "--state", "wood"],
        vec!["--state", "a,id=4", "--state", "b,id=4"],
        vec!["--states", "a,a"],
    ] {
        let mut full = vec!["create", "sw"];
        full.extend(args);
        let (result, _) = run(&fx, &full, &NonInteractiveInput).await;
        assert!(result.is_err());
        assert!(!switch_path(&fx, "sw").exists());
    }
}

#[tokio::test]
async fn test_create_conflicting_replace_flags_are_rejected() {
    let fx = fixture("sw_conflict");
    let (result, _) = run(
        &fx,
        &["create", "sw", "--states", "a", "--state", "b"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("--states"), "{err:#}");
}

#[tokio::test]
async fn test_create_json_reports_state_count() {
    let fx = fixture("sw_create_report");
    let (result, out) = run_with(
        &fx,
        &["create", "sw", "--states", "a,b"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert_eq!(out.last_success().unwrap()["state_count"], 2);
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

/// `sw` with wood (11), stone (12), metal (13).
async fn rich_switch(fx: &AssetTestFixture) {
    create_ok(
        fx,
        &["--state", "wood,id=11", "--state", "stone,id=12", "--state", "metal,id=13"],
    )
    .await;
}

#[tokio::test]
async fn test_update_states_keeps_the_ids_of_the_names_that_stay() {
    let fx = fixture("sw_up_replace");
    rich_switch(&fx).await;
    let (result, out) = run_with(
        &fx,
        &["update", "sw", "--states", "metal,wood,grass"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    let states = states_of(&fx, "sw");
    assert_eq!(states[0], (13, "metal".to_string()));
    assert_eq!(states[1], (11, "wood".to_string()));
    assert_eq!(states[2].1, "grass");
    assert_eq!(out.last_success().unwrap()["updated_fields"], json!(["states"]));
    assert_flatc(&fx, "sw");
}

#[tokio::test]
async fn test_update_rename_add_and_remove() {
    let fx = fixture("sw_up_edit");
    rich_switch(&fx).await;
    let (result, _) = run(
        &fx,
        &[
            "update", "sw", "--rename-state", "stone=rock", "--remove-state", "13", "--add-state",
            "grass,id=40",
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert_eq!(
        states_of(&fx, "sw"),
        vec![
            (11, "wood".to_string()),
            (12, "rock".to_string()),
            (40, "grass".to_string())
        ]
    );
    assert_flatc(&fx, "sw");
}

#[tokio::test]
async fn test_update_clear_with_new_states_replaces_the_list() {
    let fx = fixture("sw_up_clear");
    rich_switch(&fx).await;
    let (result, _) = run(
        &fx,
        &["update", "sw", "--clear-states", "--add-state", "only,id=3"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert_eq!(states_of(&fx, "sw"), vec![(3, "only".to_string())]);
}

#[tokio::test]
async fn test_update_leaving_no_states_is_rejected_and_writes_nothing() {
    let fx = fixture("sw_up_empty");
    rich_switch(&fx).await;
    let before = std::fs::read_to_string(switch_path(&fx, "sw")).unwrap();
    let (result, _) = run(&fx, &["update", "sw", "--clear-states"], &NonInteractiveInput).await;
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(switch_path(&fx, "sw")).unwrap(), before);
}

#[tokio::test]
async fn test_update_unknown_state_fails_and_writes_nothing() {
    let fx = fixture("sw_up_unknown");
    rich_switch(&fx).await;
    let before = std::fs::read_to_string(switch_path(&fx, "sw")).unwrap();
    for flag in [["--remove-state", "grass"], ["--rename-state", "grass=lawn"]] {
        let mut args = vec!["update", "sw"];
        args.extend(flag);
        let (result, _) = run(&fx, &args, &NonInteractiveInput).await;
        assert_eq!(
            as_cli_error(&result.unwrap_err()).unwrap().code,
            codes::ERR_VALIDATION_FIELD
        );
    }
    assert_eq!(std::fs::read_to_string(switch_path(&fx, "sw")).unwrap(), before);
}

#[tokio::test]
async fn test_update_rename_syntax_is_checked() {
    let fx = fixture("sw_up_rename_syntax");
    rich_switch(&fx).await;
    for bad in ["stone", "=rock", "stone="] {
        let (result, _) = run(
            &fx,
            &["update", "sw", "--rename-state", bad],
            &NonInteractiveInput,
        )
        .await;
        let err = result.unwrap_err();
        assert!(format!("{err:#}").contains("OLD=NEW"), "{bad}: {err:#}");
    }
}

#[tokio::test]
async fn test_update_refuses_to_drop_a_state_a_switch_container_uses() {
    let fx = fixture("sw_up_in_use");
    rich_switch(&fx).await;
    let id = read_switch(&fx, "sw").id;
    fx.create_test_sound("step", 101).unwrap();
    fx.write_asset_json(
        "switch_containers",
        "footsteps",
        json!({"id": 301, "name": "footsteps", "bus": 2, "switch_group": id,
               "default_switch_state": 11,
               "entries": [{"object": 101, "switch_states": [12]}]}),
    )
    .unwrap();
    let before = std::fs::read_to_string(switch_path(&fx, "sw")).unwrap();

    // 12 is used by an entry, 11 as the default; 13 is free.
    for flag in [["--remove-state", "stone"], ["--remove-state", "wood"]] {
        let mut args = vec!["update", "sw"];
        args.extend(flag);
        let (result, _) = run(&fx, &args, &NonInteractiveInput).await;
        let err = result.unwrap_err();
        let cli = as_cli_error(&err).unwrap();
        assert_eq!(cli.code, codes::ERR_ASSET_IN_USE, "{err:#}");
        assert!(format!("{err:#}").contains("footsteps"), "{err:#}");
    }
    // Replacing the list so that a used id disappears is refused too.
    let (result, _) = run(&fx, &["update", "sw", "--states", "metal"], &NonInteractiveInput).await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_ASSET_IN_USE
    );
    assert_eq!(std::fs::read_to_string(switch_path(&fx, "sw")).unwrap(), before);

    // An unused state can go, and a rename (same id) is always fine.
    let (result, _) = run(
        &fx,
        &["update", "sw", "--remove-state", "metal", "--rename-state", "stone=rock"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert_eq!(states_of(&fx, "sw").len(), 2);
}

#[tokio::test]
async fn test_update_ignores_containers_of_other_switches() {
    let fx = fixture("sw_up_other_group");
    rich_switch(&fx).await;
    fx.write_asset_json(
        "switch_containers",
        "other",
        json!({"id": 302, "name": "other", "bus": 2, "switch_group": 999,
               "default_switch_state": 11, "entries": [{"object": 101, "switch_states": [12]}]}),
    )
    .unwrap();
    let (result, _) = run(&fx, &["update", "sw", "--remove-state", "stone"], &NonInteractiveInput).await;
    result.unwrap();
}

#[tokio::test]
async fn test_update_keeps_unknown_keys_and_untouched_states_verbatim() {
    let fx = fixture("sw_up_unknown_keys");
    fx.write_asset_json(
        "switches",
        "hand",
        json!({
            "_a": {"note": "kept"},
            "id": 900,
            "name": "hand",
            "states": [{"id": 1, "name": "a", "_s": "verbatim"}, {"id": 2, "name": "b"}],
            "_z": 3
        }),
    )
    .unwrap();
    // A no-op update rewrites the file without touching the states.
    let (result, _) = run(&fx, &["update", "hand"], &NonInteractiveInput).await;
    result.unwrap();
    let raw = read_raw(&fx, "hand");
    assert_eq!(raw["states"][0]["_s"], "verbatim");
    assert_eq!(raw["_a"], json!({"note": "kept"}));
    assert_eq!(raw["_z"], 3);

    let (result, _) = run(&fx, &["update", "hand", "--add-state", "c"], &NonInteractiveInput).await;
    result.unwrap();
    let raw = read_raw(&fx, "hand");
    let keys: Vec<&str> = raw.as_object().unwrap().keys().map(String::as_str).collect();
    let at = |k: &str| keys.iter().position(|x| *x == k).unwrap();
    assert!(at("states") < at("_a"), "unknown keys follow the modelled ones: {keys:?}");
    assert!(at("_a") < at("_z"), "unknown keys keep their relative order: {keys:?}");
    assert_eq!(raw["_a"], json!({"note": "kept"}));
    assert_eq!(raw["states"].as_array().unwrap().len(), 3);
}
