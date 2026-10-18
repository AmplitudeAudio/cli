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

//! Tests for `am asset bus-file create|list|delete`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, BusFileCommands, handle_bus_file_command};
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

fn parse(args: &[&str]) -> BusFileCommands {
    let mut full = vec!["am", "asset", "bus-file"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::BusFile { command },
        } => command,
        _ => panic!("expected a bus-file command"),
    }
}

async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: &CaptureOutput,
) -> anyhow::Result<()> {
    let _cwd = CwdGuard::enter(fx.project_root());
    handle_bus_file_command(&parse(args), None, input, output).await
}

async fn run(fx: &AssetTestFixture, args: &[&str]) -> (anyhow::Result<()>, CaptureOutput) {
    let output = CaptureOutput::interactive();
    let result = run_with(fx, args, &NonInteractiveInput, &output).await;
    (result, output)
}

fn bus_file_path(fx: &AssetTestFixture, name: &str) -> PathBuf {
    fx.project_root().join(format!("{name}.buses.json"))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn code_of(err: &anyhow::Error) -> i32 {
    as_cli_error(err).unwrap().code
}

#[test]
fn test_subcommands_parse() {
    assert!(matches!(
        parse(&["create", "extra"]),
        BusFileCommands::Create { .. }
    ));
    assert!(matches!(parse(&["list"]), BusFileCommands::List {}));
    let BusFileCommands::Delete { file_name, force } = parse(&["delete", "extra", "--force"])
    else {
        panic!("expected delete");
    };
    assert_eq!(file_name, "extra");
    assert!(force);
}

#[tokio::test]
async fn test_create_writes_master_bus_with_id_1() {
    let fx = AssetTestFixture::new("busfile_create").unwrap();
    let (result, _out) = run(&fx, &["create", "extra"]).await;
    result.unwrap();

    let file = read_json(&bus_file_path(&fx, "extra"));
    let buses = file["buses"].as_array().unwrap();
    assert_eq!(buses.len(), 1);
    assert_eq!(buses[0]["id"], json!(1));
    assert_eq!(buses[0]["name"], json!("master"));
    assert_eq!(buses[0]["gain"], json!(1.0));
    assert_eq!(buses[0]["fader"], json!("Linear"));
}

#[tokio::test]
async fn test_create_accepts_the_full_file_name() {
    let fx = AssetTestFixture::new("busfile_create_full").unwrap();
    run(&fx, &["create", "extra.buses.json"]).await.0.unwrap();
    assert!(bus_file_path(&fx, "extra").exists());
}

#[tokio::test]
async fn test_create_refuses_existing_file_and_bad_names() {
    let fx = AssetTestFixture::new("busfile_create_dup").unwrap();
    let (result, _out) = run(&fx, &["create", "main"]).await;
    assert_eq!(
        code_of(&result.unwrap_err()),
        codes::ERR_ASSET_ALREADY_EXISTS
    );

    for bad in ["../evil", "a/b", "has space"] {
        let (result, _out) = run(&fx, &["create", bad]).await;
        assert_eq!(
            code_of(&result.unwrap_err()),
            codes::ERR_VALIDATION_FIELD,
            "{bad}"
        );
    }
    assert!(!fx.project_root().join("evil.buses.json").exists());
}

#[tokio::test]
async fn test_created_file_is_accepted_by_flatc() {
    let fx = AssetTestFixture::new("busfile_flatc").unwrap();
    run(&fx, &["create", "extra"]).await.0.unwrap();
    common::flatc::assert_flatc_accepts_buses(&bus_file_path(&fx, "extra"));
}

#[tokio::test]
async fn test_list_reports_files_with_bus_counts() {
    let fx = AssetTestFixture::new("busfile_list").unwrap();
    run(&fx, &["create", "extra"]).await.0.unwrap();

    let output = CaptureOutput::json();
    run_with(&fx, &["list"], &NonInteractiveInput, &output)
        .await
        .unwrap();
    let data = output.last_success().unwrap();
    assert_eq!(data["count"], json!(2));
    let files = data["files"].as_array().unwrap();
    let extra = files.iter().find(|f| f["file"] == "extra").unwrap();
    assert_eq!(extra["buses"], json!(1));
    let main = files.iter().find(|f| f["file"] == "main").unwrap();
    assert_eq!(main["buses"], json!(2));
}

#[tokio::test]
async fn test_delete_refuses_the_last_file() {
    let fx = AssetTestFixture::new("busfile_delete_last").unwrap();
    let (result, _out) = run(&fx, &["delete", "main", "--force"]).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_VALIDATION_FIELD);
    assert!(format!("{err:#}").contains("last"), "{err:#}");
    assert!(bus_file_path(&fx, "main").exists());
}

