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

//! Tests for `soundbank create` and the flag surface of `soundbank create/update`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::assets::{Asset, ProjectContext, Soundbank};
use am::commands::asset::SoundbankCommands;
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::fixtures::CaptureOutput;
use common::scripted_input::ScriptedInput;
use common::soundbank_helpers::*;

// ---------------------------------------------------------------------------
// Flag parsing
// ---------------------------------------------------------------------------

#[test]
fn test_create_parses_every_list_flag() {
    match parse(&[
        "create", "b", "--sound", "a,b", "--sound", "c", "--collection", "d", "--event", "e",
        "--switch", "f", "--switch-container", "g", "--effect", "h", "--attenuator", "i",
        "--rtpc", "j", "--include", "sound:k", "--from-json", "l.json",
    ]) {
        SoundbankCommands::Create {
            lists,
            include,
            from_json,
            ..
        } => {
            assert_eq!(lists.sound, vec!["a", "b", "c"]);
            assert_eq!(lists.collection, vec!["d"]);
            assert_eq!(lists.event, vec!["e"]);
            assert_eq!(lists.switch, vec!["f"]);
            assert_eq!(lists.switch_container, vec!["g"]);
            assert_eq!(lists.effect, vec!["h"]);
            assert_eq!(lists.attenuator, vec!["i"]);
            assert_eq!(lists.rtpc, vec!["j"]);
            assert_eq!(include, vec!["sound:k"]);
            assert_eq!(from_json.unwrap().to_str(), Some("l.json"));
        }
        _ => panic!("expected create"),
    }
}

#[test]
fn test_update_parses_every_edit_flag() {
    match parse(&[
        "update", "b", "--add-sound", "a", "--add-collection", "a", "--add-event", "a",
        "--add-switch", "a", "--add-switch-container", "a", "--add-effect", "a",
        "--add-attenuator", "a", "--add-rtpc", "a", "--remove-sound", "r,s",
        "--remove-collection", "r", "--remove-event", "r", "--remove-switch", "r",
        "--remove-switch-container", "r", "--remove-effect", "r", "--remove-attenuator", "r",
        "--remove-rtpc", "r", "--clear-sounds", "--clear-collections", "--clear-events",
        "--clear-switches", "--clear-switch-containers", "--clear-effects",
        "--clear-attenuators", "--clear-rtpc", "-a", "sound:x", "-r", "sound:y",
    ]) {
        SoundbankCommands::Update { edit, add, remove, .. } => {
            assert_eq!(edit.add_sound, vec!["a"]);
            assert_eq!(edit.add_rtpc, vec!["a"]);
            assert_eq!(edit.remove_sound, vec!["r", "s"]);
            assert_eq!(edit.remove_switch_container, vec!["r"]);
            assert!(edit.clear_sounds && edit.clear_collections && edit.clear_events);
            assert!(edit.clear_switches && edit.clear_switch_containers && edit.clear_effects);
            assert!(edit.clear_attenuators && edit.clear_rtpc);
            assert_eq!(add, vec!["sound:x"]);
            assert_eq!(remove, vec!["sound:y"]);
        }
        _ => panic!("expected update"),
    }
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_with_every_list_flag_writes_runtime_names() {
    let fx = fixture("sb_create_all");
    create_ok(
        &fx,
        &[
            "--sound", "boom,step", "--collection", "crowd", "--event", "hit", "--switch",
            "surface", "--switch-container", "footsteps", "--effect", "echo", "--attenuator",
            "room", "--rtpc", "wind",
        ],
    )
    .await;
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound", "fx/step.wav.amsound"]);
    assert_eq!(entries(&fx, "bank", "collections"), ["crowd.amcollection"]);
    assert_eq!(entries(&fx, "bank", "events"), ["hit.amevent"]);
    assert_eq!(entries(&fx, "bank", "switches"), ["surface.amswitch"]);
    assert_eq!(entries(&fx, "bank", "switch_containers"), ["footsteps.amswitchcontainer"]);
    assert_eq!(entries(&fx, "bank", "effects"), ["echo.amfx"]);
    assert_eq!(entries(&fx, "bank", "attenuators"), ["room.amattenuation"]);
    assert_eq!(entries(&fx, "bank", "rtpc"), ["wind.amrtpc"]);
    assert_flatc(&fx, "bank");
}

#[tokio::test]
async fn test_created_bank_always_carries_all_eight_lists() {
    let fx = fixture("sb_create_lists");
    create_ok(&fx, &["--sound", "boom"]).await;
    let raw = read_raw(&fx, "bank");
    for field in [
        "sounds", "collections", "events", "switches", "switch_containers", "effects",
        "attenuators", "rtpc",
    ] {
        assert!(raw[field].is_array(), "{field} is missing: the SDK would crash on load");
    }
    assert!(raw["name"].is_string() && raw["id"].as_u64().unwrap_or(0) != 0);
}

#[tokio::test]
async fn test_created_bank_passes_validation_with_the_project() {
    let fx = fixture("sb_create_valid");
    create_ok(&fx, &["--sound", "boom,step", "--effect", "echo"]).await;
    let bank: Soundbank =
        serde_json::from_str(&std::fs::read_to_string(bank_path(&fx, "bank")).unwrap()).unwrap();
    let ctx = ProjectContext::new(fx.project_root().to_path_buf())
        .with_validator(fx.create_project_validator().unwrap());
    bank.validate_rules(&ctx).unwrap();
}

#[tokio::test]
async fn test_include_syntax_still_works_and_merges_with_flags() {
    let fx = fixture("sb_create_include");
    create_ok(
        &fx,
        &["--include", "sound:boom", "--include", "switch_container:footsteps", "--sound", "step"],
    )
    .await;
    assert_eq!(entries(&fx, "bank", "sounds"), ["fx/step.wav.amsound", "boom.amsound"]);
    assert_eq!(entries(&fx, "bank", "switch_containers"), ["footsteps.amswitchcontainer"]);
}

#[tokio::test]
async fn test_names_runtime_names_and_paths_are_equivalent() {
    let fx = fixture("sb_create_forms");
    create_ok(&fx, &["--sound", "boom,boom.amsound,fx/step.wav"]).await;
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound", "fx/step.wav.amsound"]);
}

#[tokio::test]
async fn test_unknown_name_is_a_reference_error_listing_available_names() {
    let fx = fixture("sb_create_unknown");
    let (result, _) =
        run(&fx, &["create", "bank", "--sound", "boom,ghost"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    let text = format!("{err:#}");
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_VALIDATION_REFERENCE);
    assert!(text.contains("ghost") && text.contains("boom") && text.contains("step"), "{text}");
    assert!(!bank_path(&fx, "bank").exists());
}

#[tokio::test]
async fn test_unknown_include_type_is_rejected() {
    let fx = fixture("sb_create_type");
    let (result, _) = run(&fx, &["create", "bank", "--include", "bus:master"], &NonInteractiveInput).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("Invalid asset type"));
    let (result, _) = run(&fx, &["create", "bank", "--include", "boom"], &NonInteractiveInput).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("Invalid include format"));
}

