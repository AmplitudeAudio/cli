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

//! Tests for `collection update` and `collection list` with the full collection field surface.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::assets::generated::{
    CollectionEntry, Scope, SequenceSoundSchedulerConfig, SequenceSoundSchedulerEndBehavior,
    SoundSchedulerConfig, SoundSchedulerMode,
};
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::collection_helpers::*;
use common::fixtures::CaptureOutput;
use serde_json::json;

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_changes_only_the_provided_fields() {
    let fx = fixture("col_update_partial");
    rich_collection(&fx).await;
    let before = read_collection(&fx, "steps");

    let (result, out) = run_with(
        &fx,
        &[
            "update", "steps", "--effect", "echo", "--scope", "entity", "--pitch", "0.9",
        ],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    let after = read_collection(&fx, "steps");

    let mut expected = before.clone();
    expected.effect = 66;
    expected.scope = Scope::Entity;
    expected.pitch = after.pitch.clone();
    assert_eq!(after, expected);
    assert_eq!(after.pitch.as_ref().and_then(|p| p.as_static()), Some(0.9));
    assert!(after.scheduler.unwrap().config.is_some(), "config survives");

    let fields = out.last_success().unwrap()["updated_fields"].clone();
    assert_eq!(fields, json!(["effect", "pitch", "scope"]));
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_update_keeps_scheduler_config_when_setting_the_scheduler_mode_to_the_same_value() {
    let fx = fixture("col_update_same_mode");
    rich_collection(&fx).await;
    let before = read_collection(&fx, "steps");
    let (result, _) = run(
        &fx,
        &["update", "steps", "--scheduler-mode", "Random"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert_eq!(read_collection(&fx, "steps"), before);
}

#[tokio::test]
async fn test_update_preserves_unknown_keys_and_hand_edited_config() {
    let fx = fixture("col_update_unknown");
    rich_collection(&fx).await;
    let mut raw = read_raw(&fx, "steps");
    raw["x_comment"] = json!("hand written");
    raw["x_extra"] = json!({"a": [1, 2]});
    std::fs::write(
        collection_path(&fx, "steps"),
        serde_json::to_string_pretty(&raw).unwrap(),
    )
    .unwrap();

    let (result, _) = run(
        &fx,
        &["update", "steps", "--gain", "0.7"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();

    let after = read_raw(&fx, "steps");
    assert_eq!(after["x_comment"], "hand written");
    assert_eq!(after["x_extra"], json!({"a": [1, 2]}));
    assert_eq!(after["scheduler"], raw["scheduler"]);
    assert_eq!(after["sounds"], raw["sounds"]);
    assert_eq!(after["sounds_type"], raw["sounds_type"]);
    assert!((after["gain"]["value"].as_f64().unwrap() - 0.7).abs() < 1e-6);
    let keys: Vec<&String> = after.as_object().unwrap().keys().collect();
    let known_first = keys.iter().position(|k| k.as_str() == "bus").unwrap();
    let unknown = keys.iter().position(|k| k.as_str() == "x_comment").unwrap();
    assert!(
        known_first < unknown,
        "unknown keys follow the modelled ones: {keys:?}"
    );
}

#[tokio::test]
async fn test_update_mode_change_retags_entries_and_drops_the_old_config() {
    let fx = fixture("col_update_mode");
    rich_collection(&fx).await;
    let (result, _) = run(
        &fx,
        &[
            "update",
            "steps",
            "--scheduler-mode",
            "Sequence",
            "--end-behavior",
            "reverse",
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let c = read_collection(&fx, "steps");
    let scheduler = c.scheduler.unwrap();
    assert_eq!(scheduler.mode, SoundSchedulerMode::Sequence);
    assert_eq!(
        scheduler.config,
        Some(SoundSchedulerConfig::Sequence(
            SequenceSoundSchedulerConfig {
                end_behavior: SequenceSoundSchedulerEndBehavior::Reverse,
            }
        ))
    );
    let sounds = c.sounds.unwrap();
    assert!(matches!(&sounds[0], CollectionEntry::Sequence(e) if e.sound == 101));
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_update_edits_entries() {
    let fx = fixture("col_update_entries");
    create_ok(&fx, &["--bus", "sfx"]).await;

    let (r, _) = run(
        &fx,
        &[
            "update",
            "steps",
            "--add-entry",
            "step1",
            "--add-entry",
            "step2,weight=2",
        ],
        &NonInteractiveInput,
    )
    .await;
    r.unwrap();
    assert_eq!(read_collection(&fx, "steps").sounds.unwrap().len(), 2);

    let (r, _) = run(
        &fx,
        &["update", "steps", "--remove-entry", "step1"],
        &NonInteractiveInput,
    )
    .await;
    r.unwrap();
    let sounds = read_collection(&fx, "steps").sounds.unwrap();
    assert_eq!(sounds.len(), 1);
    assert!(matches!(&sounds[0], CollectionEntry::Random(e) if e.sound == 102 && e.weight == 2.0));

    let (r, _) = run(
        &fx,
        &["update", "steps", "--entry", "step1", "--entry", "step2"],
        &NonInteractiveInput,
    )
    .await;
    r.unwrap();
    assert_eq!(read_collection(&fx, "steps").sounds.unwrap().len(), 2);

    let (r, _) = run(
        &fx,
        &["update", "steps", "--clear-entries"],
        &NonInteractiveInput,
    )
    .await;
    r.unwrap();
    assert!(read_collection(&fx, "steps").sounds.unwrap().is_empty());
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_update_dangling_entry_leaves_the_file_alone() {
    let fx = fixture("col_update_dangling");
    rich_collection(&fx).await;
    let before = std::fs::read_to_string(collection_path(&fx, "steps")).unwrap();
    let (result, _) = run(
        &fx,
        &["update", "steps", "--add-entry", "999"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert_eq!(
        std::fs::read_to_string(collection_path(&fx, "steps")).unwrap(),
        before
    );
}

#[tokio::test]
async fn test_update_can_clear_optional_references_with_none() {
    let fx = fixture("col_update_clear");
    create_ok(
        &fx,
        &["--bus", "sfx", "--effect", "echo", "--attenuation", "short"],
    )
    .await;
    let (r, _) = run(
        &fx,
        &[
            "update",
            "steps",
            "--effect",
            "none",
            "--attenuation",
            "none",
        ],
        &NonInteractiveInput,
    )
    .await;
    r.unwrap();
    let c = read_collection(&fx, "steps");
    assert_eq!((c.effect, c.attenuation), (0, 0));
}

#[tokio::test]
async fn test_update_fixes_a_legacy_bus_zero_file_only_when_given_a_bus() {
    let fx = fixture("col_update_legacy_bus");
    fx.create_test_collection("legacy", 5000, &[]).unwrap();

    let (result, _) = run(
        &fx,
        &["update", "legacy", "--gain", "0.5"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_EMPTY_REFERENCE
    );

    let (result, _) = run(
        &fx,
        &["update", "legacy", "--bus", "sfx", "--gain", "0.5"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert_eq!(read_collection(&fx, "legacy").bus, 2);
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_reports_the_entry_count() {
    let fx = fixture("col_list_count");
    rich_collection(&fx).await;
    let (result, out) = run_with(&fx, &["list"], &NonInteractiveInput, CaptureOutput::json()).await;
    result.unwrap();
    assert_eq!(
        out.last_success().unwrap()["collections"][0]["entry_count"],
        2
    );
}
