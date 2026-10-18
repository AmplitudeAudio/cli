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

//! Shared helpers of the collection command tests.

use std::path::Path;

use am::app::{App, Commands};
use am::assets::Collection;
use am::commands::asset::{AssetCommands, CollectionCommands, handle_collection_command};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use serde_json::{Value, json};

use super::fixtures::{AssetTestFixture, CaptureOutput};

/// Restores the working directory when dropped.
pub struct CwdGuard(Option<std::path::PathBuf>);

impl CwdGuard {
    pub fn enter(path: &Path) -> Self {
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

pub fn parse(args: &[&str]) -> CollectionCommands {
    let mut full = vec!["am", "asset", "collection"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Collection { command },
        } => command,
        _ => panic!("expected a collection command"),
    }
}

pub fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.create_test_sound("step1", 101).unwrap();
    fx.create_test_sound("step2", 102).unwrap();
    fx.write_asset_json("attenuators", "short", json!({"id": 55, "name": "short"}))
        .unwrap();
    fx.write_asset_json("effects", "echo", json!({"id": 66, "name": "echo"}))
        .unwrap();
    fx.write_asset_json("rtpc", "speed", json!({"id": 77, "name": "speed"}))
        .unwrap();
    fx
}

pub fn collection_path(fx: &AssetTestFixture, name: &str) -> std::path::PathBuf {
    fx.project_root()
        .join("sources/collections")
        .join(format!("{name}.json"))
}

pub fn read_collection(fx: &AssetTestFixture, name: &str) -> Collection {
    serde_json::from_str(&std::fs::read_to_string(collection_path(fx, name)).unwrap()).unwrap()
}

pub fn read_raw(fx: &AssetTestFixture, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(collection_path(fx, name)).unwrap()).unwrap()
}

pub async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: CaptureOutput,
) -> (anyhow::Result<()>, CaptureOutput) {
    let _cwd = CwdGuard::enter(fx.project_root());
    let result = handle_collection_command(&parse(args), None, input, &output).await;
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
        "collection_definition.fbs",
        &collection_path(fx, name),
    );
}

pub async fn rich_collection(fx: &AssetTestFixture) {
    create_ok(
        fx,
        &[
            "--bus",
            "sfx",
            "--gain",
            "0.4",
            "--avoid-repeat",
            "true",
            "--repeat-count",
            "2",
            "--entry",
            "step1,weight=3",
            "--entry",
            "step2",
        ],
    )
    .await;
}
