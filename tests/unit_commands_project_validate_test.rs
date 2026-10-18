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

//! `am project validate` covers every asset type and the root-level bus and
//! engine config files.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use am::app::{App, Commands};
use am::commands::project::handler;
use am::database::Database;
use am::input::NonInteractiveInput;
use clap::Parser;
use common::fixtures::{AssetTestFixture, CaptureOutput, MigratedDatabaseFixture};
use serde_json::{Value, json};

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

/// Runs `am project <args...>` from `dir` in JSON mode.
async fn run(dir: &Path, args: &[&str]) -> (anyhow::Result<()>, CaptureOutput) {
    run_with_db(dir, args, None).await
}

async fn run_with_db(
    dir: &Path,
    args: &[&str],
    database: Option<Arc<Database>>,
) -> (anyhow::Result<()>, CaptureOutput) {
    let mut full = vec!["am", "project"];
    full.extend_from_slice(args);
    let Commands::Project { command } = App::try_parse_from(full).expect("should parse").command
    else {
        panic!("expected a project command");
    };
    let _cwd = CwdGuard::enter(dir);
    let output = CaptureOutput::json();
    let result = handler(&command, database, &NonInteractiveInput, &output).await;
    (result, output)
}

fn report(output: &CaptureOutput) -> Value {
    output.last_success().expect("validate prints its report")
}

fn error_files(report: &Value) -> Vec<String> {
    report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["file"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn test_a_freshly_initialised_project_validates_clean() {
    let dir = tempfile::tempdir().unwrap();
    let db = MigratedDatabaseFixture::new().await.unwrap();
    let (init, _) = run_with_db(
        dir.path(),
        &["init", "demo", "--template", "default", "--no-register"],
        Some(db.database()),
    )
    .await;
    init.unwrap();
    let project = dir.path().join("demo");
    assert!(project.join(".amproject").exists());

    let (result, output) = run(&project, &["validate"]).await;
    let report = report(&output);
    result.unwrap_or_else(|e| panic!("{e:#}\n{report}"));
    assert_eq!(report["valid"], true, "{report}");
    assert_eq!(report["summary"]["pipelines"], 1, "{report}");
    assert_eq!(report["summary"]["buses"], 1, "{report}");
    assert_eq!(report["summary"]["engine_configs"], 1, "{report}");
}

#[tokio::test]
async fn test_attenuation_rtpc_and_pipeline_rules_are_checked() {
    let fx = AssetTestFixture::new("validate_new_types").unwrap();
    fx.write_asset_json(
        "rtpc",
        "wind",
        json!({"id": 1, "name": "wind", "min_value": 5.0, "max_value": 5.0, "default_value": 5.0}),
    )
    .unwrap();
    fx.write_asset_json("attenuators", "room", json!({"id": 2, "name": "room"}))
        .unwrap();
    fx.write_asset_json("pipelines", "bad", json!({"id": 3, "name": "bad"}))
        .unwrap();

    let (result, output) = run(fx.project_root(), &["validate"]).await;
    assert!(result.is_err());
    let report = report(&output);
    assert_eq!(report["valid"], false);
    let files = error_files(&report);
    assert!(files.iter().any(|f| f.ends_with("rtpc/wind.json")), "{files:?}");
    for dir in ["rtpc/wind.json", "attenuators/room.json", "pipelines/bad.json"] {
        assert!(files.iter().any(|f| f.ends_with(dir)), "{dir} missing: {report}");
    }
    assert_eq!(report["summary"]["rtpc"], 1);
    assert_eq!(report["summary"]["attenuators"], 1);
    assert_eq!(report["summary"]["pipelines"], 1);
}

#[tokio::test]
async fn test_rtpc_only_filter_ignores_other_types() {
    let fx = AssetTestFixture::new("validate_rtpc_only").unwrap();
    fx.write_asset_json(
        "rtpc",
        "wind",
        json!({"id": 1, "name": "wind", "min_value": 0.0, "max_value": 10.0, "default_value": 1.0}),
    )
    .unwrap();
    fx.write_asset_json("attenuators", "room", json!({"id": 2, "name": "room"}))
        .unwrap();

    let (result, output) = run(fx.project_root(), &["validate", "--rtpc-only"]).await;
    result.unwrap_or_else(|e| panic!("{e:#}"));
    let report = report(&output);
    assert_eq!(report["valid"], true, "{report}");
    assert_eq!(report["total_validated"], 1);
}

#[tokio::test]
async fn test_a_broken_bus_file_is_reported() {
    let fx = AssetTestFixture::new("validate_bad_buses").unwrap();
    std::fs::write(
        fx.project_root().join("broken.buses.json"),
        r#"{"buses": [{"id": 0, "name": "x"}]}"#,
    )
    .unwrap();

    let (result, output) = run(fx.project_root(), &["validate", "--buses-only"]).await;
    assert!(result.is_err());
    let report = report(&output);
    assert_eq!(report["valid"], false);
    assert!(
        error_files(&report).contains(&"broken.buses.json".to_string()),
        "{report}"
    );
    assert_eq!(report["errors"][0]["type"], "buses");
}

#[tokio::test]
async fn test_a_broken_engine_config_is_reported_and_json_syntax_too() {
    let fx = AssetTestFixture::new("validate_bad_config").unwrap();
    std::fs::write(fx.project_root().join("pc.config.json"), "{}").unwrap();
    std::fs::write(fx.project_root().join("junk.config.json"), "{").unwrap();

    let (result, output) = run(fx.project_root(), &["validate", "--engine-configs-only"]).await;
    assert!(result.is_err());
    let report = report(&output);
    let files = error_files(&report);
    assert!(files.contains(&"pc.config.json".to_string()), "{files:?}");
    assert!(
        report["errors"].as_array().unwrap().len() >= 2,
        "unparsable file must be reported too: {report}"
    );
}