#[tokio::test]
async fn test_delete_removes_an_unreferenced_file() {
    let fx = AssetTestFixture::new("busfile_delete").unwrap();
    run(&fx, &["create", "extra"]).await.0.unwrap();
    let (result, _out) = run(&fx, &["delete", "extra", "--force"]).await;
    result.unwrap();
    assert!(!bus_file_path(&fx, "extra").exists());
    assert!(bus_file_path(&fx, "main").exists());
}

#[tokio::test]
async fn test_delete_unknown_file_is_not_found() {
    let fx = AssetTestFixture::new("busfile_delete_missing").unwrap();
    let (result, _out) = run(&fx, &["delete", "ghost", "--force"]).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_ASSET_NOT_FOUND);
}

#[tokio::test]
async fn test_delete_refuses_when_a_sound_uses_a_bus_only_defined_there() {
    let fx = AssetTestFixture::new("busfile_delete_in_use").unwrap();
    fx.write_buses_file(
        "extra.buses.json",
        json!([
            {"id": 1, "name": "master", "gain": 1.0, "child_buses": [77], "fader": "Linear"},
            {"id": 77, "name": "only_here", "gain": 1.0, "fader": "Linear"}
        ]),
    )
    .unwrap();
    fx.write_asset_json(
        "sounds",
        "boom",
        json!({"id": 500, "name": "boom", "bus": 77}),
    )
    .unwrap();

    let (result, _out) = run(&fx, &["delete", "extra", "--force"]).await;
    let err = result.unwrap_err();
    assert_eq!(code_of(&err), codes::ERR_ASSET_IN_USE);
    assert!(format!("{err:#}").contains("boom"), "{err:#}");
    assert!(bus_file_path(&fx, "extra").exists());
}

#[tokio::test]
async fn test_delete_allows_a_file_whose_buses_are_also_defined_elsewhere() {
    let fx = AssetTestFixture::new("busfile_delete_shared").unwrap();
    // `master` (id 1) exists in both files; a sound on it survives the delete.
    run(&fx, &["create", "extra"]).await.0.unwrap();
    fx.write_asset_json(
        "sounds",
        "boom",
        json!({"id": 500, "name": "boom", "bus": 1}),
    )
    .unwrap();
    run(&fx, &["delete", "extra", "--force"]).await.0.unwrap();
    assert!(!bus_file_path(&fx, "extra").exists());
}

#[tokio::test]
async fn test_delete_non_interactive_without_force_fails_and_keeps_file() {
    let fx = AssetTestFixture::new("busfile_delete_noforce").unwrap();
    run(&fx, &["create", "extra"]).await.0.unwrap();
    let (result, _out) = run(&fx, &["delete", "extra"]).await;
    assert!(result.is_err());
    assert!(bus_file_path(&fx, "extra").exists());
}

#[tokio::test]
async fn test_delete_confirmation_can_be_declined() {
    let fx = AssetTestFixture::new("busfile_delete_decline").unwrap();
    run(&fx, &["create", "extra"]).await.0.unwrap();
    let input = ScriptedInput::new(&[], &[]).with_confirms(&[false]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["delete", "extra"], &input, &output)
        .await
        .unwrap();
    assert!(bus_file_path(&fx, "extra").exists());
}
