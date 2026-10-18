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

//! Tests for the flags and validation of `collection create`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::Path;

use am::assets::generated::{
    CollectionEntry, RandomSoundSchedulerConfig, Scope, SequenceSoundSchedulerConfig,
    SequenceSoundSchedulerEndBehavior, SoundSchedulerConfig, ValueKind,
};
use am::commands::asset::CollectionCommands;
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::collection_helpers::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// Clap surface
// ---------------------------------------------------------------------------

#[test]
fn test_create_parses_every_new_flag() {
    let cmd = parse(&[
        "create",
        "steps",
        "--bus",
        "sfx",
        "--effect",
        "echo",
        "--attenuation",
        "short",
        "--scope",
        "entity",
        "--fader",
        "EaseIn",
        "--pitch",
        "1.2",
        "--gain-rtpc",
        "g",
        "--pitch-rtpc",
        "p",
        "--priority-rtpc",
        "pr",
        "--avoid-repeat",
        "true",
        "--repeat-count",
        "3",
        "--end-behavior",
        "reverse",
        "--entry",
        "step1,weight=2",
        "--entry",
        "step2",
        "--entries-from-json",
        "entries.json",
    ]);
    let CollectionCommands::Create { bus, extra, .. } = cmd else {
        panic!("expected create");
    };
    assert_eq!(bus.as_deref(), Some("sfx"));
    assert_eq!(extra.effect.as_deref(), Some("echo"));
    assert_eq!(extra.attenuation.as_deref(), Some("short"));
    assert_eq!(extra.scope.as_deref(), Some("entity"));
    assert_eq!(extra.fader.as_deref(), Some("EaseIn"));
    assert_eq!(extra.pitch, Some(1.2));
    assert_eq!(extra.gain_rtpc.as_deref(), Some("g"));
    assert_eq!(extra.pitch_rtpc.as_deref(), Some("p"));
    assert_eq!(extra.priority_rtpc.as_deref(), Some("pr"));
    assert_eq!(extra.avoid_repeat, Some(true));
    assert_eq!(extra.repeat_count, Some(3));
    assert_eq!(extra.end_behavior.as_deref(), Some("reverse"));
    assert_eq!(extra.entry, vec!["step1,weight=2", "step2"]);
    assert_eq!(
        extra.entries_from_json.as_deref(),
        Some(Path::new("entries.json"))
    );
}

#[test]
fn test_avoid_repeat_without_a_value_means_true() {
    let CollectionCommands::Create { extra, .. } = parse(&["create", "steps", "--avoid-repeat"])
    else {
        panic!("expected create");
    };
    assert_eq!(extra.avoid_repeat, Some(true));
}

#[test]
fn test_update_parses_entry_editing_flags() {
    let cmd = parse(&[
        "update",
        "steps",
        "--add-entry",
        "step1",
        "--add-entry",
        "step2,weight=3",
        "--remove-entry",
        "step1",
        "--clear-entries",
        "--entry",
        "step2",
        "--effect",
        "none",
    ]);
    let CollectionCommands::Update { extra, edit, .. } = cmd else {
        panic!("expected update");
    };
    assert_eq!(edit.add_entry, vec!["step1", "step2,weight=3"]);
    assert_eq!(edit.remove_entry, vec!["step1"]);
    assert!(edit.clear_entries);
    assert_eq!(extra.entry, vec!["step2"]);
    assert_eq!(extra.effect.as_deref(), Some("none"));
}

