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

//! Tests for `am asset pipeline create|list|update|delete`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};

use am::app::{App, Commands};
use am::assets::Pipeline;
use am::commands::asset::{AssetCommands, PipelineCommands, handle_pipeline_command};
use am::common::errors::{as_cli_error, codes};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use common::fixtures::{AssetTestFixture, CaptureOutput};
use common::flatc::assert_flatc_accepts_schema;
use common::scripted_input::ScriptedInput;
use serde_json::json;

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

fn create_args() -> Vec<&'static str> {
    vec![
        "create",
        "main",
        "--node",
        "1:Input",
        "--node",
        "2:Reverb:1:0.5,0.25",
        "--node",
        "3:Output:2",
    ]
}

fn parse(args: &[&str]) -> PipelineCommands {
    let mut full = vec!["am", "asset", "pipeline"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Pipeline { command },
        } => command,
        _ => panic!("expected a pipeline command"),
    }
}

fn pipeline_path(fx: &AssetTestFixture, name: &str) -> PathBuf {
    fx.project_root()
        .join("sources/pipelines")
        .join(format!("{name}.json"))
}

fn read_raw(fx: &AssetTestFixture, name: &str) -> String {
    std::fs::read_to_string(pipeline_path(fx, name)).unwrap()
}

fn read_pipeline(fx: &AssetTestFixture, name: &str) -> Pipeline {
    serde_json::from_str(&read_raw(fx, name)).unwrap()
}

fn node_ids(p: &Pipeline) -> Vec<u64> {
    p.nodes.as_ref().unwrap().iter().map(|n| n.id).collect()
}

async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: &CaptureOutput,
) -> anyhow::Result<()> {
    let _cwd = CwdGuard::enter(fx.project_root());
    handle_pipeline_command(&parse(args), None, input, output).await
}

async fn run(fx: &AssetTestFixture, args: &[&str]) -> (anyhow::Result<()>, CaptureOutput) {
    let output = CaptureOutput::interactive();
    let result = run_with(fx, args, &NonInteractiveInput, &output).await;
    (result, output)
}

// ---------------------------------------------------------------------------
// Clap surface
// ---------------------------------------------------------------------------

#[test]
fn test_create_parses_node_flags() {
    let PipelineCommands::Create { name, args } = parse(&create_args()) else {
        panic!("expected create");
    };
    assert_eq!(name, "main");
    assert_eq!(args.node.len(), 3);
    assert!(args.from_json.is_none());

    let PipelineCommands::Create { args, .. } = parse(&["create", "p", "--from-json", "n.json"])
    else {
        panic!("expected create");
    };
    assert!(args.from_json.is_some());
}

#[test]
fn test_node_and_from_json_conflict() {
    let result = App::try_parse_from([
        "am",
        "asset",
        "pipeline",
        "create",
        "p",
        "--node",
        "1:Input",
        "--from-json",
        "n.json",
    ]);
    assert!(result.is_err());
}

#[test]
fn test_update_list_delete_parse() {
    let PipelineCommands::Update { name, args } = parse(&[
        "update",
        "main",
        "--add-node",
        "4:Clamp:2",
        "--remove-node",
        "5",
        "--remove-node",
        "6",
    ]) else {
        panic!("expected update");
    };
    assert_eq!(name, "main");
    assert_eq!(args.add_node, vec!["4:Clamp:2".to_string()]);
    assert_eq!(args.remove_node, vec![5, 6]);
    assert!(matches!(parse(&["list"]), PipelineCommands::List {}));
    let PipelineCommands::Delete { name, force } = parse(&["delete", "main", "--force"]) else {
        panic!("expected delete");
    };
    assert_eq!(name, "main");
    assert!(force);
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_writes_a_file_that_round_trips() {
    let fx = AssetTestFixture::new("pipe_create").unwrap();
    let (result, _out) = run(&fx, &create_args()).await;
    result.unwrap();

    let p = read_pipeline(&fx, "main");
    assert_ne!(p.id, 0);
    assert_eq!(p.name.as_deref(), Some("main"));
    let nodes = p.nodes.unwrap();
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[1].name.as_deref(), Some("Reverb"));
    assert_eq!(nodes[1].consume, Some(vec![1]));
    assert_eq!(nodes[1].parameters, Some(vec![0.5, 0.25]));
    assert_eq!(nodes[0].consume, Some(vec![]));
}

