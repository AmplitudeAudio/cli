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

//! Tests for `soundbank update` and the interactive walk-throughs of the soundbank commands.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::fixtures::{AssetTestFixture, CaptureOutput};
use common::scripted_input::ScriptedInput;
use common::soundbank_helpers::*;
use serde_json::json;

async fn fixture_with_bank(name: &str) -> AssetTestFixture {
    let fx = fixture(name);
    create_ok(&fx, &["--sound", "boom", "--collection", "crowd"]).await;
    fx
}

// ---------------------------------------------------------------------------
// Update through flags
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_adds_and_removes_by_list_flag() {
    let fx = fixture_with_bank("sb_up_edit").await;
    update_ok(&fx, &["--add-sound", "step", "--add-event", "hit", "--remove-collection", "crowd"])
        .await;
    // removing the only collection is fine: the bank still holds sounds and an event
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound", "fx/step.wav.amsound"]);
    assert_eq!(entries(&fx, "bank", "events"), ["hit.amevent"]);
    assert!(entries(&fx, "bank", "collections").is_empty());
    assert_flatc(&fx, "bank");
}

#[tokio::test]
async fn test_update_replaces_and_clears_lists() {
    let fx = fixture_with_bank("sb_up_replace").await;
    update_ok(&fx, &["--sound", "step", "--clear-collections", "--effect", "echo"]).await;
    assert_eq!(entries(&fx, "bank", "sounds"), ["fx/step.wav.amsound"]);
    assert!(entries(&fx, "bank", "collections").is_empty());
    assert_eq!(entries(&fx, "bank", "effects"), ["echo.amfx"]);
}

#[tokio::test]
async fn test_update_legacy_add_and_remove_specs_still_work() {
    let fx = fixture_with_bank("sb_up_legacy").await;
    update_ok(&fx, &["--add", "sound:step", "--remove", "sound:boom"]).await;
    assert_eq!(entries(&fx, "bank", "sounds"), ["fx/step.wav.amsound"]);
}

#[tokio::test]
async fn test_update_is_idempotent_for_present_assets() {
    let fx = fixture_with_bank("sb_up_idem").await;
    update_ok(&fx, &["--add-sound", "boom"]).await;
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound"]);
}

#[tokio::test]
async fn test_update_unknown_asset_fails_listing_names_and_writes_nothing() {
    let fx = fixture_with_bank("sb_up_unknown").await;
    let before = std::fs::read_to_string(bank_path(&fx, "bank")).unwrap();
    let (result, _) = run(&fx, &["update", "bank", "--add-event", "ghost"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    let text = format!("{err:#}");
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_VALIDATION_REFERENCE);
    assert!(text.contains("ghost") && text.contains("hit"), "{text}");
    assert_eq!(std::fs::read_to_string(bank_path(&fx, "bank")).unwrap(), before);
}

#[tokio::test]
async fn test_update_removing_a_missing_entry_fails_and_writes_nothing() {
    let fx = fixture_with_bank("sb_up_missing").await;
    let before = std::fs::read_to_string(bank_path(&fx, "bank")).unwrap();
    let (result, _) = run(&fx, &["update", "bank", "--remove-sound", "step"], &NonInteractiveInput).await;
    let text = format!("{:#}", result.unwrap_err());
    assert!(text.contains("not in the soundbank") && text.contains("boom.amsound"), "{text}");
    assert_eq!(std::fs::read_to_string(bank_path(&fx, "bank")).unwrap(), before);
}

#[tokio::test]
async fn test_update_cannot_empty_the_bank() {
    let fx = fixture("sb_up_empty");
    create_ok(&fx, &["--sound", "boom"]).await;
    let (result, _) = run(&fx, &["update", "bank", "--clear-sounds"], &NonInteractiveInput).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("no assets"));
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound"]);
}

#[tokio::test]
async fn test_update_conflicting_replace_flags_fail() {
    let fx = fixture_with_bank("sb_up_conflict").await;
    let (result, _) = run(
        &fx,
        &["update", "bank", "--sound", "step", "--clear-sounds"],
        &NonInteractiveInput,
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("Conflicting flags"));
}

