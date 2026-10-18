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

//! End-to-end flow on a project created by `am project init` (default
//! template), driven through the real command handlers.
//!
//! The bus and engine config files that `init` writes live in the sources
//! dir; every command must find them there.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};

use am::app::{App, Commands};
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use clap::Parser;
use common::fixtures::{CaptureOutput, MigratedDatabaseFixture};
use serde_json::Value;

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

struct Flow {
    db: MigratedDatabaseFixture,
    workspace: tempfile::TempDir,
}

impl Flow {
    fn project(&self) -> PathBuf {
        self.workspace.path().join("game")
    }

    fn sources(&self) -> PathBuf {
        self.project().join("sources")
    }

    /// Runs `am <args>` from `dir`, returning the result and what was printed.
    async fn am_in(&self, dir: &Path, args: &[&str]) -> (anyhow::Result<()>, CaptureOutput) {
        let mut argv = vec!["am"];
        argv.extend_from_slice(args);
        let command = App::try_parse_from(argv).expect("should parse").command;
        let output = CaptureOutput::json();
        let _cwd = CwdGuard::enter(dir);
        let db = Some(self.db.database());
        let input = NonInteractiveInput;
        let result = match &command {
            Commands::Asset { command } => {
                am::commands::asset::handler(command, db, &input, &output).await
            }
            Commands::Project { command } => {
                am::commands::project::handler(command, db, &input, &output).await
            }
            _ => panic!("unexpected command"),
        };
        (result, output)
    }

    async fn am(&self, args: &[&str]) -> (anyhow::Result<()>, CaptureOutput) {
        self.am_in(&self.project(), args).await
    }

    async fn ok(&self, args: &[&str]) -> CaptureOutput {
        let (result, output) = self.am(args).await;
        if let Err(e) = result {
            panic!("`am {}` failed: {e:#}", args.join(" "));
        }
        output
    }
}

async fn fresh_project() -> Flow {
    let flow = Flow {
        db: MigratedDatabaseFixture::new().await.unwrap(),
        workspace: tempfile::tempdir().unwrap(),
    };
    let (result, _) = flow
        .am_in(
            flow.workspace.path(),
            &[
                "project",
                "init",
                "game",
                "--template",
                "default",
                "--no-register",
            ],
        )
        .await;
    result.expect("project init should succeed");
    flow
}

fn code_of(err: &anyhow::Error) -> i32 {
    as_cli_error(err).expect("a CliError").code
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn names(output: &CaptureOutput, key: &str, field: &str) -> Vec<String> {
    output.last_success().unwrap()[key]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|r| r[field].as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn test_init_places_bus_and_config_in_sources_dir() {
    let flow = fresh_project().await;
    assert!(flow.sources().join("pc.buses.json").exists());
    assert!(flow.sources().join("pc.config.json").exists());
    assert!(!flow.project().join("pc.buses.json").exists());
}

#[tokio::test]
async fn test_bus_list_and_engine_config_list_see_template_files() {
    let flow = fresh_project().await;

    let buses = names(&flow.ok(&["asset", "bus", "list"]).await, "buses", "name");
    for expected in ["master", "voices", "environment"] {
        assert!(buses.contains(&expected.to_string()), "buses: {buses:?}");
    }

    let configs = names(
        &flow.ok(&["asset", "engine-config", "list"]).await,
        "files",
        "file",
    );
    assert_eq!(configs, vec!["pc".to_string()]);
}

#[tokio::test]
async fn test_full_flow_on_fresh_project() {
    let flow = fresh_project().await;

    // A new bus is written next to the template's bus file.
    flow.ok(&["asset", "bus", "create", "sfx"]).await;
    let file = read_json(&flow.sources().join("pc.buses.json"));
    assert!(
        file["buses"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["name"] == "sfx")
    );
    assert!(!flow.project().join("pc.buses.json").exists());

    // Assets can use the template's buses and the new one.
    std::fs::write(flow.project().join("data").join("step.wav"), b"").unwrap();
    flow.ok(&[
        "asset", "sound", "create", "footstep", "--file", "step.wav", "--bus", "sfx",
    ])
    .await;
    flow.ok(&[
        "asset", "sound", "create", "x", "--file", "step.wav", "--bus", "master",
    ])
    .await;
    flow.ok(&["asset", "collection", "create", "steps", "--bus", "sfx"])
        .await;
    flow.ok(&[
        "asset",
        "switch",
        "create",
        "surface",
        "--states",
        "wood,stone",
    ])
    .await;
    flow.ok(&[
        "asset",
        "switch-container",
        "create",
        "surface_steps",
        "--bus",
        "sfx",
        "--switch-group",
        "surface",
    ])
    .await;

    // The template config uses its pipeline and bus file.
    // The template's own pipeline is `pc` and builds to `pc.ampipeline`, the
    // file its config names.
    assert!(flow.sources().join("pipelines/pc.json").exists());
    let listed = names(&flow.ok(&["asset", "pipeline", "list"]).await, "pipelines", "name");
    assert_eq!(listed, vec!["pc".to_string()]);
    let (result, _)= flow.am(&["asset", "pipeline", "delete", "pc"]).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_ASSET_IN_USE);
    let (result, _) = flow.am(&["asset", "bus-file", "delete", "pc"]).await;
    assert!(result.is_err(), "the bus file in use must not be deleted");

    // A second bus file is created in the sources dir.
    flow.ok(&["asset", "bus-file", "create", "extra"]).await;
    assert!(flow.sources().join("extra.buses.json").exists());

    // Everything validates.
    let out = flow.ok(&["project", "validate"]).await;
    let result = out.last_success().unwrap();
    assert_eq!(result["valid"], true, "validate: {result}");
    assert!(result["errors"].as_array().unwrap().is_empty());
    assert_eq!(result["summary"]["buses"], 2);
    assert_eq!(result["summary"]["engine_configs"], 1);
}

#[tokio::test]
async fn test_build_compiles_the_files_in_the_sources_dir() {
    let flow = fresh_project().await;
    flow.ok(&["asset", "bus", "create", "sfx"]).await;
    let (result, _) = flow.am(&["project", "build"]).await;
    if let Err(e) = &result
        && matches!(
            as_cli_error(e).map(|c| c.code),
            Some(codes::ERR_SDK_NOT_FOUND)
        )
    {
        eprintln!("skipped build check: SDK is not available");
        return;
    }
    result.expect("am project build should succeed on a fresh project");
    let build = flow.project().join("build");
    assert!(build.join("pc.buses.ambus").exists(), "compiled buses file");
    assert!(build.join("pc.config.amconfig").exists(), "compiled config");
    assert!(
        build.join("pipelines/pc.ampipeline").exists(),
        "the template pipeline builds to the file its config names"
    );
}
