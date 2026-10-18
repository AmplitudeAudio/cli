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

//! Tests for `am asset bus update|delete`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, BusCommands, handle_bus_command, handle_sound_command};
use am::common::errors::{as_cli_error, codes};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use common::fixtures::{AssetTestFixture, CaptureOutput};
use common::scripted_input::ScriptedInput;
use serde_json::{Value, json};

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

fn parse(args: &[&str]) -> BusCommands {
    let mut full = vec!["am", "asset", "bus"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Bus { command },
        } => command,
        _ => panic!("expected a bus command"),
    }
}

async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: &CaptureOutput,
) -> anyhow::Result<()> {
    let _cwd = CwdGuard::enter(fx.project_root());
    handle_bus_command(&parse(args), None, input, output).await
}

async fn run(fx: &AssetTestFixture, args: &[&str]) -> (anyhow::Result<()>, CaptureOutput) {
    let output = CaptureOutput::interactive();
    let result = run_with(fx, args, &NonInteractiveInput, &output).await;
    (result, output)
}

/// A project whose only bus file is `main.buses.json` holding just `master`.
fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    write_main(
        &fx,
        json!([{"id": 1, "name": "master", "gain": 1.0, "fader": "Linear"}]),
    );
    fx
}

fn write_main(fx: &AssetTestFixture, buses: Value) {
    fx.write_buses_file("main.buses.json", buses).unwrap();
}

fn file_path(fx: &AssetTestFixture, name: &str) -> PathBuf {
    fx.project_root().join(format!("{name}.buses.json"))
}

fn read_file(fx: &AssetTestFixture, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(file_path(fx, name)).unwrap()).unwrap()
}

fn bus<'a>(file: &'a Value, name: &str) -> &'a Value {
    file["buses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["name"] == name)
        .unwrap_or_else(|| panic!("no bus '{name}' in {file}"))
}

fn children(file: &Value, name: &str) -> Vec<u64> {
    bus(file, name)["child_buses"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default()
}

fn code_of(err: &anyhow::Error) -> i32 {
    as_cli_error(err).unwrap().code
}

fn bytes(fx: &AssetTestFixture, name: &str) -> String {
    std::fs::read_to_string(file_path(fx, name)).unwrap()
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_changes_only_the_given_flags() {
    let fx = fixture("bus_update");
    run(&fx, &["create", "sfx", "--gain", "0.5", "--fader", "Ease"])
        .await
        .0
        .unwrap();
    run(&fx, &["update", "sfx", "--gain", "0.8"])
        .await
        .0
        .unwrap();
    let file = read_file(&fx, "main");
    assert_eq!(bus(&file, "sfx")["gain"], json!(0.8));
    assert_eq!(bus(&file, "sfx")["fader"], json!("Ease"));
}

#[tokio::test]
async fn test_update_without_flags_in_non_interactive_mode_fails() {
    let fx = fixture("bus_update_nothing");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    let (result, _out) = run(&fx, &["update", "sfx"]).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_VALIDATION_FIELD);
}

#[tokio::test]
async fn test_update_unknown_bus_is_not_found() {
    let fx = fixture("bus_update_missing");
    let (result, _out) = run(&fx, &["update", "ghost", "--gain", "1"]).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_ASSET_NOT_FOUND);
}

#[tokio::test]
async fn test_reparent_updates_both_parents() {
    let fx = fixture("bus_reparent");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    run(&fx, &["create", "music"]).await.0.unwrap();
    run(&fx, &["create", "weapons", "--parent", "sfx"])
        .await
        .0
        .unwrap();
    let weapons_id = bus(&read_file(&fx, "main"), "weapons")["id"]
        .as_u64()
        .unwrap();

    run(&fx, &["update", "weapons", "--parent", "music"])
        .await
        .0
        .unwrap();
    let file = read_file(&fx, "main");
    assert!(children(&file, "sfx").is_empty());
    assert_eq!(children(&file, "music"), vec![weapons_id]);
    common::flatc::assert_flatc_accepts_buses(&file_path(&fx, "main"));
}

