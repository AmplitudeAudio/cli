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

//! Tests for `switch-container create` and `switch-container update`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::assets::generated::{Scope, Spatialization, SwitchContainerUpdateBehavior, ValueKind};
use am::commands::asset::SwitchContainerCommands;
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::fixtures::CaptureOutput;
use common::scripted_input::ScriptedInput;
use common::switch_container_helpers::*;
use serde_json::json;

fn code_of(err: &anyhow::Error) -> i32 {
    as_cli_error(err).unwrap().code
}

// ---------------------------------------------------------------------------
// Clap surface
// ---------------------------------------------------------------------------

#[test]
fn test_create_parses_every_flag() {
    let cmd = parse(&[
        "create", "steps", "--bus", "sfx", "--switch-group", "surface", "--gain", "0.5",
        "--priority", "9", "--spatialization", "hrtf", "--effect", "echo", "--attenuation",
        "short", "--scope", "entity", "--fader", "EaseIn", "--pitch", "1.2", "--gain-rtpc", "g",
        "--pitch-rtpc", "p", "--priority-rtpc", "pr", "--update-behavior", "UpdateOnChange",
        "--default-switch-state", "stone", "--entry", "step1,states=wood", "--entry",
        "crowd,states=stone", "--entries-from-json", "e.json", "--map", "wood=step1",
    ]);
    let SwitchContainerCommands::Create { bus, switch_group, extra, priority, .. } = cmd else {
        panic!("expected create");
    };
    assert_eq!(bus.as_deref(), Some("sfx"));
    assert_eq!(switch_group.as_deref(), Some("surface"));
    assert_eq!(priority, Some(9));
    assert_eq!(extra.update_behavior.as_deref(), Some("UpdateOnChange"));
    assert_eq!(extra.default_switch_state.as_deref(), Some("stone"));
    assert_eq!(extra.entry.len(), 2);
    assert_eq!(extra.map, vec!["wood=step1"]);
    assert_eq!(extra.gain_rtpc.as_deref(), Some("g"));
    assert_eq!(extra.pitch, Some(1.2));
}

#[test]
fn test_legacy_switch_flag_is_an_alias() {
    let SwitchContainerCommands::Create { switch_group, .. } =
        parse(&["create", "steps", "--switch", "surface"])
    else {
        panic!("expected create");
    };
    assert_eq!(switch_group.as_deref(), Some("surface"));
}

#[test]
fn test_update_parses_entry_editing_flags() {
    let SwitchContainerCommands::Update { edit, extra, .. } = parse(&[
        "update", "steps", "--add-entry", "step1,states=wood", "--remove-entry", "crowd",
        "--clear-entries", "--entry", "step2,states=stone",
    ]) else {
        panic!("expected update");
    };
    assert_eq!(edit.add_entry, vec!["step1,states=wood"]);
    assert_eq!(edit.remove_entry, vec!["crowd"]);
    assert!(edit.clear_entries);
    assert_eq!(extra.entry, vec!["step2,states=stone"]);
}