#[tokio::test]
async fn test_created_file_is_accepted_by_flatc() {
    let fx = AssetTestFixture::new("pipe_flatc").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    assert_flatc_accepts_schema("pipeline_definition.fbs", &pipeline_path(&fx, "main"));
}

#[tokio::test]
async fn test_create_from_json_array_and_object() {
    let fx = AssetTestFixture::new("pipe_from_json").unwrap();
    let nodes = json!([
        {"id": 1, "name": "Input", "consume": []},
        {"id": 2, "name": "Output", "consume": [1]},
    ]);
    let array = fx.project_root().join("nodes.json");
    std::fs::write(&array, nodes.to_string()).unwrap();
    run(
        &fx,
        &["create", "a", "--from-json", array.to_str().unwrap()],
    )
    .await
    .0
    .unwrap();
    assert_eq!(node_ids(&read_pipeline(&fx, "a")), vec![1, 2]);

    let object = fx.project_root().join("whole.json");
    std::fs::write(
        &object,
        json!({"name": "ignored", "nodes": nodes}).to_string(),
    )
    .unwrap();
    run(
        &fx,
        &["create", "b", "--from-json", object.to_str().unwrap()],
    )
    .await
    .0
    .unwrap();
    let b = read_pipeline(&fx, "b");
    assert_eq!(b.name.as_deref(), Some("b"));
    assert_eq!(node_ids(&b), vec![1, 2]);
}

#[tokio::test]
async fn test_from_json_with_a_bad_graph_is_rejected_and_nothing_written() {
    let fx = AssetTestFixture::new("pipe_from_json_bad").unwrap();
    let file = fx.project_root().join("bad.json");
    std::fs::write(
        &file,
        json!([{"id": 1, "name": "Input", "consume": []}]).to_string(),
    )
    .unwrap();
    let (result, _out) = run(&fx, &["create", "a", "--from-json", file.to_str().unwrap()]).await;
    assert!(result.is_err());
    assert!(!pipeline_path(&fx, "a").exists());

    let (result, _out) = run(&fx, &["create", "a", "--from-json", "/no/such/file.json"]).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--from-json"));
}

#[tokio::test]
async fn test_missing_nodes_name_the_flag() {
    let fx = AssetTestFixture::new("pipe_missing").unwrap();
    let (result, _out) = run(&fx, &["create", "main"]).await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("--node"), "{err:#}");
    assert!(!pipeline_path(&fx, "main").exists());
}

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = AssetTestFixture::new("pipe_json_noprompt").unwrap();
    let input = ScriptedInput::new(&[], &[]);
    let output = CaptureOutput::json();
    let result = run_with(&fx, &["create", "main"], &input, &output).await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
    assert!(!pipeline_path(&fx, "main").exists());
}

#[tokio::test]
async fn test_bad_node_syntax_and_bad_graph_write_nothing() {
    let fx = AssetTestFixture::new("pipe_bad_node").unwrap();
    let (result, _out) = run(&fx, &["create", "main", "--node", "oops"]).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--node"));
    assert!(!pipeline_path(&fx, "main").exists());

    // No Output node.
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "main",
            "--node",
            "1:Input",
            "--node",
            "2:Reverb:1",
        ],
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("Output"));
    assert!(!pipeline_path(&fx, "main").exists());
}

#[tokio::test]
async fn test_unknown_node_type_is_accepted() {
    let fx = AssetTestFixture::new("pipe_plugin_node").unwrap();
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "main",
            "--node",
            "1:Input",
            "--node",
            "2:MyPluginNode:1",
            "--node",
            "3:Output:2",
        ],
    )
    .await;
    result.unwrap();
    assert_eq!(node_ids(&read_pipeline(&fx, "main")), vec![1, 2, 3]);
}

