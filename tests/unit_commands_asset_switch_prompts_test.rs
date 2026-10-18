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

//! Tests for the interactive walk-throughs of `switch create` and `switch update`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use common::fixtures::CaptureOutput;
use common::scripted_input::ScriptedInput;
use common::switch_helpers::*;
use serde_json::json;

#[tokio::test]
async fn test_interactive_create_walks_the_states_and_their_ids() {
    let fx = fixture("sw_walk");
    let input = ScriptedInput::new(&[], &["wood", "stone", "7", "8"]).with_confirms(&[
        true,  // add another state
        false, // add another state
        false, // accept defaults?
    ]);
    let (result, _) = run(&fx, &["create", "sw"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert_eq!(
        states_of(&fx, "sw"),
        vec![(7, "wood".to_string()), (8, "stone".to_string())]
    );
    for label in [
        "state name",
        "another state",
        "accept defaults",
        "ID of state 'wood'",
    ] {
        assert!(input.saw_prompt(label), "no prompt mentions '{label}'");
    }
    assert_flatc(&fx, "sw");
}

#[tokio::test]
async fn test_interactive_create_accepting_defaults_generates_the_ids() {
    let fx = fixture("sw_walk_defaults");
    let input = ScriptedInput::new(&[], &["wood"]).with_confirms(&[false, true]);
    let (result, _) = run(&fx, &["create", "sw"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    let states = states_of(&fx, "sw");
    assert_eq!(states.len(), 1);
    assert_ne!(states[0].0, 0);
}

#[tokio::test]
async fn test_interactive_create_does_not_ask_for_what_flags_gave() {
    let fx = fixture("sw_flags_win");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run(&fx, &["create", "sw", "--states", "a,b"], &input).await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_interactive_update_adds_renames_and_removes() {
    let fx = fixture("sw_up_walk");
    create_ok(
        &fx,
        &[
            "--state",
            "wood,id=11",
            "--state",
            "stone,id=12",
            "--state",
            "metal,id=13",
        ],
    )
    .await;
    let input = ScriptedInput::new(
        &[
            "Add a state",
            "Rename a state",
            "stone (ID 12)",
            "Remove a state",
            "metal (ID 13)",
            "Done",
        ],
        &["grass", "rock"],
    );
    let (result, _) = run(&fx, &["update", "sw"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    let states = states_of(&fx, "sw");
    assert_eq!(states.len(), 3);
    assert_eq!(states[0], (11, "wood".to_string()));
    assert_eq!(states[1], (12, "rock".to_string()));
    assert_eq!(states[2].1, "grass");
    assert_flatc(&fx, "sw");
}

#[tokio::test]
async fn test_interactive_update_changes_a_state_id() {
    let fx = fixture("sw_up_id");
    create_ok(&fx, &["--state", "wood,id=11", "--state", "stone,id=12"]).await;
    let input = ScriptedInput::new(&["Change a state ID", "wood (ID 11)", "Done"], &["21"]);
    let (result, _) = run(&fx, &["update", "sw"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert_eq!(
        states_of(&fx, "sw"),
        vec![(21, "wood".to_string()), (12, "stone".to_string())]
    );
}

#[tokio::test]
async fn test_interactive_update_doing_nothing_leaves_the_file_alone() {
    let fx = fixture("sw_up_keep");
    create_ok(&fx, &["--states", "a,b"]).await;
    let before = std::fs::read_to_string(switch_path(&fx, "sw")).unwrap();
    let input = ScriptedInput::new(&["Done"], &[]);
    let (result, _) = run(&fx, &["update", "sw"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert_eq!(
        std::fs::read_to_string(switch_path(&fx, "sw")).unwrap(),
        before
    );
}

#[tokio::test]
async fn test_interactive_update_repairs_a_state_with_id_zero() {
    let fx = fixture("sw_up_legacy_id");
    fx.write_asset_json(
        "switches",
        "legacy",
        json!({"_a": 1, "id": 900, "name": "legacy",
               "states": [{"id": 0, "name": "wood"}, {"id": 2, "name": "stone"}]}),
    )
    .unwrap();
    let input = ScriptedInput::new(&["Change a state ID", "wood (ID 0)", "Done"], &["5"]);
    let (result, _) = run(&fx, &["update", "legacy"], &input).await;
    result.unwrap();
    assert_eq!(
        states_of(&fx, "legacy"),
        vec![(5, "wood".to_string()), (2, "stone".to_string())]
    );
    assert_eq!(read_raw(&fx, "legacy")["_a"], 1);
}

#[tokio::test]
async fn test_interactive_update_repairs_an_unnamed_state() {
    let fx = fixture("sw_up_legacy_name");
    fx.write_asset_json(
        "switches",
        "legacy",
        json!({"id": 900, "name": "legacy", "states": [{"id": 1}, {"id": 2, "name": "stone"}]}),
    )
    .unwrap();
    let input = ScriptedInput::new(&["Rename a state", "(unnamed) (ID 1)", "Done"], &["wood"]);
    let (result, _) = run(&fx, &["update", "legacy"], &input).await;
    result.unwrap();
    assert_eq!(
        states_of(&fx, "legacy"),
        vec![(1, "wood".to_string()), (2, "stone".to_string())]
    );
}

#[tokio::test]
async fn test_interactive_update_repairs_a_switch_without_states() {
    let fx = fixture("sw_up_legacy_none");
    fx.write_asset_json("switches", "legacy", json!({"id": 900, "name": "legacy"}))
        .unwrap();
    let input = ScriptedInput::new(&["Add a state", "Done"], &["wood"]);
    let (result, out) = run_with(
        &fx,
        &["update", "legacy"],
        &input,
        CaptureOutput::interactive(),
    )
    .await;
    result.unwrap();
    drop(out);
    assert_eq!(states_of(&fx, "legacy").len(), 1);
}

#[tokio::test]
async fn test_interactive_update_refuses_to_drop_a_used_state() {
    let fx = fixture("sw_up_in_use");
    create_ok(&fx, &["--state", "wood,id=11", "--state", "stone,id=12"]).await;
    let id = read_switch(&fx, "sw").id;
    fx.write_asset_json(
        "switch_containers",
        "footsteps",
        json!({"id": 301, "name": "footsteps", "bus": 2, "switch_group": id,
               "default_switch_state": 11, "entries": []}),
    )
    .unwrap();
    let before = std::fs::read_to_string(switch_path(&fx, "sw")).unwrap();
    let input = ScriptedInput::new(&["Remove a state", "wood (ID 11)", "Done"], &[]);
    let (result, _) = run(&fx, &["update", "sw"], &input).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("footsteps"));
    assert_eq!(
        std::fs::read_to_string(switch_path(&fx, "sw")).unwrap(),
        before
    );
}