// ---------------------------------------------------------------------------
// Non-interactive create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_with_entries_and_fades_is_accepted_by_flatc() {
    let fx = fixture("sc_create_rich");
    rich_container(&fx).await;
    let c = read_container(&fx, "steps");
    assert_eq!(c.bus, 2);
    assert_eq!(c.switch_group, 10);
    assert_eq!(c.default_switch_state, 11, "defaults to the first state");
    assert_eq!(c.fader.as_deref(), Some("Linear"));
    let entries = c.entries.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].object, 101);
    assert_eq!(entries[0].switch_states, vec![11, 12]);
    assert!(entries[0].continue_between_states);
    let fade_in = entries[0].fade_in.clone().unwrap();
    assert!((fade_in.duration - 0.2).abs() < 1e-6);
    assert_eq!(fade_in.fader.as_deref(), Some("EaseIn"));
    assert_eq!(entries[0].fade_out.clone().unwrap().fader.as_deref(), Some("Linear"));
    assert_eq!(entries[1].object, 201);
    assert_eq!(entries[1].gain.clone().unwrap().kind, ValueKind::RTPC);
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_create_sets_every_container_field() {
    let fx = fixture("sc_create_fields");
    create_ok(
        &fx,
        &[
            "--bus", "sfx", "--switch-group", "surface", "--effect", "echo", "--attenuation",
            "short", "--scope", "entity", "--fader", "EaseIn", "--pitch", "1.5", "--priority-rtpc",
            "speed", "--spatialization", "position", "--update-behavior", "UpdateOnChange",
            "--default-switch-state", "gravel",
        ],
    )
    .await;
    let c = read_container(&fx, "steps");
    assert_eq!((c.effect, c.attenuation), (66, 55));
    assert_eq!(c.scope, Scope::Entity);
    assert_eq!(c.fader.as_deref(), Some("EaseIn"));
    assert_eq!(c.pitch.unwrap().as_static(), Some(1.5));
    assert_eq!(c.priority.unwrap().kind, ValueKind::RTPC);
    assert_eq!(c.spatialization, Spatialization::Position);
    assert_eq!(c.update_behavior, SwitchContainerUpdateBehavior::UpdateOnChange);
    assert_eq!(c.default_switch_state, 13);
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_legacy_map_flag_builds_entries() {
    let fx = fixture("sc_create_map");
    create_ok(
        &fx,
        &["--bus", "sfx", "--switch", "surface", "--map", "wood=step1", "--map", "stone=step1", "--map", "gravel=crowd"],
    )
    .await;
    let entries = read_container(&fx, "steps").entries.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].switch_states, vec![11, 12]);
    assert_eq!(entries[1].object, 201);
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_entries_from_json_resolves_names_and_fades() {
    let fx = fixture("sc_create_json");
    let file = fx.project_root().join("entries.json");
    std::fs::write(
        &file,
        json!([
            {"object": "step1", "switch_states": ["wood", 12],
             "fade_out": {"duration": 0.5, "fader": "EaseOut"}, "gain": 0.5},
            {"object": "collection:boom", "switch_states": ["gravel"]}
        ])
        .to_string(),
    )
    .unwrap();
    create_ok(
        &fx,
        &["--bus", "sfx", "--switch-group", "surface", "--entries-from-json", file.to_str().unwrap()],
    )
    .await;
    let entries = read_container(&fx, "steps").entries.unwrap();
    assert_eq!(entries[0].switch_states, vec![11, 12]);
    assert_eq!(entries[0].fade_out.clone().unwrap().fader.as_deref(), Some("EaseOut"));
    assert_eq!(entries[1].object, 203);
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_missing_bus_names_the_flag_and_writes_nothing() {
    let fx = fixture("sc_no_bus");
    let (result, _) = run(&fx, &["create", "steps", "--switch-group", "surface"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_VALIDATION_EMPTY_REFERENCE);
    assert!(format!("{err:#}").contains("--bus"), "{err:#}");
    assert!(!container_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_missing_switch_group_names_the_flag_and_writes_nothing() {
    let fx = fixture("sc_no_group");
    let (result, _) = run(&fx, &["create", "steps", "--bus", "sfx"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_VALIDATION_EMPTY_REFERENCE);
    assert!(format!("{err:#}").contains("--switch-group"), "{err:#}");
    assert!(!container_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_unknown_state_name_lists_the_states_of_the_group() {
    let fx = fixture("sc_bad_state");
    let (result, _) = run(
        &fx,
        &["create", "steps", "--bus", "sfx", "--switch-group", "surface", "--entry", "step1,states=snow"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_VALIDATION_REFERENCE);
    let text = format!("{err:#} {}", as_cli_error(&err).unwrap().suggestion);
    assert!(text.contains("wood") && text.contains("stone") && text.contains("gravel"), "{text}");
    assert!(!text.contains("rain"));
    assert!(!container_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_dangling_ids_are_reference_errors() {
    let fx = fixture("sc_dangling");
    for (entry, field) in [
        ("999,states=wood", "entries[0].object"),
        ("step1,states=999", "entries[0].switch_states[0]"),
        ("step1,states=21", "entries[0].switch_states[0]"),
    ] {
        let (result, _) = run(
            &fx,
            &["create", "steps", "--bus", "sfx", "--switch-group", "surface", "--entry", entry],
            &NonInteractiveInput,
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(code_of(&err), codes::ERR_VALIDATION_REFERENCE, "{entry}");
        assert!(format!("{err:#}").contains(field) || format!("{err:?}").contains(field), "{entry}: {err:?}");
        assert!(!container_path(&fx, "steps").exists());
    }
}

#[tokio::test]
async fn test_ambiguous_object_name_must_be_qualified() {
    let fx = fixture("sc_ambiguous");
    let (result, _) = run(
        &fx,
        &["create", "steps", "--bus", "sfx", "--switch-group", "surface", "--entry", "boom,states=wood"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_VALIDATION_REFERENCE);
    create_ok(&fx, &["--bus", "sfx", "--switch-group", "surface", "--entry", "sound:boom,states=wood"]).await;
    assert_eq!(read_container(&fx, "steps").entries.unwrap()[0].object, 103);
}

#[tokio::test]
async fn test_entry_syntax_errors_are_field_errors() {
    let fx = fixture("sc_syntax");
    for bad in [
        "step1",
        "step1,states=wood,bogus=1",
        "step1,states=wood,fade-in=abc",
        "step1,states=wood,fade-in=-1",
        "step1,states=wood,fade-in=1:Wobbly",
        "step1,states=wood,continue=maybe",
        "step1,states=wood,states=stone",
        ",states=wood",
    ] {
        let (result, _) = run(
            &fx,
            &["create", "steps", "--bus", "sfx", "--switch-group", "surface", "--entry", bad],
            &NonInteractiveInput,
        )
        .await;
        assert_eq!(code_of(&result.unwrap_err()), codes::ERR_VALIDATION_FIELD, "{bad}");
        assert!(!container_path(&fx, "steps").exists(), "{bad}");
    }
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_changes_only_the_given_fields() {
    let fx = fixture("sc_up_fields");
    rich_container(&fx).await;
    let before = read_container(&fx, "steps");
    let (result, out) = run_with(
        &fx,
        &["update", "steps", "--update-behavior", "UpdateOnChange", "--effect", "echo"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    let mut expected = before.clone();
    expected.update_behavior = SwitchContainerUpdateBehavior::UpdateOnChange;
    expected.effect = 66;
    assert_eq!(read_container(&fx, "steps"), expected);
    assert_eq!(
        out.last_success().unwrap()["updated_fields"],
        json!(["update_behavior", "effect"])
    );
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_update_keeps_unknown_keys_after_the_modelled_ones_and_untouched_entries_verbatim() {
    let fx = fixture("sc_up_unknown");
    rich_container(&fx).await;
    let mut raw = read_raw(&fx, "steps");
    raw["_a"] = json!("hand written");
    raw["entries"][0]["x_note"] = json!("keep me");
    std::fs::write(container_path(&fx, "steps"), serde_json::to_string_pretty(&raw).unwrap()).unwrap();
    let entries_before = raw["entries"].clone();

    let (result, _) = run(&fx, &["update", "steps", "--gain", "0.9"], &NonInteractiveInput).await;
    result.unwrap();
    let after = read_raw(&fx, "steps");
    assert_eq!(after["entries"], entries_before, "entries are written back verbatim");
    assert_eq!(after["_a"], "hand written");
    let keys: Vec<&String> = after.as_object().unwrap().keys().collect();
    let position = |k: &str| keys.iter().position(|x| x.as_str() == k).unwrap();
    assert!(position("entries") < position("_a"), "{keys:?}");
    assert!(position("bus") < position("_a"), "{keys:?}");
    assert_eq!(after["gain"]["value"], json!(0.9f32 as f64));
}

#[tokio::test]
async fn test_update_add_remove_and_map_entries() {
    let fx = fixture("sc_up_entries");
    rich_container(&fx).await;
    let (result, _) = run(
        &fx,
        &[
            "update", "steps", "--add-entry", "step2,states=stone,fade-out=0.3", "--remove-entry",
            "crowd", "--map", "gravel=step1",
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let entries = read_container(&fx, "steps").entries.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].object, 101);
    assert_eq!(entries[0].switch_states, vec![11, 12, 13]);
    assert_eq!(entries[1].object, 102);
    assert_flatc(&fx, "steps");

    let (result, _) = run(&fx, &["update", "steps", "--remove-entry", "crowd"], &NonInteractiveInput).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_VALIDATION_FIELD);
}

#[tokio::test]
async fn test_update_to_a_state_of_another_switch_is_rejected_and_writes_nothing() {
    let fx = fixture("sc_up_bad_state");
    rich_container(&fx).await;
    let before = std::fs::read_to_string(container_path(&fx, "steps")).unwrap();
    let (result, _) = run(&fx, &["update", "steps", "--default-switch-state", "rain"], &NonInteractiveInput).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_VALIDATION_REFERENCE);
    let (result, _) = run(&fx, &["update", "steps", "--switch-group", "weather"], &NonInteractiveInput).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_VALIDATION_REFERENCE);
    assert_eq!(std::fs::read_to_string(container_path(&fx, "steps")).unwrap(), before);
}

#[tokio::test]
async fn test_update_of_a_legacy_container_without_bus_points_at_the_flag() {
    let fx = fixture("sc_up_legacy");
    fx.write_asset_json(
        "switch_containers",
        "old",
        json!({"id": 900, "name": "old", "switch_group": 10, "default_switch_state": 11,
               "bus": 0, "entries": [], "effect": 0, "attenuation": 0,
               "scope": "World", "spatialization": "None", "update_behavior": "UpdateOnPlay"}),
    )
    .unwrap();
    let (result, _) = run(&fx, &["update", "old", "--gain", "0.5"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_VALIDATION_EMPTY_REFERENCE);
    assert!(as_cli_error(&err).unwrap().suggestion.contains("--bus"));
    let (result, _) = run(&fx, &["update", "old", "--bus", "sfx", "--fader", "Linear"], &NonInteractiveInput).await;
    result.unwrap();
}

// ---------------------------------------------------------------------------
// JSON / interactive
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = fixture("sc_json_noprompt");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run_with(
        &fx,
        &["create", "steps", "--switch-group", "surface"],
        &input,
        CaptureOutput::json(),
    )
    .await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_VALIDATION_EMPTY_REFERENCE);
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_interactive_create_walks_every_field_with_states_from_the_group() {
    let fx = fixture("sc_walk");
    let input = ScriptedInput::new(
        &[
            "sfx",           // bus
            "surface",       // switch group
            "echo",          // effect
            "short",         // attenuation
            "Entity",        // scope
            "EaseIn",        // fader
            "UpdateOnChange", // update behavior
            "static",        // gain type
            "static",        // priority type
            "Position",      // spatialization
            "stone",         // default state
            "step1 (sound)", // entry object
            "wood",          // entry state
            "gravel",        // second state
            "Linear",        // fade-in fader
        ],
        &["0.8", "10", "0.25"],
    )
    .with_confirms(&[
        false, // accept defaults
        false, // pitch?
        true,  // add an entry
        true,  // another state
        false, // yet another state
        false, // accept entry defaults
        true,  // continue between states
        true,  // fade in
        false, // fade out
        false, // entry gain
        false, // entry pitch
        false, // another entry
    ]);
    let (result, _) = run(&fx, &["create", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    for label in ["remaining optional fields", "Default switch state", "Switch state of this entry"] {
        assert!(input.saw_prompt(label), "no prompt mentioned '{label}'");
    }
    let c = read_container(&fx, "steps");
    assert_eq!((c.bus, c.switch_group, c.default_switch_state), (2, 10, 12));
    assert_eq!(c.update_behavior, SwitchContainerUpdateBehavior::UpdateOnChange);
    let entry = &c.entries.unwrap()[0];
    assert_eq!(entry.switch_states, vec![11, 13]);
    assert!(entry.continue_between_states);
    assert!((entry.fade_in.clone().unwrap().duration - 0.25).abs() < 1e-9);
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_interactive_create_accepting_defaults_asks_nothing_more() {
    let fx = fixture("sc_walk_defaults");
    let input = ScriptedInput::new(&["sfx", "surface"], &[]).with_confirms(&[true]);
    let (result, _) = run(&fx, &["create", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("Scope"));
    assert_eq!(read_container(&fx, "steps").default_switch_state, 11);
}
