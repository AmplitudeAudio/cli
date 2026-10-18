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

//! Shared helpers of the switch container command tests.

use am::app::{App, Commands};
use am::assets::SwitchContainer;
use am::commands::asset::{
    AssetCommands, SwitchContainerCommands, handle_switch_container_command,
};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use serde_json::{Value, json};

use super::collection_helpers::CwdGuard;
use super::fixtures::{AssetTestFixture, CaptureOutput};

pub fn parse(args: &[&str]) -> SwitchContainerCommands {
    let mut full = vec!["am", "asset", "switch-container"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::SwitchContainer { command },
        } => command,
        _ => panic!("expected a switch container command"),
    }
}

/// A project with sounds `step1` (101) / `step2` (102) / `boom` (103), collections
/// `crowd` (201) / `boom` (203), switches `surface` (10: wood 11, stone 12, gravel 13)
/// and `weather` (20: rain 21), an attenuation, an effect, an rtpc and buses
/// master (1) / sfx (2).
pub fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.create_test_sound("step1", 101).unwrap();
    fx.create_test_sound("step2", 102).unwrap();
    fx.create_test_sound("boom", 103).unwrap();
    fx.write_asset_json(
        "collections",
        "crowd",
        json!({"id": 201, "name": "crowd", "bus": 2}),
    )
    .unwrap();
    fx.write_asset_json(
        "collections",
        "boom",
        json!({"id": 203, "name": "boom", "bus": 2}),
    )
    .unwrap();
    fx.write_asset_json(
        "switches",
        "surface",
        json!({"id": 10, "name": "surface", "states": [
            {"id": 11, "name": "wood"}, {"id": 12, "name": "stone"}, {"id": 13, "name": "gravel"}]}),
    )
    .unwrap();
    fx.write_asset_json(
        "switches",
        "weather",
        json!({"id": 20, "name": "weather", "states": [{"id": 21, "name": "rain"}]}),
    )
    .unwrap();
    fx.write_asset_json("attenuators", "short", json!({"id": 55, "name": "short"}))
        .unwrap();
    fx.write_asset_json("effects", "echo", json!({"id": 66, "name": "echo"}))
        .unwrap();
    fx.write_asset_json("rtpc", "speed", json!({"id": 77, "name": "speed"}))
        .unwrap();
    fx
}

pub fn container_path(fx: &AssetTestFixture, name: &str) -> std::path::PathBuf {
    fx.project_root()
        .join("sources/switch_containers")
        .join(format!("{name}.json"))
}

pub fn read_container(fx: &AssetTestFixture, name: &str) -> SwitchContainer {
    serde_json::from_str(&std::fs::read_to_string(container_path(fx, name)).unwrap()).unwrap()
}

pub fn read_raw(fx: &AssetTestFixture, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(container_path(fx, name)).unwrap()).unwrap()
}

pub async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: CaptureOutput,
) -> (anyhow::Result<()>, CaptureOutput) {
    let _cwd = CwdGuard::enter(fx.project_root());
    let result = handle_switch_container_command(&parse(args), None, input, &output).await;
    (result, output)
}

pub async fn run(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
) -> (anyhow::Result<()>, CaptureOutput) {
    run_with(fx, args, input, CaptureOutput::interactive()).await
}

pub async fn create_ok(fx: &AssetTestFixture, args: &[&str]) {
    let mut full = vec!["create", "steps"];
    full.extend_from_slice(args);
    let (result, _out) = run(fx, &full, &NonInteractiveInput).await;
    result.unwrap();
}

pub fn assert_flatc(fx: &AssetTestFixture, name: &str) {
    super::flatc::assert_flatc_accepts_schema(
        "switch_container_definition.fbs",
        &container_path(fx, name),
    );
}

/// Creates `steps` with a bus, the surface switch and two entries.
pub async fn rich_container(fx: &AssetTestFixture) {
    create_ok(
        fx,
        &[
            "--bus",
            "sfx",
            "--switch-group",
            "surface",
            "--gain",
            "0.4",
            "--entry",
            "step1,states=wood|stone,fade-in=0.2:EaseIn,continue=true",
            "--entry",
            "crowd,states=gravel,gain-rtpc=speed",
        ],
    )
    .await;
}