#[tokio::test]
async fn test_create_rejects_duplicate_name() {
    let fx = AssetTestFixture::new("pipe_dup").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &create_args()).await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_ASSET_ALREADY_EXISTS
    );
}

#[tokio::test]
async fn test_interactive_create_adds_nodes_one_at_a_time() {
    let fx = AssetTestFixture::new("pipe_walk").unwrap();
    // Node ids are assigned 1, 2, 3; consumers are picked from earlier nodes.
    let input = ScriptedInput::new(
        &[
            "Input", "Reverb", "1 Input", // reverb consumes the input
            "Output", "2 Reverb", // output consumes the reverb
        ],
        &[
            "",          // input parameters
            "0.5, 0.25", // reverb parameters
            "",          // output parameters
        ],
    )
    .with_confirms(&[true, true, false]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["create", "walk"], &input, &output)
        .await
        .unwrap();
    input.assert_all_consumed();
    assert!(input.saw_prompt("add another node"));

    let nodes = read_pipeline(&fx, "walk").nodes.unwrap();
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[1].consume, Some(vec![1]));
    assert_eq!(nodes[1].parameters, Some(vec![0.5, 0.25]));
    assert_eq!(nodes[2].consume, Some(vec![2]));
    // Consumers were offered only from nodes already entered.
    let seen = input.options_seen.lock().unwrap();
    let consumer_options = seen
        .iter()
        .find(|o| o.iter().any(|s| s == "1 Input") && !o.iter().any(|s| s == "Reverb"))
        .expect("a consumer pick list");
    assert!(!consumer_options.iter().any(|s| s.starts_with("2 ")));
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_json_returns_created_pipeline() {
    let fx = AssetTestFixture::new("pipe_list").unwrap();
    run(&fx, &create_args()).await.0.unwrap();

    let output = CaptureOutput::json();
    run_with(&fx, &["list"], &NonInteractiveInput, &output)
        .await
        .unwrap();
    let data = output.last_success().unwrap();
    assert_eq!(data["count"], 1);
    let first = &data["pipelines"][0];
    assert_eq!(first["name"], "main");
    assert_eq!(first["node_count"], 3);
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_add_and_remove_nodes() {
    let fx = AssetTestFixture::new("pipe_update").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let before = read_pipeline(&fx, "main");

    run(&fx, &["update", "main", "--add-node", "4:Clamp:1"])
        .await
        .0
        .unwrap();
    let after = read_pipeline(&fx, "main");
    assert_eq!(node_ids(&after), vec![1, 2, 3, 4]);
    assert_eq!(after.id, before.id);

    run(&fx, &["update", "main", "--remove-node", "4"])
        .await
        .0
        .unwrap();
    assert_eq!(node_ids(&read_pipeline(&fx, "main")), vec![1, 2, 3]);
}

#[tokio::test]
async fn test_update_node_flag_replaces_the_whole_list() {
    let fx = AssetTestFixture::new("pipe_replace").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    run(
        &fx,
        &[
            "update",
            "main",
            "--node",
            "1:Input",
            "--node",
            "2:Output:1",
        ],
    )
    .await
    .0
    .unwrap();
    assert_eq!(node_ids(&read_pipeline(&fx, "main")), vec![1, 2]);
}

#[tokio::test]
async fn test_update_remove_of_a_consumed_node_names_consumers_and_keeps_file() {
    let fx = AssetTestFixture::new("pipe_remove_used").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let before = read_raw(&fx, "main");

    let (result, _out) = run(&fx, &["update", "main", "--remove-node", "2"]).await;
    let text = format!("{:#}", result.unwrap_err());
    assert!(text.contains("Output"), "{text}");
    assert_eq!(read_raw(&fx, "main"), before);
}

#[tokio::test]
async fn test_update_validates_the_new_graph() {
    let fx = AssetTestFixture::new("pipe_update_bad").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let before = read_raw(&fx, "main");
    let (result, _out) = run(&fx, &["update", "main", "--add-node", "4:Clamp:99"]).await;
    assert!(result.is_err());
    assert_eq!(read_raw(&fx, "main"), before);
}

#[tokio::test]
async fn test_update_preserves_unknown_keys_and_key_order() {
    let fx = AssetTestFixture::new("pipe_roundtrip").unwrap();
    fx.write_asset_json(
        "pipelines",
        "hand",
        json!({
            "zeta_note": "keep me",
            "id": 55,
            "name": "hand",
            "nodes": [
                {"id": 1, "name": "Input", "consume": []},
                {"id": 2, "name": "Output", "consume": [1]},
            ],
            "alpha_note": {"a": 1},
        }),
    )
    .unwrap();

    run(&fx, &["update", "hand", "--add-node", "3:Clamp:1"])
        .await
        .0
        .unwrap();
    let text = read_raw(&fx, "hand");
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["zeta_note"], "keep me");
    assert_eq!(value["alpha_note"], json!({"a": 1}));
    assert_eq!(value["id"], 55);
    assert_eq!(value["nodes"].as_array().unwrap().len(), 3);
    let pos = |k: &str| text.find(&format!("\"{k}\"")).unwrap();
    // Known keys come first (schema order); unknown keys keep their own relative order.
    assert!(
        pos("id") < pos("name") && pos("name") < pos("nodes"),
        "{text}"
    );
    assert!(pos("zeta_note") < pos("alpha_note"), "{text}");
    assert!(pos("nodes") < pos("zeta_note"), "{text}");
}

