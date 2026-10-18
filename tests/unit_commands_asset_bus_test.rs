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

//! Tests for `am asset bus create|list`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};

use am::app::{App, Commands};
use am::assets::ProjectValidator;
use am::commands::asset::{AssetCommands, BusCommands, handle_bus_command, handle_sound_command};
use am::common::errors::{as_cli_error, codes};
use am::forms::references::RefTarget;
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
// Clap surface
// ---------------------------------------------------------------------------

#[test]
fn test_create_parses_every_flag_with_repeatable_duck() {
    let BusCommands::Create { name, args } = parse(&[
        "create",
        "sfx",
        "--file",
        "main",
        "--gain",
        "0.5",
        "--parent",
        "master",
        "--fader",
        "Ease",
        "--duck",
        "music:0.2:1:2",
        "--duck",
        "voices:0.5:0.5:0.5:Ease:Linear",
    ]) else {
        panic!("expected create");
    };
    assert_eq!(name, "sfx");
    assert_eq!(args.file.as_deref(), Some("main"));
    assert_eq!(args.gain, Some(0.5));
    assert_eq!(args.parent.as_deref(), Some("master"));
    assert_eq!(args.fader.as_deref(), Some("Ease"));
    assert_eq!(args.duck.len(), 2);
}

#[test]
fn test_other_subcommands_parse() {
    assert!(matches!(parse(&["list"]), BusCommands::List { .. }));
    assert!(matches!(
        parse(&["update", "sfx", "--gain", "0.1"]),
        BusCommands::Update { .. }
    ));
    let BusCommands::Delete { name, file, force } =
        parse(&["delete", "sfx", "--file", "main", "--force"])
    else {
        panic!("expected delete");
    };
    assert_eq!(
        (name.as_str(), file.as_deref(), force),
        ("sfx", Some("main"), true)
    );
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_adds_the_bus_and_updates_masters_children() {
    let fx = fixture("bus_create");
    let (result, _out) = run(&fx, &["create", "sfx", "--parent", "master"]).await;
    result.unwrap();

    let file = read_file(&fx, "main");
    let sfx = bus(&file, "sfx");
    let id = sfx["id"].as_u64().unwrap();
    assert!(id > 1);
    assert_eq!(sfx["gain"], json!(1.0));
    assert_eq!(sfx["fader"], json!("Linear"));
    assert_eq!(children(&file, "master"), vec![id]);
}

#[tokio::test]
async fn test_parent_defaults_to_master() {
    let fx = fixture("bus_default_parent");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    let file = read_file(&fx, "main");
    assert_eq!(children(&file, "master").len(), 1);
}

#[tokio::test]
async fn test_create_nested_bus_and_flags() {
    let fx = fixture("bus_nested");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    run(
        &fx,
        &[
            "create", "weapons", "--parent", "sfx", "--gain", "0.5", "--fader", "ease",
        ],
    )
    .await
    .0
    .unwrap();

    let file = read_file(&fx, "main");
    let weapons = bus(&file, "weapons");
    assert_eq!(weapons["gain"], json!(0.5));
    assert_eq!(weapons["fader"], json!("Ease"));
    assert_eq!(
        children(&file, "sfx"),
        vec![weapons["id"].as_u64().unwrap()]
    );
}

#[tokio::test]
async fn test_create_with_duck_buses() {
    let fx = fixture("bus_duck");
    run(&fx, &["create", "music"]).await.0.unwrap();
    run(
        &fx,
        &[
            "create",
            "voices",
            "--duck",
            "music:0.25:1.5:2:SCurveSmooth:Ease",
        ],
    )
    .await
    .0
    .unwrap();

    let file = read_file(&fx, "main");
    let music_id = bus(&file, "music")["id"].clone();
    let duck = &bus(&file, "voices")["duck_buses"][0];
    assert_eq!(duck["id"], music_id);
    assert_eq!(duck["target_gain"], json!(0.25));
    assert_eq!(
        duck["fade_in"],
        json!({"duration": 1.5, "fader": "SCurveSmooth"})
    );
    assert_eq!(duck["fade_out"], json!({"duration": 2.0, "fader": "Ease"}));
    common::flatc::assert_flatc_accepts_buses(&file_path(&fx, "main"));
}

#[tokio::test]
async fn test_created_file_is_accepted_by_flatc() {
    let fx = fixture("bus_flatc");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    run(
        &fx,
        &["create", "music", "--parent", "sfx", "--gain", "0.3"],
    )
    .await
    .0
    .unwrap();
    common::flatc::assert_flatc_accepts_buses(&file_path(&fx, "main"));
}

#[tokio::test]
async fn test_duplicate_name_is_rejected_within_and_across_files() {
    let fx = fixture("bus_dup");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    let before = bytes(&fx, "main");

    let (result, _out) = run(&fx, &["create", "sfx"]).await;
    assert_eq!(
        code_of(&result.unwrap_err()),
        codes::ERR_ASSET_ALREADY_EXISTS
    );
    assert_eq!(bytes(&fx, "main"), before);

    fx.write_buses_file(
        "second.buses.json",
        json!([{"id": 1, "name": "master", "gain": 1.0, "fader": "Linear"}]),
    )
    .unwrap();
    let (result, _out) = run(&fx, &["create", "sfx", "--file", "second"]).await;
    assert_eq!(
        code_of(&result.unwrap_err()),
        codes::ERR_ASSET_ALREADY_EXISTS
    );
    assert_eq!(
        read_file(&fx, "second")["buses"].as_array().unwrap().len(),
        1
    );

    let (result, _out) = run(&fx, &["create", "master", "--file", "second"]).await;
    assert_eq!(
        code_of(&result.unwrap_err()),
        codes::ERR_ASSET_ALREADY_EXISTS
    );
}

#[tokio::test]
async fn test_generated_id_avoids_ids_of_other_buses_and_assets() {
    let fx = fixture("bus_ids");
    run(&fx, &["create", "a"]).await.0.unwrap();
    run(&fx, &["create", "b"]).await.0.unwrap();
    run(&fx, &["create", "c"]).await.0.unwrap();
    let file = read_file(&fx, "main");
    let mut ids: Vec<u64> = file["buses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["id"].as_u64().unwrap())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 4);
}

#[tokio::test]
async fn test_unknown_parent_fader_and_bad_duck_leave_the_file_untouched() {
    let fx = fixture("bus_bad_args");
    let before = bytes(&fx, "main");
    for args in [
        vec!["create", "sfx", "--parent", "ghost"],
        vec!["create", "sfx", "--fader", "Bouncy"],
        vec!["create", "sfx", "--gain", "-1"],
        vec!["create", "sfx", "--duck", "ghost:0.5:1:1"],
        vec!["create", "sfx", "--duck", "master:0.5"],
        vec!["create", "sfx", "--duck", "master:x:1:1"],
        vec!["create", "sfx", "--duck", "master:0.5:-1:1"],
        vec!["create", "sfx", "--duck", "master:0.5:1:1:Nope"],
        vec!["create", "bad:name"],
    ] {
        let (result, _out) = run(&fx, &args).await;
        let err = result.unwrap_err();
        assert!(as_cli_error(&err).is_some(), "{args:?}: {err:#}");
        assert_eq!(bytes(&fx, "main"), before, "{args:?}");
    }
}

#[tokio::test]
async fn test_create_with_no_bus_file_suggests_creating_one() {
    let fx = AssetTestFixture::new("bus_no_file").unwrap();
    std::fs::remove_file(file_path(&fx, "main")).unwrap();
    let (result, _out) = run(&fx, &["create", "sfx"]).await;
    let err = result.unwrap_err();
    let cli = as_cli_error(&err).unwrap();
    assert!(
        cli.suggestion.contains("bus-file create"),
        "{}",
        cli.suggestion
    );
}

#[tokio::test]
async fn test_missing_file_flag_with_two_files_names_the_flag() {
    let fx = fixture("bus_two_files");
    fx.write_buses_file(
        "second.buses.json",
        json!([{"id": 1, "name": "master", "gain": 1.0, "fader": "Linear"}]),
    )
    .unwrap();
    let before = (bytes(&fx, "main"), bytes(&fx, "second"));

    let (result, _out) = run(&fx, &["create", "sfx"]).await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("--file"), "{err:#}");

    let output = CaptureOutput::json();
    let err = run_with(&fx, &["create", "sfx"], &NonInteractiveInput, &output)
        .await
        .unwrap_err();
    assert!(format!("{err:#}").contains("--file"), "{err:#}");
    assert_eq!(before, (bytes(&fx, "main"), bytes(&fx, "second")));
}

#[tokio::test]
async fn test_interactive_run_picks_the_file_when_there_are_two() {
    let fx = fixture("bus_pick_file");
    fx.write_buses_file(
        "second.buses.json",
        json!([{"id": 1, "name": "master", "gain": 1.0, "fader": "Linear"}]),
    )
    .unwrap();
    // file pick, then "accept defaults" (confirm default is true).
    let input = ScriptedInput::new(&["second"], &[]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["create", "sfx"], &input, &output)
        .await
        .unwrap();
    assert_eq!(
        read_file(&fx, "second")["buses"].as_array().unwrap().len(),
        2
    );
    assert_eq!(read_file(&fx, "main")["buses"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_interactive_create_walks_the_fields() {
    let fx = fixture("bus_interactive");
    run(&fx, &["create", "music"]).await.0.unwrap();
    // accept-defaults = no; parent pick, fader pick, duck bus pick; texts: gain, duck gain, in, out.
    let input = ScriptedInput::new(
        &["master", "Ease", "music", "Linear", "Linear"],
        &["0.7", "0.4", "1", "2"],
    )
    .with_confirms(&[false, true, false]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["create", "voices"], &input, &output)
        .await
        .unwrap();

    let file = read_file(&fx, "main");
    let voices = bus(&file, "voices");
    assert_eq!(voices["gain"], json!(0.7));
    assert_eq!(voices["fader"], json!("Ease"));
    assert_eq!(voices["duck_buses"][0]["target_gain"], json!(0.4));
    assert_eq!(voices["duck_buses"][0]["fade_out"]["duration"], json!(2.0));
}

#[tokio::test]
async fn test_list_only_buses_of_the_selected_file_and_json() {
    let fx = fixture("bus_list");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    let output = CaptureOutput::json();
    run_with(&fx, &["list"], &NonInteractiveInput, &output)
        .await
        .unwrap();
    let data = output.last_success().unwrap();
    assert_eq!(data["count"], json!(2));
    let rows = data["buses"].as_array().unwrap();
    let sfx = rows.iter().find(|r| r["name"] == "sfx").unwrap();
    assert_eq!(sfx["parent"], json!("master"));
    assert_eq!(sfx["file"], json!("main"));
}

#[tokio::test]
async fn test_validator_lists_the_new_bus() {
    let fx = fixture("bus_validator");
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    let validator: ProjectValidator = fx.create_project_validator().unwrap();
    let names: Vec<String> = validator
        .list(RefTarget::Bus)
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert!(names.contains(&"sfx".to_string()), "{names:?}");
}

#[tokio::test]
async fn test_sound_create_on_the_new_bus_works_end_to_end() {
    let fx = fixture("bus_sound_e2e");
    fx.create_data_file("a.wav").unwrap();
    run(&fx, &["create", "sfx"]).await.0.unwrap();
    let id = bus(&read_file(&fx, "main"), "sfx")["id"].as_u64().unwrap();

    let _cwd = CwdGuard::enter(fx.project_root());
    let mut full = vec!["am", "asset", "sound"];
    full.extend(["create", "boom", "--file", "a.wav", "--bus", "sfx"]);
    let Commands::Asset {
        command: AssetCommands::Sound { command },
    } = App::try_parse_from(full).unwrap().command
    else {
        panic!("expected sound");
    };
    let output = CaptureOutput::interactive();
    handle_sound_command(&command, None, &NonInteractiveInput, &output)
        .await
        .unwrap();
    let sound: Value = serde_json::from_str(
        &std::fs::read_to_string(fx.project_root().join("sources/sounds/boom.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(sound["bus"].as_u64(), Some(id));
}