#[tokio::test]
async fn test_update_from_json_replaces_only_the_lists_present() {
    let fx = fixture_with_bank("sb_up_json").await;
    let file = fx.project_root().join("lists.json");
    std::fs::write(&file, r#"{"sounds": ["step"]}"#).unwrap();
    update_ok(&fx, &["--from-json", file.to_str().unwrap()]).await;
    assert_eq!(entries(&fx, "bank", "sounds"), ["fx/step.wav.amsound"]);
    assert_eq!(entries(&fx, "bank", "collections"), ["crowd.amcollection"]);
}

#[tokio::test]
async fn test_update_keeps_unknown_keys_and_untouched_lists_verbatim() {
    let fx = fixture_with_bank("sb_up_unknown_keys").await;
    let mut raw = read_raw(&fx, "bank");
    let map = raw.as_object_mut().unwrap();
    map.insert("_a".into(), json!("note"));
    map.insert("_z".into(), json!({"keep": [1, 2]}));
    std::fs::write(bank_path(&fx, "bank"), serde_json::to_string_pretty(&raw).unwrap()).unwrap();
    let before = read_raw(&fx, "bank");

    update_ok(&fx, &["--add-event", "hit"]).await;

    let after = read_raw(&fx, "bank");
    assert_eq!(after["_a"], "note");
    assert_eq!(after["_z"], json!({"keep": [1, 2]}));
    for untouched in ["sounds", "collections", "switches", "rtpc", "id", "name"] {
        assert_eq!(after[untouched], before[untouched], "{untouched}");
    }
    assert_eq!(after["events"], json!(["hit.amevent"]));
    let keys: Vec<&String> = after.as_object().unwrap().keys().collect();
    let at = |k: &str| keys.iter().position(|x| x.as_str() == k).unwrap();
    assert!(at("sounds") < at("_a"), "unknown keys follow the modelled ones: {keys:?}");
    assert!(at("_a") < at("_z"), "unknown keys keep their relative order: {keys:?}");
}

#[tokio::test]
async fn test_update_repairs_a_bank_missing_lists() {
    let fx = fixture("sb_up_repair");
    std::fs::write(
        bank_path(&fx, "bank"),
        json!({"id": 9, "name": "bank", "sounds": ["boom.amsound"]}).to_string(),
    )
    .unwrap();
    // validation names the problem...
    update_ok(&fx, &["--add-event", "hit"]).await;
    // ...and update writes the lists the SDK dereferences
    let raw = read_raw(&fx, "bank");
    for field in ["collections", "switches", "switch_containers", "effects", "attenuators", "rtpc"] {
        assert_eq!(raw[field], json!([]), "{field}");
    }
    assert_eq!(raw["events"], json!(["hit.amevent"]));
    assert_flatc(&fx, "bank");
}

#[tokio::test]
async fn test_update_reports_the_changed_lists() {
    let fx = fixture_with_bank("sb_up_report").await;
    let (result, out) = run_with(
        &fx,
        &["update", "bank", "--add-event", "hit", "--clear-collections", "--add-sound", "step"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    let data = out.last_success().unwrap();
    assert_eq!(data["updated_fields"], json!(["sounds", "collections", "events"]));
}

#[tokio::test]
async fn test_update_of_a_missing_bank_fails() {
    let fx = fixture("sb_up_nobank");
    let (result, _) = run(&fx, &["update", "ghost", "--add-sound", "boom"], &NonInteractiveInput).await;
    assert_eq!(as_cli_error(&result.unwrap_err()).unwrap().code, codes::ERR_ASSET_NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Interactive walk-throughs
// ---------------------------------------------------------------------------

const NONE: &[&str] = &[];

#[tokio::test]
async fn test_interactive_create_picks_assets_type_by_type() {
    let fx = fixture("sb_walk_create");
    let input = ScriptedInput::new(&[], &[])
        .with_confirms(&[false]) // accept defaults?
        .with_multi_selects(&[
            &["boom", "step (fx/step.wav)"], // sounds
            &["crowd"],                      // collections
            &["hit"],                        // events
            NONE,                            // switches
            &["footsteps"],                  // switch containers
            NONE,                            // effects
            NONE,                            // attenuators
            &["wind"],                       // rtpc
        ]);
    let (result, _) = run(&fx, &["create", "bank"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound", "fx/step.wav.amsound"]);
    assert_eq!(entries(&fx, "bank", "events"), ["hit.amevent"]);
    assert_eq!(entries(&fx, "bank", "rtpc"), ["wind.amrtpc"]);
    assert!(entries(&fx, "bank", "effects").is_empty());
    assert!(input.saw_prompt("accept defaults"));
    assert!(input.saw_prompt("Switch containers to include"));
    assert_flatc(&fx, "bank");
}

#[tokio::test]
async fn test_interactive_create_accepting_defaults_stops_after_the_first_pick() {
    let fx = fixture("sb_walk_defaults");
    let input = ScriptedInput::new(&[], &[])
        .with_confirms(&[true])
        .with_multi_selects(&[&["boom"]]);
    let (result, _) = run(&fx, &["create", "bank"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound"]);
    assert!(entries(&fx, "bank", "collections").is_empty());
}

#[tokio::test]
async fn test_interactive_create_with_no_pick_fails_naming_the_flags() {
    let fx = fixture("sb_walk_nothing");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run(&fx, &["create", "bank"], &input).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--sound"));
    assert!(!bank_path(&fx, "bank").exists());
}

#[tokio::test]
async fn test_interactive_create_does_not_ask_for_what_flags_gave() {
    let fx = fixture("sb_walk_flags");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run(&fx, &["create", "bank", "--sound", "boom"], &input).await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_interactive_update_adds_and_removes() {
    let fx = fixture_with_bank("sb_walk_update").await;
    let input = ScriptedInput::new(
        &[
            "Events (0 in the soundbank)",
            "Add assets",
            "Collections (1 in the soundbank)",
            "Remove assets",
            "Done",
        ],
        &[],
    )
    .with_multi_selects(&[&["hit"], &["crowd.amcollection"]]);
    let (result, _) = run(&fx, &["update", "bank"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert_eq!(entries(&fx, "bank", "events"), ["hit.amevent"]);
    assert!(entries(&fx, "bank", "collections").is_empty());
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound"]);
    // assets already in the bank are not offered again
    let offered = input.options_seen.lock().unwrap().clone();
    assert!(offered.iter().all(|o| !o.contains(&"boom".to_string()) || o.len() > 1));
}

#[tokio::test]
async fn test_interactive_update_repairs_legacy_entries() {
    let fx = fixture("sb_walk_legacy");
    std::fs::write(
        bank_path(&fx, "bank"),
        json!({
            "id": 9, "name": "bank",
            "sounds": ["sounds/boom.json"], "collections": ["crowd.amcollection"],
            "events": [], "switches": [], "switch_containers": [], "effects": [],
            "attenuators": [], "rtpc": []
        })
        .to_string(),
    )
    .unwrap();
    // the legacy entry makes the bank invalid until it is removed
    let (result, _) = run(&fx, &["update", "bank", "--add-event", "hit"], &NonInteractiveInput).await;
    assert!(format!("{:#}", result.unwrap_err()).contains(".amsound"));

    let input = ScriptedInput::new(
        &["Sounds (1 in the soundbank)", "Remove assets", "Done"],
        &[],
    )
    .with_multi_selects(&[&["sounds/boom.json"]]);
    let (result, _) = run(&fx, &["update", "bank"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(entries(&fx, "bank", "sounds").is_empty());
    assert_eq!(entries(&fx, "bank", "collections"), ["crowd.amcollection"]);
}

#[tokio::test]
async fn test_interactive_update_not_asked_when_flags_are_given() {
    let fx = fixture_with_bank("sb_walk_flags_up").await;
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run(&fx, &["update", "bank", "--add-event", "hit"], &input).await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
}
