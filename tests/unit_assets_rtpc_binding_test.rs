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

//! RTPC-bound values on sounds, collections and switch containers.
//!
//! The SDK reads `rtpc()->id()` of an RTPC-kind value without a check
//! (`Rtpc.cpp`), so a missing `rtpc` object or an unknown/zero id must fail
//! validation, like it does for effect parameters.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, handle_sound_command};
use am::commands::project;
use am::input::NonInteractiveInput;
use clap::Parser;
use common::collection_helpers::{self as coll, CwdGuard};
use common::fixtures::{AssetTestFixture, CaptureOutput};
use common::switch_container_helpers as sc;
use serde_json::{Value, json};

const SOUND: &str = "sources/sounds/step1.json";
const COLLECTION: &str = "sources/collections/steps.json";
const CONTAINER: &str = "sources/switch_containers/steps.json";

fn dangling() -> Value {
    json!({"kind": "RTPC", "value": 0.0, "rtpc": {"id": 424242}})
}

fn zero_id() -> Value {
    json!({"kind": "RTPC", "value": 0.0, "rtpc": {"id": 0}})
}

fn no_rtpc_object() -> Value {
    json!({"kind": "RTPC", "value": 0.0})
}

fn bound_to_speed() -> Value {
    json!({"kind": "RTPC", "value": 0.0, "rtpc": {"id": 77}})
}

fn patch(path: &std::path::Path, edit: impl FnOnce(&mut Value)) {
    let mut json: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    edit(&mut json);
    std::fs::write(path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
}

fn sound_path(fx: &AssetTestFixture, name: &str) -> std::path::PathBuf {
    fx.project_root()
        .join(format!("sources/sounds/{name}.json"))
}

/// Runs `am project validate --json` and returns the errors reported for `file`
/// (the fixture's other assets are deliberately minimal and report their own).
async fn validate(fx: &AssetTestFixture, file: &str) -> String {
    let command = match App::try_parse_from(["am", "project", "validate"])
        .unwrap()
        .command
    {
        Commands::Project { command } => command,
        _ => unreachable!(),
    };
    let output = CaptureOutput::json();
    let _cwd = CwdGuard::enter(fx.project_root());
    let _ = project::handler(&command, None, &NonInteractiveInput, &output).await;
    let errors = output.last_success().expect("validate prints its result")["errors"].clone();
    let own: Vec<&Value> = errors
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["file"] == file)
        .collect();
    serde_json::to_string(&own).unwrap()
}

async fn assert_fails(fx: &AssetTestFixture, file: &str, needle: &str) {
    let errors = validate(fx, file).await;
    assert!(errors.contains(needle), "expected '{needle}' in {errors}");
}

async fn assert_clean(fx: &AssetTestFixture, file: &str) {
    let errors = validate(fx, file).await;
    assert_eq!(errors, "[]", "{file} should be valid");
}

/// A sound whose audio file exists, so only its RTPC bindings can fail.
fn sound_fixture(name: &str) -> AssetTestFixture {
    let fx = coll::fixture(name);
    fx.create_data_file("step1.wav").unwrap();
    patch(&sound_path(&fx, "step1"), |s| {
        s["path"] = json!("step1.wav")
    });
    fx
}

#[tokio::test]
async fn test_sound_rtpc_gain_with_unknown_id_fails_validate() {
    let fx = sound_fixture("rtpc_sound_dangling");
    patch(&sound_path(&fx, "step1"), |s| s["gain"] = dangling());
    assert_fails(&fx, SOUND, "424242").await;
}

#[tokio::test]
async fn test_sound_rtpc_pitch_without_rtpc_object_fails_validate() {
    let fx = sound_fixture("rtpc_sound_missing");
    patch(&sound_path(&fx, "step1"), |s| s["pitch"] = no_rtpc_object());
    assert_fails(&fx, SOUND, "pitch").await;
}

#[tokio::test]
async fn test_sound_rtpc_with_id_zero_fails_validate() {
    let fx = sound_fixture("rtpc_sound_zero");
    patch(&sound_path(&fx, "step1"), |s| s["spread"] = zero_id());
    assert_fails(&fx, SOUND, "spread").await;
}

#[tokio::test]
async fn test_sound_rtpc_bound_to_an_existing_rtpc_is_valid() {
    let fx = sound_fixture("rtpc_sound_ok");
    patch(&sound_path(&fx, "step1"), |s| {
        s["gain"] = bound_to_speed();
        s["pitch"] = bound_to_speed();
    });
    assert_clean(&fx, SOUND).await;
}

#[tokio::test]
async fn test_sound_update_does_not_keep_a_broken_rtpc_binding() {
    let fx = sound_fixture("rtpc_sound_update");
    patch(&sound_path(&fx, "step1"), |s| s["pitch"] = no_rtpc_object());
    let command =
        match App::try_parse_from(["am", "asset", "sound", "update", "step1", "--gain", "0.5"])
            .unwrap()
            .command
        {
            Commands::Asset {
                command: AssetCommands::Sound { command },
            } => command,
            _ => unreachable!(),
        };
    let output = CaptureOutput::interactive();
    let _cwd = CwdGuard::enter(fx.project_root());
    let result = handle_sound_command(&command, None, &NonInteractiveInput, &output).await;
    let error = format!(
        "{:#}",
        result.expect_err("update must fail on the broken pitch")
    );
    assert!(error.contains("pitch"), "error: {error}");
}

#[tokio::test]
async fn test_collection_rtpc_values_are_checked() {
    let fx = coll::fixture("rtpc_collection");
    coll::create_ok(&fx, &["--bus", "sfx", "--entry", "step1"]).await;
    let path = coll::collection_path(&fx, "steps");
    patch(&path, |c| c["pitch"] = dangling());
    assert_fails(&fx, COLLECTION, "424242").await;
    patch(&path, |c| c["pitch"] = bound_to_speed());
    patch(&path, |c| c["sounds"][0]["gain"] = no_rtpc_object());
    assert_fails(&fx, COLLECTION, "sounds[0].gain").await;
    patch(&path, |c| c["sounds"][0]["gain"] = bound_to_speed());
    assert_clean(&fx, COLLECTION).await;
}

#[tokio::test]
async fn test_switch_container_rtpc_values_are_checked() {
    let fx = sc::fixture("rtpc_container");
    sc::rich_container(&fx).await;
    let path = sc::container_path(&fx, "steps");
    patch(&path, |c| c["gain"] = dangling());
    assert_fails(&fx, CONTAINER, "424242").await;
    patch(&path, |c| c["gain"] = bound_to_speed());
    patch(&path, |c| c["entries"][0]["pitch"] = no_rtpc_object());
    assert_fails(&fx, CONTAINER, "entries[0].pitch").await;
    patch(&path, |c| c["entries"][0]["pitch"] = bound_to_speed());
    assert_clean(&fx, CONTAINER).await;
}