#[tokio::test]
async fn test_update_without_changes_errors_non_interactively() {
    let fx = AssetTestFixture::new("pipe_update_none").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &["update", "main"]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_unknown_pipeline_is_not_found() {
    let fx = AssetTestFixture::new("pipe_update_missing").unwrap();
    let (result, _out) = run(&fx, &["update", "nope", "--add-node", "1:Input"]).await;
    let err = result.unwrap_err();
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_ASSET_NOT_FOUND);
}

#[tokio::test]
async fn test_interactive_update_adds_edits_and_removes_nodes() {
    let fx = AssetTestFixture::new("pipe_update_walk").unwrap();
    run(&fx, &create_args()).await.0.unwrap();

    let input = ScriptedInput::new(
        &[
            "Add a node",
            "Clamp",
            "1 Input", // new node 4 consumes the input
            "Edit a node",
            "2 Reverb",
            "1 Input", // keep the same consumer
            "Remove a node",
            "4 Clamp",
            "Done",
        ],
        &[
            "",     // new node parameters
            "0.75", // edited reverb parameters
        ],
    );
    let output = CaptureOutput::interactive();
    run_with(&fx, &["update", "main"], &input, &output)
        .await
        .unwrap();
    input.assert_all_consumed();

    let nodes = read_pipeline(&fx, "main").nodes.unwrap();
    assert_eq!(
        nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(nodes[1].parameters, Some(vec![0.75]));
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_with_force_removes_the_file() {
    let fx = AssetTestFixture::new("pipe_delete").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    run(&fx, &["delete", "main", "--force"]).await.0.unwrap();
    assert!(!pipeline_path(&fx, "main").exists());
}

#[tokio::test]
async fn test_delete_without_force_needs_confirmation_non_interactively() {
    let fx = AssetTestFixture::new("pipe_delete_noforce").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &["delete", "main"]).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--force"));
    assert!(pipeline_path(&fx, "main").exists());
}

#[tokio::test]
async fn test_delete_declined_confirmation_keeps_the_file() {
    let fx = AssetTestFixture::new("pipe_delete_decline").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let input = ScriptedInput::new(&[], &[]).with_confirms(&[false]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["delete", "main"], &input, &output)
        .await
        .unwrap();
    assert!(pipeline_path(&fx, "main").exists());
}

#[tokio::test]
async fn test_delete_unknown_pipeline_is_not_found() {
    let fx = AssetTestFixture::new("pipe_delete_missing").unwrap();
    let (result, _out) = run(&fx, &["delete", "nope", "--force"]).await;
    let err = result.unwrap_err();
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_ASSET_NOT_FOUND);
}
