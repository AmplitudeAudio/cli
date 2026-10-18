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

//! Tests for update round-trips: unknown top-level keys in hand-edited asset
//! files survive `update`, while known fields follow the typed update.
//!
//! The command-level tests change the process working directory; run with
//! `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};

use am::app::{App, Commands};
use am::assets::AssetType;
use am::commands::asset::{
    AssetCommands, AttenuationCommands, RtpcCommands, SoundCommands, handle_attenuation_command,
    handle_rtpc_command, handle_sound_command,
};
use am::common::json_merge::{merge_update, merge_update_unknown_only};
use am::forms::coverage::known_field_names;
use am::input::NonInteractiveInput;
use clap::Parser;
use common::fixtures::{AssetTestFixture, CaptureOutput};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// merge_update: pure unit tests
// ---------------------------------------------------------------------------

#[test]
fn test_merge_update_carries_over_unknown_top_level_key() {
    let existing = json!({"id": 1, "name": "a", "x_custom": {"a": 1}});
    let updated = json!({"id": 1, "name": "b"});

    let merged = merge_update(&existing, &updated);

    assert_eq!(merged, json!({"id": 1, "name": "b", "x_custom": {"a": 1}}));
}

#[test]
fn test_merge_update_updated_value_wins_for_shared_keys() {
    let existing = json!({"gain": 1.0});
    let updated = json!({"gain": 0.5});

    assert_eq!(merge_update(&existing, &updated), json!({"gain": 0.5}));
}

#[test]
fn test_merge_update_keeps_every_updated_key_then_carried_keys() {
    let existing = json!({"a": 1, "x": 2, "y": 3});
    let updated = json!({"a": 9, "c": 4});

    let merged = merge_update(&existing, &updated);
    let object = merged.as_object().unwrap();

    // Every updated key is present with its updated value, and each carried-over
    // key appears exactly once.
    assert_eq!(object.get("a"), Some(&json!(9)));
    assert_eq!(object.get("c"), Some(&json!(4)));
    assert_eq!(object.get("x"), Some(&json!(2)));
    assert_eq!(object.get("y"), Some(&json!(3)));
    assert_eq!(object.len(), 4);
}

#[test]
fn test_merge_update_non_objects_return_updated() {
    assert_eq!(merge_update(&json!(1), &json!("x")), json!("x"));
    assert_eq!(merge_update(&json!({"a": 1}), &json!(5)), json!(5));
    assert_eq!(merge_update(&json!([1]), &json!({"a": 1})), json!({"a": 1}));
}

#[test]
fn test_merge_update_does_not_mutate_inputs() {
    let existing = json!({"a": 1, "x": {"k": true}});
    let updated = json!({"a": 2});
    let existing_before = existing.clone();
    let updated_before = updated.clone();

    let _ = merge_update(&existing, &updated);

    assert_eq!(existing, existing_before);
    assert_eq!(updated, updated_before);
}

#[test]
fn test_merge_update_does_not_carry_nested_unknown_keys() {
    let existing = json!({"shape": {"Box": {"w": 1}, "extra": 2}});
    let updated = json!({"shape": {"Sphere": {"r": 3}}});

    let merged = merge_update(&existing, &updated);

    assert_eq!(merged, json!({"shape": {"Sphere": {"r": 3}}}));
}

// ---------------------------------------------------------------------------
// merge_update_unknown_only: known fields follow the typed update
// ---------------------------------------------------------------------------

#[test]
fn test_unknown_only_carries_unknown_keys_and_updates_known_ones() {
    let existing = json!({"effect": 66, "gain": 0.4, "x_custom": 1});
    let updated = json!({"effect": 0, "gain": 0.5});

    let merged = merge_update_unknown_only(&existing, &updated, &["effect", "gain"]);

    assert_eq!(merged, json!({"effect": 0, "gain": 0.5, "x_custom": 1}));
}

#[test]
fn test_unknown_only_does_not_resurrect_a_cleared_known_field() {
    // The update cleared `fade`, so the typed serialization omits it. The old
    // value must not come back from the original file.
    let existing = json!({"fade": {"duration": 2}, "x_custom": 1});
    let updated = json!({});

    let merged = merge_update_unknown_only(&existing, &updated, &["fade"]);

    assert_eq!(merged, json!({"x_custom": 1}));
}

