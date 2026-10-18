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

//! Shared helpers of the soundbank command tests.

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, SoundbankCommands, handle_soundbank_command};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use serde_json::{Value, json};

use super::collection_helpers::CwdGuard;
use super::fixtures::{AssetTestFixture, CaptureOutput};

pub fn parse(args: &[&str]) -> SoundbankCommands {
    let mut full = vec!["am", "asset", "soundbank"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Soundbank { command },
        } => command,
        _ => panic!("expected a soundbank command"),
    }
}

/// A project with sounds boom and fx/step.wav, collection crowd, event hit, switch surface,
/// switch container footsteps, effect echo, attenuator room and rtpc wind.
pub fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.create_test_sound("boom", 11).unwrap();
    std::fs::create_dir_all(fx.sources_dir().join("sounds/fx")).unwrap();
    std::fs::write(
        fx.sources_dir().join("sounds/fx/step.wav.json"),
        json!({"id": 12, "name": "step"}).to_string(),
    )
    .unwrap();
    fx.create_test_collection("crowd", 21, &[]).unwrap();
    for (dir, name, id) in [
        ("events", "hit", 41),
        ("switches", "surface", 51),
        ("switch_containers", "footsteps", 61),
        ("effects", "echo", 71),
        ("attenuators", "room", 81),
        ("rtpc", "wind", 91),
    ] {
        fx.write_asset_json(dir, name, json!({"id": id, "name": name}))
            .unwrap();
    }
    fx
}

pub fn bank_path(fx: &AssetTestFixture, name: &str) -> std::path::PathBuf {
    fx.project_root()
        .join("sources/soundbanks")
        .join(format!("{name}.json"))
}

pub fn read_raw(fx: &AssetTestFixture, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(bank_path(fx, name)).unwrap()).unwrap()
}

/// The entries of list `field` of bank `name`, as written.
pub fn entries(fx: &AssetTestFixture, name: &str, field: &str) -> Vec<String> {
    read_raw(fx, name)[field]
        .as_array()
        .unwrap_or_else(|| panic!("{field} is not a list"))
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

pub async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: CaptureOutput,
) -> (anyhow::Result<()>, CaptureOutput) {
    let _cwd = CwdGuard::enter(fx.project_root());
    let result = handle_soundbank_command(&parse(args), None, input, &output).await;
    (result, output)
}

pub async fn run(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
) -> (anyhow::Result<()>, CaptureOutput) {
    run_with(fx, args, input, CaptureOutput::interactive()).await
}

/// Creates bank `bank` non-interactively with the extra `args`.
pub async fn create_ok(fx: &AssetTestFixture, args: &[&str]) {
    let mut full = vec!["create", "bank"];
    full.extend_from_slice(args);
    let (result, _out) = run(fx, &full, &NonInteractiveInput).await;
    result.unwrap();
}

/// Updates bank `bank` non-interactively with the extra `args`.
pub async fn update_ok(fx: &AssetTestFixture, args: &[&str]) {
    let mut full = vec!["update", "bank"];
    full.extend_from_slice(args);
    let (result, _out) = run(fx, &full, &NonInteractiveInput).await;
    result.unwrap();
}

pub fn assert_flatc(fx: &AssetTestFixture, name: &str) {
    super::flatc::assert_flatc_accepts_schema("sound_bank_definition.fbs", &bank_path(fx, name));
}