#[tokio::test]
async fn test_missing_assets_names_the_flags_and_writes_nothing() {
    let fx = fixture("sb_create_none");
    let (result, _) = run(&fx, &["create", "bank"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("--sound") && text.contains("--include"), "{text}");
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_VALIDATION_FIELD);
    assert!(!bank_path(&fx, "bank").exists());
}

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = fixture("sb_json_no_prompt");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run_with(&fx, &["create", "bank"], &input, CaptureOutput::json()).await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
    assert!(!bank_path(&fx, "bank").exists());

    create_ok(&fx, &["--sound", "boom"]).await;
    let before = std::fs::read_to_string(bank_path(&fx, "bank")).unwrap();
    let (result, _) = run_with(&fx, &["update", "bank"], &input, CaptureOutput::json()).await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(bank_path(&fx, "bank")).unwrap(), before);
}

#[tokio::test]
async fn test_creating_an_existing_bank_fails() {
    let fx = fixture("sb_create_twice");
    create_ok(&fx, &["--sound", "boom"]).await;
    let (result, _) = run(&fx, &["create", "bank", "--sound", "boom"], &NonInteractiveInput).await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_ASSET_ALREADY_EXISTS
    );
}

#[tokio::test]
async fn test_json_mode_reports_the_counts() {
    let fx = fixture("sb_create_report");
    let (result, out) = run_with(
        &fx,
        &["create", "bank", "--sound", "boom,step", "--event", "hit"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    let data = out.last_success().unwrap();
    assert_eq!(data["asset_count"], 3);
    assert_eq!(data["assets_by_type"]["sounds"], 2);
    assert_eq!(data["assets_by_type"]["events"], 1);
}

// ---------------------------------------------------------------------------
// --from-json
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_from_json_reads_lists_and_accepts_sdk_banks() {
    let fx = fixture("sb_from_json");
    let file = fx.project_root().join("lists.json");
    std::fs::write(
        &file,
        r#"{"id": 5, "name": "old", "sounds": ["boom", "step"], "events": ["hit"], "rtpc": []}"#,
    )
    .unwrap();
    create_ok(&fx, &["--from-json", file.to_str().unwrap()]).await;
    assert_eq!(entries(&fx, "bank", "sounds"), ["boom.amsound", "fx/step.wav.amsound"]);
    assert_eq!(entries(&fx, "bank", "events"), ["hit.amevent"]);
    assert_ne!(read_raw(&fx, "bank")["id"], 5);
    assert_flatc(&fx, "bank");
}

#[tokio::test]
async fn test_from_json_rejects_unknown_keys_and_bad_values() {
    let fx = fixture("sb_from_json_bad");
    let file = fx.project_root().join("lists.json");
    for (body, needle) in [
        (r#"{"sounds": ["boom"], "music": []}"#, "music"),
        (r#"{"sounds": "boom"}"#, "list of names"),
        (r#"{"sounds": [1]}"#, "not a name"),
        (r#"["boom"]"#, "object"),
        ("{", "not valid JSON"),
    ] {
        std::fs::write(&file, body).unwrap();
        let (result, _) = run(
            &fx,
            &["create", "bank", "--from-json", file.to_str().unwrap()],
            &NonInteractiveInput,
        )
        .await;
        let text = format!("{:#}", result.unwrap_err());
        assert!(text.contains(needle), "{body}: {text}");
    }
    assert!(!bank_path(&fx, "bank").exists());
}

#[tokio::test]
async fn test_from_json_conflicts_with_the_flag_of_the_same_list() {
    let fx = fixture("sb_from_json_conflict");
    let file = fx.project_root().join("lists.json");
    std::fs::write(&file, r#"{"sounds": ["boom"]}"#).unwrap();
    let (result, _) = run(
        &fx,
        &["create", "bank", "--sound", "step", "--from-json", file.to_str().unwrap()],
        &NonInteractiveInput,
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("Conflicting flags"));
    // different lists do not conflict
    let (result, _) = run(
        &fx,
        &["create", "bank", "--event", "hit", "--from-json", file.to_str().unwrap()],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
}
