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

//! Interactive `switch-container update` walkthroughs and bus warnings.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::input::NonInteractiveInput;
use common::fixtures::CaptureOutput;
use common::scripted_input::ScriptedInput;
use common::switch_container_helpers::*;
use serde_json::json;

/// The selects every update walkthrough starts with: bus, group, attenuation, effect kept.
const KEEP_FOUR: [&str; 4] = ["(keep current)"; 4];

#[tokio::test]
async fn test_interactive_update_accepting_everything_changes_nothing() {
    let fx = fixture("scp_noop");
    rich_container(&fx).await;
    let before = read_container(&fx, "steps");
    let mut selects = KEEP_FOUR.to_vec();
    selects.extend(["World", "Linear", "UpdateOnPlay", "None", "wood"]);
    let input = ScriptedInput::new(&selects, &[]).with_confirms(&[false, false, false, false]);
    let (result, _) = run(&fx, &["update", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert_eq!(read_container(&fx, "steps"), before);
}

#[tokio::test]
async fn test_interactive_update_repairs_a_legacy_container() {
    let fx = fixture("scp_legacy");
    fx.write_asset_json(
        "switch_containers",
        "old",
        json!({"id": 900, "name": "old", "switch_group": 10, "default_switch_state": 0,
               "bus": 0, "entries": [], "effect": 0, "attenuation": 0,
               "scope": "World", "spatialization": "None", "update_behavior": "UpdateOnPlay"}),
    )
    .unwrap();
    // The first fader option and the first state are what an unset value used to fall back to:
    // picking them must still count as a change.
    let input = ScriptedInput::new(
        &[
            "sfx",
            "(keep current)",
            "(keep current)",
            "(keep current)",
            "World",
            "Constant",
            "UpdateOnPlay",
            "None",
            "wood",
        ],
        &[],
    )
    .with_confirms(&[false, false, false, false]);
    let (result, _) = run(&fx, &["update", "old"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(input.saw_prompt("current: unset"));
    let c = read_container(&fx, "old");
    assert_eq!(c.bus, 2);
    assert_eq!(c.fader.as_deref(), Some("Constant"));
    assert_eq!(c.default_switch_state, 11);
}

#[tokio::test]
async fn test_interactive_update_changing_the_group_reasks_state_and_entries() {
    let fx = fixture("scp_group");
    rich_container(&fx).await;
    let input = ScriptedInput::new(
        &[
            "(keep current)",
            "weather",
            "(keep current)",
            "(keep current)",
            "World",
            "Linear",
            "UpdateOnPlay",
            "None",
            "rain",
            "step2 (sound)",
            "rain",
        ],
        &[],
    )
    .with_confirms(&[
        false, false, false, // gain, pitch, priority
        true,  // add an entry
        true,  // accept this entry's defaults
        false, // another entry
    ]);
    let (result, _) = run(&fx, &["update", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    let c = read_container(&fx, "steps");
    assert_eq!((c.switch_group, c.default_switch_state), (20, 21));
    let entries = c.entries.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        (entries[0].object, entries[0].switch_states.clone()),
        (102, vec![21])
    );
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_interactive_update_removing_one_entry_keeps_the_others() {
    let fx = fixture("scp_edit");
    rich_container(&fx).await;
    let before = read_container(&fx, "steps").entries.unwrap();
    let mut selects = KEEP_FOUR.to_vec();
    selects.extend([
        "World",
        "Linear",
        "UpdateOnPlay",
        "None",
        "wood",
        "Remove an entry",
        "crowd",
        "Done",
    ]);
    let input = ScriptedInput::new(&selects, &[]).with_confirms(&[false, false, false, true]);
    let (result, _) = run(&fx, &["update", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    let after = read_container(&fx, "steps").entries.unwrap();
    assert_eq!(after, vec![before[0].clone()]);
}

#[tokio::test]
async fn test_bus_outside_the_active_bus_file_warns_on_create_and_update() {
    let fx = fixture("scp_inactive_bus");
    fx.write_buses_file("extra.buses.json", json!([{"id": 3, "name": "ambient"}]))
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
            "steps",
            "--bus",
            "ambient",
            "--switch-group",
            "surface",
        ],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert_eq!(read_container(&fx, "steps").bus, 3);
    assert!(
        out.all_warnings()
            .iter()
            .any(|w| w.contains("main.buses.json"))
    );
    assert!(
        out.last_success().unwrap()["warnings"][0]
            .as_str()
            .unwrap()
            .contains("ambient")
    );

    let (result, out) = run_with(
        &fx,
        &["update", "steps", "--bus", "sfx"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert!(out.all_warnings().is_empty());
    let (result, out) = run_with(
        &fx,
        &["update", "steps", "--bus", "ambient"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert!(out.all_warnings().iter().any(|w| w.contains("ambient")));
}

#[tokio::test]
async fn test_update_errors_for_missing_fader_and_fades_name_the_fix() {
    let fx = fixture("scp_suggestions");
    fx.write_asset_json(
        "switch_containers",
        "old",
        json!({"id": 900, "name": "old", "switch_group": 10, "default_switch_state": 11,
               "bus": 2, "entries": [{"object": 101, "switch_states": [11],
               "continue_between_states": false}],
               "effect": 0, "attenuation": 0, "scope": "World",
               "spatialization": "None", "update_behavior": "UpdateOnPlay"}),
    )
    .unwrap();
    let (result, _) = run(
        &fx,
        &["update", "old", "--gain", "0.5"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert!(
        am::common::errors::as_cli_error(&err)
            .unwrap()
            .suggestion
            .contains("--fader")
    );
    let (result, _) = run(
        &fx,
        &["update", "old", "--fader", "Linear"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    let suggestion = am::common::errors::as_cli_error(&err)
        .unwrap()
        .suggestion
        .clone();
    assert!(
        suggestion.contains("--entries-from-json") && suggestion.contains("--remove-entry"),
        "{suggestion}"
    );
}