#[test]
fn test_unknown_only_does_not_resurrect_stale_union_tag() {
    // Changing a union variant rewrites the payload; the old tag must not survive.
    let existing = json!({"shape": {"Box": {"w": 1}}, "shape_type": "Box"});
    let updated = json!({"shape": {"Sphere": {"r": 3}}, "shape_type": "Sphere"});

    let merged = merge_update_unknown_only(&existing, &updated, &["shape", "shape_type"]);

    assert_eq!(merged, updated);
}

#[test]
fn test_known_field_names_cover_sound_fields_and_not_custom_keys() {
    let names = known_field_names(AssetType::Sound);

    assert!(names.iter().any(|n| n == "gain"));
    assert!(names.iter().any(|n| n == "effect"));
    assert!(!names.iter().any(|n| n == "x_custom"));
}

// ---------------------------------------------------------------------------
// Command-level round-trips
// ---------------------------------------------------------------------------

/// Restores the working directory when dropped.
struct CwdGuard(Option<PathBuf>);

impl CwdGuard {
    fn enter(path: &Path) -> Self {
        let previous = std::env::current_dir().ok();
        std::env::set_current_dir(path).unwrap();
        Self(previous)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        if let Some(dir) = self.0.take() {
            let _ = std::env::set_current_dir(dir);
        }
    }
}

fn sound_command(args: &[&str]) -> SoundCommands {
    let mut full = vec!["am", "asset", "sound"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Sound { command },
        } => command,
        _ => panic!("expected a sound command"),
    }
}

fn attenuation_command(args: &[&str]) -> AttenuationCommands {
    let mut full = vec!["am", "asset", "attenuation"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Attenuation { command },
        } => command,
        _ => panic!("expected an attenuation command"),
    }
}

fn rtpc_command(args: &[&str]) -> RtpcCommands {
    let mut full = vec!["am", "asset", "rtpc"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Rtpc { command },
        } => command,
        _ => panic!("expected an rtpc command"),
    }
}

fn sound_fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.create_data_file("a.wav").unwrap();
    fx
}