#[tokio::test]
async fn test_reparent_rules() {
    let fx = fixture("bus_reparent_bad");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    run(&fx, &["create", "weapons", "--parent", "sfx"])
        .await
        .0
        .unwrap();
    let before = bytes(&fx, "main");

    // master has no parent; a bus cannot move below its own descendant or itself.
    for args in [
        vec!["update", "master", "--parent", "sfx"],
        vec!["update", "sfx", "--parent", "weapons"],
        vec!["update", "sfx", "--parent", "sfx"],
        vec!["update", "sfx", "--parent", "ghost"],
    ] {
        let (result, _out) = run(&fx, &args).await;
        assert!(result.is_err(), "{args:?}");
        assert_eq!(bytes(&fx, "main"), before, "{args:?}");
    }
}

#[tokio::test]
async fn test_update_ducks_replace_and_clear() {
    let fx = fixture("bus_update_ducks");
    run(&fx, &["create", "music"]).await.0.unwrap();
    run(&fx, &["create", "voices", "--duck", "music:0.5:1:1"])
        .await
        .0
        .unwrap();
    run(&fx, &["update", "voices", "--duck", "master:0.1:2:2"])
        .await
        .0
        .unwrap();
    let file = read_file(&fx, "main");
    let ducks = bus(&file, "voices")["duck_buses"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(ducks.len(), 1);
    assert_eq!(ducks[0]["id"], json!(1));

    run(&fx, &["update", "voices", "--clear-ducks"])
        .await
        .0
        .unwrap();
    let file = read_file(&fx, "main");
    assert!(
        bus(&file, "voices")
            .get("duck_buses")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    );
}

#[tokio::test]
async fn test_update_keeps_unknown_keys_of_the_file_and_of_every_entry() {
    let fx = AssetTestFixture::new("bus_unknown_keys").unwrap();
    let raw = json!({
        "$comment": "hand written",
        "buses": [
            {"id": 1, "name": "master", "gain": 1.0, "child_buses": [2, 3],
             "fader": "Linear", "editor_color": "#fff"},
            {"id": 2, "name": "sfx", "gain": 1.0, "fader": "Linear", "color": "red",
             "duck_buses": [{"id": 3, "target_gain": 0.5, "note": "keep",
                "fade_in": {"duration": 1.0, "fader": "Linear"},
                "fade_out": {"duration": 1.0, "fader": "Linear"}}]},
            {"id": 3, "name": "music", "gain": 0.4, "fader": "Ease", "tag": [1, 2]}
        ],
        "editor": {"zoom": 2}
    });
    std::fs::write(
        file_path(&fx, "main"),
        serde_json::to_string_pretty(&raw).unwrap(),
    )
    .unwrap();

    run(&fx, &["update", "sfx", "--gain", "0.9"])
        .await
        .0
        .unwrap();
    let file = read_file(&fx, "main");
    assert_eq!(file["$comment"], json!("hand written"));
    assert_eq!(file["editor"], json!({"zoom": 2}));
    assert_eq!(bus(&file, "sfx")["color"], json!("red"));
    assert_eq!(bus(&file, "sfx")["gain"], json!(0.9));
    assert_eq!(
        bus(&file, "sfx")["duck_buses"],
        raw["buses"][1]["duck_buses"]
    );
    // Other entries are untouched, value for value.
    assert_eq!(bus(&file, "master"), &raw["buses"][0]);
    assert_eq!(bus(&file, "music"), &raw["buses"][2]);

    // Re-parenting rewrites only the two parents' child lists.
    run(&fx, &["update", "music", "--parent", "sfx"])
        .await
        .0
        .unwrap();
    let file = read_file(&fx, "main");
    assert_eq!(bus(&file, "master")["editor_color"], json!("#fff"));
    assert_eq!(bus(&file, "music")["tag"], json!([1, 2]));
    assert_eq!(children(&file, "master"), vec![2]);
    assert_eq!(children(&file, "sfx"), vec![3]);
    let keys: Vec<&String> = file.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["$comment", "buses", "editor"]);
}

