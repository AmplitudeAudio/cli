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

//! Shared helpers of the event command tests.

#![allow(dead_code)]

use am::app::{App, Commands};
use am::assets::Event;
use am::commands::asset::{AssetCommands, EventCommands, handle_event_command};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use serde_json::{Value, json};

use super::collection_helpers::CwdGuard;
use super::fixtures::{AssetTestFixture, CaptureOutput};

pub fn parse(args: &[&str]) -> EventCommands {
    let mut full = vec!["am", "asset", "event"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Event { command },
        } => command,
        _ => panic!("expected an event command"),
    }
}

/// A project with sounds `step1` (101) / `step2` (102) / `boom` (103), collections
/// `crowd` (201) / `boom` (203), switch container `music` (301) and buses
/// master (1) / sfx (2) / music_bus (3). `boom` is both a sound and a collection.
pub fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.create_test_sound("step1", 101).unwrap();
    fx.create_test_sound("step2", 102).unwrap();
    fx.create_test_sound("boom", 103).unwrap();
    fx.write_asset_json("collections", "crowd", json!({"id": 201, "name": "crowd", "bus": 2}))
        .unwrap();
    fx.write_asset_json("collections", "boom", json!({"id": 203, "name": "boom", "bus": 2}))
        .unwrap();
    fx.write_asset_json(
        "switch_containers",
        "music",
        json!({"id": 301, "name": "music", "bus": 2}),
    )
    .unwrap();
    fx.write_buses_file(
        "main.buses.json",
        json!([
            {"id": 1, "name": "master", "gain": 1.0, "child_buses": [2, 3], "fader": "Linear"},
            {"id": 2, "name": "sfx", "gain": 1.0, "fader": "Linear"},
            {"id": 3, "name": "music_bus", "gain": 1.0, "fader": "Linear"}
        ]),
    )
    .unwrap();
    fx
}

pub fn event_path(fx: &AssetTestFixture, name: &str) -> std::path::PathBuf {
    fx.project_root()
        .join("sources/events")
        .join(format!("{name}.json"))
}

pub fn read_event(fx: &AssetTestFixture, name: &str) -> Event {
    serde_json::from_str(&std::fs::read_to_string(event_path(fx, name)).unwrap()).unwrap()
}

pub fn read_raw(fx: &AssetTestFixture, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(event_path(fx, name)).unwrap()).unwrap()
}

pub async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: CaptureOutput,
) -> (anyhow::Result<()>, CaptureOutput) {
    let _cwd = CwdGuard::enter(fx.project_root());
    let result = handle_event_command(&parse(args), None, input, &output).await;
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
    let mut full = vec!["create", "ev"];
    full.extend_from_slice(args);
    let (result, _out) = run(fx, &full, &NonInteractiveInput).await;
    result.unwrap();
}

pub fn assert_flatc(fx: &AssetTestFixture, name: &str) {
    super::flatc::assert_flatc_accepts_schema("event_definition.fbs", &event_path(fx, name));
}

/// An event with one action of every type, for update tests.
pub async fn rich_event(fx: &AssetTestFixture) {
    create_ok(
        fx,
        &[
            "--run-mode",
            "sequential",
            "--action",
            "play:step1,crowd,scope=world",
            "--action",
            "stop:step2,active=false",
            "--action",
            "mute-bus:sfx",
            "--action",
            "wait:250",
        ],
    )
    .await;
}