/// Adds an unknown top-level key to an asset file, as a hand edit would.
fn inject_unknown(path: &Path, key: &str, value: Value) {
    let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc.as_object_mut().unwrap().insert(key.to_string(), value);
    std::fs::write(path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
}

fn read_value(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

#[tokio::test]
async fn test_sound_update_keeps_unknown_field_and_changes_gain() {
    let fx = sound_fixture("roundtrip_sound");
    let _cwd = CwdGuard::enter(fx.project_root());
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    handle_sound_command(
        &sound_command(&[
            "create", "boom", "--file", "a.wav", "--bus", "sfx", "--gain", "0.4",
        ]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let path = fx.project_root().join("sources/sounds/boom.json");
    inject_unknown(&path, "x_custom", json!({"a": 1}));
    let before = read_value(&path);

    handle_sound_command(
        &sound_command(&["update", "boom", "--gain", "0.5"]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let after = read_value(&path);

    assert_eq!(after["x_custom"], json!({"a": 1}));
    assert_eq!(after["gain"]["value"], json!(0.5));
    assert_eq!(sorted_keys(&after), sorted_keys(&before));
}

#[tokio::test]
async fn test_attenuation_update_keeps_unknown_field_and_changes_max_distance() {
    let fx = AssetTestFixture::new("roundtrip_attenuation").unwrap();
    let _cwd = CwdGuard::enter(fx.project_root());
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    handle_attenuation_command(
        &attenuation_command(&[
            "create",
            "short",
            "--max-distance",
            "50",
            "--shape",
            "sphere",
            "--inner-radius",
            "5",
            "--outer-radius",
            "10",
            "--curve-part",
            "0,1,1,0,Linear",
            "--curve-part",
            "1,0,2,0,Linear",
        ]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let path = fx.project_root().join("sources/attenuators/short.json");
    inject_unknown(&path, "x_custom", json!({"a": 1}));
    let before = read_value(&path);

    handle_attenuation_command(
        &attenuation_command(&["update", "short", "--max-distance", "80"]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let after = read_value(&path);

    assert_eq!(after["x_custom"], json!({"a": 1}));
    assert_eq!(after["max_distance"], json!(80.0));
    assert_eq!(sorted_keys(&after), sorted_keys(&before));
}

#[tokio::test]
async fn test_rtpc_update_keeps_unknown_field_and_changes_default_value() {
    let fx = AssetTestFixture::new("roundtrip_rtpc").unwrap();
    let _cwd = CwdGuard::enter(fx.project_root());
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    handle_rtpc_command(
        &rtpc_command(&["create", "speed", "--min-value", "0", "--max-value", "10"]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let path = fx.project_root().join("sources/rtpc/speed.json");
    inject_unknown(&path, "x_custom", json!({"a": 1}));
    let before = read_value(&path);

    handle_rtpc_command(
        &rtpc_command(&["update", "speed", "--default-value", "3"]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let after = read_value(&path);

    assert_eq!(after["x_custom"], json!({"a": 1}));
    assert_eq!(after["default_value"], json!(3.0));
    assert_eq!(sorted_keys(&after), sorted_keys(&before));
}

// ---------------------------------------------------------------------------
// Key order: updates keep the order that create writes
// ---------------------------------------------------------------------------

fn keys_in_order(value: &Value) -> Vec<String> {
    value.as_object().unwrap().keys().cloned().collect()
}

#[test]
fn test_merge_update_orders_updated_keys_first_then_carried_keys() {
    // Doc-comment promise: updated's keys in updated's order, then the carried-over
    // keys of existing in existing's order.
    let existing = json!({"z": 1, "x": 2, "y": 3, "a": 4});
    let updated = json!({"b": 9, "a": 8, "c": 7});

    let merged = merge_update(&existing, &updated);

    assert_eq!(keys_in_order(&merged), vec!["b", "a", "c", "z", "x", "y"]);
}

#[tokio::test]
async fn test_sound_update_keeps_create_key_order_with_unknown_key_last() {
    let fx = sound_fixture("order_sound");
    let _cwd = CwdGuard::enter(fx.project_root());
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    handle_sound_command(
        &sound_command(&[
            "create", "boom", "--file", "a.wav", "--bus", "sfx", "--gain", "0.4",
        ]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let path = fx.project_root().join("sources/sounds/boom.json");
    let created_keys = keys_in_order(&read_value(&path));
    inject_unknown(&path, "x_custom", json!({"a": 1}));

    handle_sound_command(
        &sound_command(&["update", "boom", "--gain", "0.5"]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let updated_keys = keys_in_order(&read_value(&path));

    let mut expected = created_keys;
    expected.push("x_custom".to_string());
    assert_eq!(updated_keys, expected);
}

#[tokio::test]
async fn test_attenuation_update_keeps_create_key_order_with_unknown_key_last() {
    let fx = AssetTestFixture::new("order_attenuation").unwrap();
    let _cwd = CwdGuard::enter(fx.project_root());
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    handle_attenuation_command(
        &attenuation_command(&[
            "create",
            "short",
            "--max-distance",
            "50",
            "--shape",
            "sphere",
            "--inner-radius",
            "5",
            "--outer-radius",
            "10",
            "--curve-part",
            "0,1,1,0,Linear",
            "--curve-part",
            "1,0,2,0,Linear",
        ]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let path = fx.project_root().join("sources/attenuators/short.json");
    let created_keys = keys_in_order(&read_value(&path));
    inject_unknown(&path, "x_custom", json!({"a": 1}));

    handle_attenuation_command(
        &attenuation_command(&["update", "short", "--max-distance", "80"]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let updated_keys = keys_in_order(&read_value(&path));

    let mut expected = created_keys;
    expected.push("x_custom".to_string());
    assert_eq!(updated_keys, expected);
}

#[tokio::test]
async fn test_rtpc_update_keeps_create_key_order_with_unknown_key_last() {
    let fx = AssetTestFixture::new("order_rtpc").unwrap();
    let _cwd = CwdGuard::enter(fx.project_root());
    let input = NonInteractiveInput;
    let output = CaptureOutput::interactive();

    handle_rtpc_command(
        &rtpc_command(&["create", "speed", "--min-value", "0", "--max-value", "10"]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let path = fx.project_root().join("sources/rtpc/speed.json");
    let created_keys = keys_in_order(&read_value(&path));
    inject_unknown(&path, "x_custom", json!({"a": 1}));

    handle_rtpc_command(
        &rtpc_command(&["update", "speed", "--default-value", "3"]),
        None,
        &input,
        &output,
    )
    .await
    .unwrap();
    let updated_keys = keys_in_order(&read_value(&path));

    let mut expected = created_keys;
    expected.push("x_custom".to_string());
    assert_eq!(updated_keys, expected);
}
