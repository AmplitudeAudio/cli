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

//! Shared helpers of the switch command tests.

use am::app::{App, Commands};
use am::assets::Switch;
use am::commands::asset::{AssetCommands, SwitchCommands, handle_switch_command};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use serde_json::Value;

use super::collection_helpers::CwdGuard;
use super::fixtures::{AssetTestFixture, CaptureOutput};

pub fn parse(args: &[&str]) -> SwitchCommands {
    let mut full = vec!["am", "asset", "switch"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Switch { command },
        } => command,
        _ => panic!("expected a switch command"),
    }
}

pub fn fixture(name: &str) -> AssetTestFixture {
    AssetTestFixture::new(name).unwrap()
}

pub fn switch_path(fx: &AssetTestFixture, name: &str) -> std::path::PathBuf {
    fx.project_root()
        .join("sources/switches")
        .join(format!("{name}.json"))
}

pub fn read_switch(fx: &AssetTestFixture, name: &str) -> Switch {
    serde_json::from_str(&std::fs::read_to_string(switch_path(fx, name)).unwrap()).unwrap()
}

pub fn read_raw(fx: &AssetTestFixture, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(switch_path(fx, name)).unwrap()).unwrap()
}

/// `(id, name)` of every state of switch `name`, in file order.
pub fn states_of(fx: &AssetTestFixture, name: &str) -> Vec<(u64, String)> {
    read_switch(fx, name)
        .states
        .unwrap_or_default()
        .into_iter()
        .map(|s| (s.id, s.name.unwrap_or_default()))
        .collect()
}

pub async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: CaptureOutput,
) -> (anyhow::Result<()>, CaptureOutput) {
    let _cwd = CwdGuard::enter(fx.project_root());
    let result = handle_switch_command(&parse(args), None, input, &output).await;
    (result, output)
}

pub async fn run(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
) -> (anyhow::Result<()>, CaptureOutput) {
    run_with(fx, args, input, CaptureOutput::interactive()).await
}

/// Creates switch `sw` non-interactively with the extra `args`.
pub async fn create_ok(fx: &AssetTestFixture, args: &[&str]) {
    let mut full = vec!["create", "sw"];
    full.extend_from_slice(args);
    let (result, _out) = run(fx, &full, &NonInteractiveInput).await;
    result.unwrap();
}

pub fn assert_flatc(fx: &AssetTestFixture, name: &str) {
    super::flatc::assert_flatc_accepts_schema("switch_definition.fbs", &switch_path(fx, name));
}
