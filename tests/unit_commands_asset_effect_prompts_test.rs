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

//! Tests for the interactive walk-throughs of `effect create` and `effect update`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use common::effect_helpers::*;
use common::fixtures::CaptureOutput;
use common::scripted_input::ScriptedInput;
use serde_json::json;

#[tokio::test]
async fn test_interactive_create_walks_the_filter_and_every_parameter() {
    let fx = fixture("fx_walk");
    let input = ScriptedInput::new(&["Delay", "static", "bind to RTPC", "speed"], &["0.7"])
        .with_confirms(&[
            false, // accept defaults?
            true,  // add a parameter
            true,  // add another
            false, // add another
        ]);
    let (result, _) = run(&fx, &["create", "fx1"], &input).await;
    result.unwrap();
    input.assert_all_consumed();

    let effect = read_effect(&fx, "fx1");
    assert_eq!(effect.effect.as_deref(), Some("Delay"));
    let params = effect.parameters.unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].as_static(), Some(0.7));
    assert_eq!(params[1].rtpc.as_ref().unwrap().id, 401);
    for label in ["effect filter", "accept defaults", "add a parameter", "another parameter"] {
        assert!(input.saw_prompt(label), "no prompt mentions '{label}'");
    }
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_interactive_create_offers_every_registered_filter_and_a_plugin_entry() {
    let fx = fixture("fx_options");
    let input = ScriptedInput::new(&["Lofi"], &[]).with_confirms(&[true]);
    let (result, _) = run(&fx, &["create", "fx1"], &input).await;
    result.unwrap();
    let options = input.options_seen.lock().unwrap()[0].clone();
    for name in ["BassBoost", "Delay", "Equalizer", "WaveShaper"] {
        assert!(options.iter().any(|o| o == name), "{name} missing from {options:?}");
    }
    assert!(options.last().unwrap().to_lowercase().contains("plugin"));
}

