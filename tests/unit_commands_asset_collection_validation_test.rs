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

//! Tests for the required fields and reference validation of `collection create`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::collection_helpers::*;
use common::fixtures::CaptureOutput;
use common::scripted_input::ScriptedInput;
use serde_json::json;

// ---------------------------------------------------------------------------
// Required and reference validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_missing_bus_names_the_flag_and_writes_nothing() {
    let fx = fixture("col_no_bus");
    let (result, _) = run(&fx, &["create", "steps"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("--bus"));
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_EMPTY_REFERENCE
    );
    assert!(!collection_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_create_numeric_zero_and_unknown_bus_are_rejected() {
    let fx = fixture("col_bad_bus");
    let (zero, _) = run(
        &fx,
        &["create", "steps", "--bus", "0"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&zero.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_EMPTY_REFERENCE
    );
    let (unknown, _) = run(
        &fx,
        &["create", "steps", "--bus", "nope"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&unknown.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    let (dangling, _) = run(
        &fx,
        &["create", "steps", "--bus", "999"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&dangling.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert!(!collection_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_dangling_entry_sound_id_is_a_reference_error() {
    let fx = fixture("col_dangling_entry");
    let (result, _) = run(
        &fx,
        &["create", "steps", "--bus", "sfx", "--entry", "999"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert!(!collection_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_zero_entry_sound_id_is_rejected() {
    let fx = fixture("col_zero_entry");
    let (result, _) = run(
        &fx,
        &["create", "steps", "--bus", "sfx", "--entry", "0"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_EMPTY_REFERENCE
    );
}

#[tokio::test]
async fn test_unknown_effect_and_attenuation_names_list_what_exists() {
    let fx = fixture("col_unknown_refs");
    let (effect, _) = run(
        &fx,
        &["create", "steps", "--bus", "sfx", "--effect", "nope"],
        &NonInteractiveInput,
    )
    .await;
    let err = effect.unwrap_err();
    let cli = as_cli_error(&err).unwrap();
    assert_eq!(cli.code, codes::ERR_VALIDATION_REFERENCE);
    assert!(cli.suggestion.contains("echo"), "{}", cli.suggestion);

    let (att, _) = run(
        &fx,
        &["create", "steps", "--bus", "sfx", "--attenuation", "nope"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&att.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
}

#[tokio::test]
async fn test_invalid_enums_and_numbers_are_rejected() {
    let fx = fixture("col_invalid_values");
    for extra in [
        vec!["--scope", "galaxy"],
        vec!["--fader", "zigzag"],
        vec!["--pitch", "NaN"],
        vec!["--end-behavior", "loop"],
        vec!["--gain", "1.5"],
        vec!["--pitch", "1.0", "--pitch-rtpc", "speed"],
    ] {
        let mut args = vec!["create", "steps", "--bus", "sfx"];
        args.extend(extra.iter().copied());
        let (result, _) = run(&fx, &args, &NonInteractiveInput).await;
        assert!(result.is_err(), "{extra:?}");
        assert!(!collection_path(&fx, "steps").exists(), "{extra:?}");
    }
}

#[tokio::test]
async fn test_config_flag_of_the_other_scheduler_and_weight_on_sequence_are_rejected() {
    let fx = fixture("col_mismatch");
    let (result, _) = run(
        &fx,
        &[
            "create",
            "steps",
            "--bus",
            "sfx",
            "--scheduler-mode",
            "Sequence",
            "--avoid-repeat",
            "true",
        ],
        &NonInteractiveInput,
    )
    .await;
    assert!(
        format!("{:#}", result.unwrap_err())
            .to_lowercase()
            .contains("random")
    );

    let (result, _) = run(
        &fx,
        &[
            "create",
            "steps",
            "--bus",
            "sfx",
            "--scheduler-mode",
            "Sequence",
            "--entry",
            "step1,weight=2",
        ],
        &NonInteractiveInput,
    )
    .await;
    assert!(
        format!("{:#}", result.unwrap_err())
            .to_lowercase()
            .contains("weight")
    );
    assert!(!collection_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = fixture("col_json_no_prompt");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run_with(&fx, &["create", "steps"], &input, CaptureOutput::json()).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--bus"));
    assert!(input.prompts.lock().unwrap().is_empty());
    assert!(!collection_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_bus_outside_the_active_bus_file_warns() {
    let fx = fixture("col_inactive_bus");
    fx.write_buses_file("extra.buses.json", json!([{"id": 3, "name": "ambient"}]))
        .unwrap();
    std::fs::write(
        fx.project_root().join("pc.config.json"),
        json!({"buses_file": "main.buses.json"}).to_string(),
    )
    .unwrap();

    let (result, out) = run_with(
        &fx,
        &["create", "steps", "--bus", "ambient"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert_eq!(read_collection(&fx, "steps").bus, 3);
    let warnings = out.all_warnings();
    assert!(
        warnings.iter().any(|w| w.contains("main.buses.json")),
        "{warnings:?}"
    );
    let data = out.last_success().unwrap();
    assert!(data["warnings"][0].as_str().unwrap().contains("ambient"));
}