// ---------------------------------------------------------------------------
// Non-interactive create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_random_with_config_and_entries_is_accepted_by_flatc() {
    let fx = fixture("col_create_random");
    create_ok(
        &fx,
        &[
            "--bus",
            "sfx",
            "--effect",
            "echo",
            "--attenuation",
            "short",
            "--scope",
            "entity",
            "--fader",
            "easein",
            "--pitch",
            "1.2",
            "--gain-rtpc",
            "speed",
            "--avoid-repeat",
            "true",
            "--repeat-count",
            "3",
            "--entry",
            "step1,weight=2.5,gain=0.5,pitch-rtpc=speed",
            "--entry",
            "102",
        ],
    )
    .await;

    let c = read_collection(&fx, "steps");
    assert_eq!(c.bus, 2);
    assert_eq!(c.effect, 66);
    assert_eq!(c.attenuation, 55);
    assert_eq!(c.scope, Scope::Entity);
    assert_eq!(c.fader.as_deref(), Some("EaseIn"));
    assert_eq!(c.pitch.as_ref().and_then(|p| p.as_static()), Some(1.2));
    let gain = c.gain.unwrap();
    assert_eq!(gain.kind, ValueKind::RTPC);
    assert_eq!(gain.rtpc.unwrap().id, 77);
    assert_eq!(
        c.scheduler.unwrap().config,
        Some(SoundSchedulerConfig::Random(RandomSoundSchedulerConfig {
            avoid_repeat: true,
            repeat_count: 3,
        }))
    );
    let sounds = c.sounds.unwrap();
    let CollectionEntry::Random(first) = &sounds[0] else {
        panic!("expected a Random entry");
    };
    assert_eq!(first.sound, 101);
    assert_eq!(first.weight, 2.5);
    assert_eq!(first.gain.as_ref().and_then(|g| g.as_static()), Some(0.5));
    assert_eq!(first.pitch.as_ref().unwrap().rtpc.as_ref().unwrap().id, 77);
    assert!(matches!(&sounds[1], CollectionEntry::Random(e) if e.sound == 102 && e.weight == 1.0));

    let raw = read_raw(&fx, "steps");
    assert_eq!(raw["sounds_type"], json!(["Random", "Random"]));
    assert_eq!(raw["scheduler"]["config_type"], "Random");
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_create_sequence_with_end_behavior_and_entries_is_accepted_by_flatc() {
    let fx = fixture("col_create_seq");
    create_ok(
        &fx,
        &[
            "--bus",
            "sfx",
            "--scheduler-mode",
            "Sequence",
            "--end-behavior",
            "Reverse",
            "--entry",
            "step1",
            "--entry",
            "step2,pitch=0.9",
        ],
    )
    .await;
    let c = read_collection(&fx, "steps");
    assert_eq!(
        c.scheduler.unwrap().config,
        Some(SoundSchedulerConfig::Sequence(
            SequenceSoundSchedulerConfig {
                end_behavior: SequenceSoundSchedulerEndBehavior::Reverse,
            }
        ))
    );
    let sounds = c.sounds.unwrap();
    assert!(
        sounds
            .iter()
            .all(|e| matches!(e, CollectionEntry::Sequence(_)))
    );
    let raw = read_raw(&fx, "steps");
    assert_eq!(raw["sounds_type"], json!(["Sequence", "Sequence"]));
    assert_eq!(raw["scheduler"]["config_type"], "Sequence");
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_create_without_flags_for_the_new_fields_keeps_the_old_shape() {
    let fx = fixture("col_create_plain");
    create_ok(&fx, &["--bus", "sfx"]).await;
    let c = read_collection(&fx, "steps");
    assert_eq!(c.scheduler.unwrap().config, None);
    assert!(c.sounds.unwrap().is_empty());
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_entries_from_json_accepts_a_list_of_objects() {
    let fx = fixture("col_entries_json");
    let file = fx.project_root().join("entries.json");
    std::fs::write(
        &file,
        json!([
            {"sound": "step1", "weight": 4.0, "gain": 0.5},
            {"sound": 102, "pitch": {"kind": "Static", "value": 1.5}}
        ])
        .to_string(),
    )
    .unwrap();
    create_ok(
        &fx,
        &[
            "--bus",
            "sfx",
            "--entries-from-json",
            file.to_str().unwrap(),
        ],
    )
    .await;
    let sounds = read_collection(&fx, "steps").sounds.unwrap();
    let CollectionEntry::Random(first) = &sounds[0] else {
        panic!("expected Random entry");
    };
    assert_eq!((first.sound, first.weight), (101, 4.0));
    assert_eq!(first.gain.as_ref().and_then(|g| g.as_static()), Some(0.5));
    let CollectionEntry::Random(second) = &sounds[1] else {
        panic!("expected Random entry");
    };
    assert_eq!(second.pitch.as_ref().and_then(|g| g.as_static()), Some(1.5));
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_entries_from_json_accepts_the_flatc_union_form() {
    let fx = fixture("col_entries_flatc_json");
    let file = fx.project_root().join("entries.json");
    std::fs::write(
        &file,
        json!({
            "sounds_type": ["Default", "Random"],
            "sounds": [{"sound": 101}, {"sound": 102, "weight": 2.0}]
        })
        .to_string(),
    )
    .unwrap();
    create_ok(
        &fx,
        &[
            "--bus",
            "sfx",
            "--entries-from-json",
            file.to_str().unwrap(),
        ],
    )
    .await;
    let sounds = read_collection(&fx, "steps").sounds.unwrap();
    assert!(matches!(&sounds[0], CollectionEntry::Random(e) if e.sound == 101));
    assert!(matches!(&sounds[1], CollectionEntry::Random(e) if e.sound == 102 && e.weight == 2.0));
}

#[tokio::test]
async fn test_entries_from_json_rejects_unknown_keys_and_unknown_sounds() {
    let fx = fixture("col_entries_json_bad");
    let file = fx.project_root().join("entries.json");
    std::fs::write(&file, json!([{"sound": "step1", "volume": 1}]).to_string()).unwrap();
    let (result, _) = run(
        &fx,
        &[
            "create",
            "steps",
            "--bus",
            "sfx",
            "--entries-from-json",
            file.to_str().unwrap(),
        ],
        &NonInteractiveInput,
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("volume"));

    std::fs::write(&file, json!([{"sound": "nope"}]).to_string()).unwrap();
    let (result, _) = run(
        &fx,
        &[
            "create",
            "steps",
            "--bus",
            "sfx",
            "--entries-from-json",
            file.to_str().unwrap(),
        ],
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
async fn test_entry_flag_and_entries_from_json_conflict() {
    let fx = fixture("col_entries_conflict");
    let file = fx.project_root().join("entries.json");
    std::fs::write(&file, "[]").unwrap();
    let (result, _) = run(
        &fx,
        &[
            "create",
            "steps",
            "--bus",
            "sfx",
            "--entry",
            "step1",
            "--entries-from-json",
            file.to_str().unwrap(),
        ],
        &NonInteractiveInput,
    )
    .await;
    assert!(result.is_err());
    assert!(!collection_path(&fx, "steps").exists());
}

#[tokio::test]
async fn test_entry_syntax_errors_name_the_problem() {
    let fx = fixture("col_entry_syntax");
    for bad in [
        "step1,weight=abc",
        "step1,loudness=3",
        "step1,gain=0.5,gain-rtpc=speed",
        "step1,weight",
        "",
    ] {
        let (result, _) = run(
            &fx,
            &["create", "steps", "--bus", "sfx", "--entry", bad],
            &NonInteractiveInput,
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(
            as_cli_error(&err).unwrap().code,
            codes::ERR_VALIDATION_FIELD,
            "{bad}"
        );
        assert!(!collection_path(&fx, "steps").exists(), "{bad}");
    }
}
