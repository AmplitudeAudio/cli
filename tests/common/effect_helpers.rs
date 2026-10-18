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

//! Shared helpers of the effect command tests.

use am::app::{App, Commands};
use am::assets::Effect;
use am::commands::asset::{AssetCommands, EffectCommands, handle_effect_command};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use serde_json::{Value, json};

use super::collection_helpers::CwdGuard;
use super::fixtures::{AssetTestFixture, CaptureOutput};

pub fn parse(args: &[&str]) -> EffectCommands {
    let mut full = vec!["am", "asset", "effect"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Effect { command },
        } => command,
        _ => panic!("expected an effect command"),
    }
}

/// A project with RTPCs `speed` (401) and `wind` (402).
pub fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.write_asset_json("rtpc", "speed", json!({"id": 401, "name": "speed"}))
        .unwrap();
    fx.write_asset_json("rtpc", "wind", json!({"id": 402, "name": "wind"}))
        .unwrap();
    fx
}

pub fn effect_path(fx: &AssetTestFixture, name: &str) -> std::path::PathBuf {
    fx.project_root()
        .join("sources/effects")
        .join(format!("{name}.json"))
}

pub fn read_effect(fx: &AssetTestFixture, name: &str) -> Effect {
    serde_json::from_str(&std::fs::read_to_string(effect_path(fx, name)).unwrap()).unwrap()
}

pub fn read_raw(fx: &AssetTestFixture, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(effect_path(fx, name)).unwrap()).unwrap()
}

pub async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: CaptureOutput,
) -> (anyhow::Result<()>, CaptureOutput) {
    let _cwd = CwdGuard::enter(fx.project_root());
    let result = handle_effect_command(&parse(args), None, input, &output).await;
    (result, output)
}

pub async fn run(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
) -> (anyhow::Result<()>, CaptureOutput) {
    run_with(fx, args, input, CaptureOutput::interactive()).await
}

/// Creates effect `fx1` (a `Delay` filter) non-interactively with the extra `args`.
pub async fn create_ok(fx: &AssetTestFixture, args: &[&str]) {
    let mut full = vec!["create", "fx1", "--effect-type", "Delay"];
    full.extend_from_slice(args);
    let (result, _out) = run(fx, &full, &NonInteractiveInput).await;
    result.unwrap();
}

pub fn assert_flatc(fx: &AssetTestFixture, name: &str) {
    super::flatc::assert_flatc_accepts_schema("effect_definition.fbs", &effect_path(fx, name));
}