#[tokio::test]
async fn test_a_file_the_sdk_could_not_load_is_not_rewritten() {
    let fx = AssetTestFixture::new("bus_broken_file").unwrap();
    // `sfx` has no fader: the SDK dereferences it while loading.
    let broken = json!({"buses": [
        {"id": 1, "name": "master", "child_buses": [2], "fader": "Linear"},
        {"id": 2, "name": "sfx"}]});
    std::fs::write(
        file_path(&fx, "main"),
        serde_json::to_string_pretty(&broken).unwrap(),
    )
    .unwrap();
    let before = bytes(&fx, "main");
    let (result, _out) = run(&fx, &["update", "master", "--gain", "0.5"]).await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("fader"), "{err:#}");
    assert_eq!(bytes(&fx, "main"), before);
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_removes_the_bus_and_its_entry_in_the_parent() {
    let fx = fixture("bus_delete");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    run(&fx, &["delete", "sfx", "--force"]).await.0.unwrap();
    let file = read_file(&fx, "main");
    assert_eq!(file["buses"].as_array().unwrap().len(), 1);
    assert!(children(&file, "master").is_empty());
    common::flatc::assert_flatc_accepts_buses(&file_path(&fx, "main"));
}

#[tokio::test]
async fn test_delete_master_is_refused() {
    let fx = fixture("bus_delete_master");
    let (result, _out) = run(&fx, &["delete", "master", "--force"]).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_VALIDATION_FIELD);
    assert!(format!("{err:#}").contains("master"));
}

#[tokio::test]
async fn test_delete_bus_with_children_is_refused() {
    let fx = fixture("bus_delete_children");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    run(&fx, &["create", "weapons", "--parent", "sfx"])
        .await
        .0
        .unwrap();
    let before = bytes(&fx, "main");
    let (result, _out) = run(&fx, &["delete", "sfx", "--force"]).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_ASSET_IN_USE);
    assert!(format!("{err:#}").contains("weapons"), "{err:#}");
    assert_eq!(bytes(&fx, "main"), before);
}

#[tokio::test]
async fn test_delete_referenced_bus_is_refused_even_with_force() {
    let fx = fixture("bus_delete_referenced");
    fx.create_data_file("a.wav").unwrap();
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    {
        let _cwd = CwdGuard::enter(fx.project_root());
        let Commands::Asset {
            command: AssetCommands::Sound { command },
        } = App::try_parse_from([
            "am", "asset", "sound", "create", "boom", "--file", "a.wav", "--bus", "sfx",
        ])
        .unwrap()
        .command
        else {
            panic!("expected sound");
        };
        handle_sound_command(
            &command,
            None,
            &NonInteractiveInput,
            &CaptureOutput::interactive(),
        )
        .await
        .unwrap();
    }
    let before = bytes(&fx, "main");
    let (result, _out) = run(&fx, &["delete", "sfx", "--force"]).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_ASSET_IN_USE);
    assert!(format!("{err:#}").contains("boom"), "{err:#}");
    assert_eq!(bytes(&fx, "main"), before);
}

#[tokio::test]
async fn test_delete_bus_used_as_duck_target_is_refused() {
    let fx = fixture("bus_delete_duck");
    run(&fx, &["create", "music"]).await.0.unwrap();
    run(&fx, &["create", "voices", "--duck", "music:0.5:1:1"])
        .await
        .0
        .unwrap();
    let (result, _out) = run(&fx, &["delete", "music", "--force"]).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_ASSET_IN_USE);
    assert!(format!("{err:#}").contains("voices"), "{err:#}");
}

#[tokio::test]
async fn test_delete_needs_confirmation_or_force() {
    let fx = fixture("bus_delete_confirm");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    let (result, _out) = run(&fx, &["delete", "sfx"]).await;
    assert!(result.is_err());
    assert_eq!(read_file(&fx, "main")["buses"].as_array().unwrap().len(), 2);

    let input = ScriptedInput::new(&[], &[]).with_confirms(&[false]);
    run_with(
        &fx,
        &["delete", "sfx"],
        &input,
        &CaptureOutput::interactive(),
    )
    .await
    .unwrap();
    assert_eq!(read_file(&fx, "main")["buses"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_delete_with_two_files_requires_the_file_flag() {
    let fx = fixture("bus_delete_two");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    fx.write_buses_file(
        "second.buses.json",
        json!([{"id": 1, "name": "master", "gain": 1.0, "fader": "Linear"}]),
    )
    .unwrap();
    let (result, _out) = run(&fx, &["delete", "sfx", "--force"]).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--file"));
    run(&fx, &["delete", "sfx", "--force", "--file", "main"])
        .await
        .0
        .unwrap();
}