#[tokio::test]
async fn test_interactive_create_accepting_defaults_leaves_the_parameters_out() {
    let fx = fixture("fx_defaults");
    let input = ScriptedInput::new(&[], &[]).with_confirms(&[true]);
    let (result, _) = run(&fx, &["create", "fx1", "--effect-type", "Flanger"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("effect filter"), "the flag already answered it");
    let effect = read_effect(&fx, "fx1");
    assert_eq!(effect.effect.as_deref(), Some("Flanger"));
    assert!(effect.parameters.unwrap_or_default().is_empty());
}

#[tokio::test]
async fn test_interactive_create_with_plugin_filter_asks_for_the_name_and_warns() {
    let fx = fixture("fx_plugin");
    let input = ScriptedInput::new(&["Other (plugin filter)"], &["MyPlugin"]).with_confirms(&[true]);
    let (result, out) = run(&fx, &["create", "fx1"], &input).await;
    result.unwrap();
    assert_eq!(read_effect(&fx, "fx1").effect.as_deref(), Some("MyPlugin"));
    assert!(out.all_warnings().iter().any(|w| w.contains("MyPlugin")));
}

#[tokio::test]
async fn test_interactive_create_does_not_ask_for_what_flags_gave() {
    let fx = fixture("fx_flags_win");
    let input = ScriptedInput::new(&["Delay"], &[]);
    let (result, _) = run(&fx, &["create", "fx1", "--param", "1"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("accept defaults"));
    assert_eq!(read_effect(&fx, "fx1").parameters.unwrap().len(), 1);
}

#[tokio::test]
async fn test_interactive_create_with_no_rtpc_in_the_project_explains_how_to_create_one() {
    let fx = common::fixtures::AssetTestFixture::new("fx_no_rtpc").unwrap();
    let input = ScriptedInput::new(&["Delay", "bind to RTPC"], &[]).with_confirms(&[false, true]);
    let (result, _) = run(&fx, &["create", "fx1"], &input).await;
    let message = format!("{:#}", result.unwrap_err());
    assert!(message.contains("am asset rtpc create"), "{message}");
    assert!(!effect_path(&fx, "fx1").exists());
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_interactive_update_changes_filter_and_adds_a_parameter() {
    let fx = fixture("fx_up_walk");
    create_ok(&fx, &["--param", "0.5"]).await;
    let input = ScriptedInput::new(&["Lofi", "Add a parameter", "static", "Done"], &["4"]);
    let (result, out) = run_with(&fx, &["update", "fx1"], &input, CaptureOutput::json()).await;
    result.unwrap();
    // JSON output mode never prompts: nothing may have changed.
    assert!(input.prompts.lock().unwrap().is_empty());
    assert_eq!(read_effect(&fx, "fx1").effect.as_deref(), Some("Delay"));
    drop(out);

    let (result, out) = run(&fx, &["update", "fx1"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(input.saw_prompt("current: Delay"));
    let effect = read_effect(&fx, "fx1");
    assert_eq!(effect.effect.as_deref(), Some("Lofi"));
    let params = effect.parameters.unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params[1].as_static(), Some(4.0));
    drop(out);
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_interactive_update_keeping_everything_changes_nothing() {
    let fx = fixture("fx_up_keep");
    create_ok(&fx, &["--param", "0.5", "--param-rtpc", "1=wind"]).await;
    let before = std::fs::read_to_string(effect_path(&fx, "fx1")).unwrap();
    let input = ScriptedInput::new(&["Delay", "Done"], &[]);
    let (result, _) = run(&fx, &["update", "fx1"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert_eq!(std::fs::read_to_string(effect_path(&fx, "fx1")).unwrap(), before);
}

#[tokio::test]
async fn test_interactive_update_edits_and_removes_parameters() {
    let fx = fixture("fx_up_edit_remove");
    create_ok(&fx, &["--param", "0.5", "--param", "1", "--param-rtpc", "1=wind", "--param", "9"]).await;
    // create: [0.5, bound(wind), ... ] -> --param 0.5, 1, 9 then bind index 1
    let before = read_effect(&fx, "fx1").parameters.unwrap();
    assert_eq!(before.len(), 3);
    let input = ScriptedInput::new(
        &[
            "Delay",
            "Edit a parameter",
            "0: 0.5",
            "bind to RTPC",
            "speed",
            "Remove a parameter",
            "2: 9",
            "Done",
        ],
        &[],
    );
    let (result, _) = run(&fx, &["update", "fx1"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    let after = read_effect(&fx, "fx1").parameters.unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0].rtpc.as_ref().unwrap().id, 401);
    assert_eq!(after[1], before[1]);
}

#[tokio::test]
async fn test_interactive_update_repairs_a_file_without_effect_type() {
    let fx = fixture("fx_up_unset");
    fx.write_asset_json("effects", "legacy", json!({"id": 901, "name": "legacy", "_a": 1}))
        .unwrap();
    let input = ScriptedInput::new(&["Equalizer", "Done"], &[]);
    let (result, _) = run(&fx, &["update", "legacy"], &input).await;
    result.unwrap();
    assert!(input.saw_prompt("current: unset"));
    let raw = read_raw(&fx, "legacy");
    assert_eq!(raw["effect"], "Equalizer");
    assert_eq!(raw["_a"], 1);
}

#[tokio::test]
async fn test_interactive_update_keeps_a_plugin_filter_name() {
    let fx = fixture("fx_up_plugin");
    fx.write_asset_json(
        "effects",
        "plug",
        json!({"id": 902, "name": "plug", "effect": "MyPlugin"}),
    )
    .unwrap();
    let before = read_raw(&fx, "plug");
    let input = ScriptedInput::new(&["Other (plugin filter)", "Done"], &["MyPlugin"]);
    let (result, _) = run(&fx, &["update", "plug"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(input.saw_prompt("plugin: MyPlugin"));
    assert_eq!(read_raw(&fx, "plug"), before);}
