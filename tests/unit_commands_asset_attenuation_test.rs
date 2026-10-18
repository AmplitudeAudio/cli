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

//! Tests for `am asset attenuation create|list|update|delete`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use am::app::{App, Commands};
use am::assets::Attenuation;
use am::commands::asset::{AssetCommands, AttenuationCommands, handle_attenuation_command};
use am::common::errors::{as_cli_error, codes};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use common::fixtures::{AssetTestFixture, CaptureOutput};
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
        "short",
        "--max-distance",
        "50",
        "--shape",
        "sphere",
        "--inner-radius",
        "5",
        "--outer-radius",
        "10",
        "--curve-part",
        "0,1,1,0,Linear",
    ]
}

fn parse(args: &[&str]) -> AttenuationCommands {
    let mut full = vec!["am", "asset", "attenuation"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Attenuation { command },
        } => command,
        _ => panic!("expected an attenuation command"),
    }
}

fn att_path(fx: &AssetTestFixture, name: &str) -> PathBuf {
    fx.project_root()
        .join("sources/attenuators")
        .join(format!("{name}.json"))
}

fn read_att(fx: &AssetTestFixture, name: &str) -> Attenuation {
    serde_json::from_str(&std::fs::read_to_string(att_path(fx, name)).unwrap()).unwrap()
}

async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: &CaptureOutput,
) -> anyhow::Result<()> {
    let _cwd = CwdGuard::enter(fx.project_root());
    handle_attenuation_command(&parse(args), None, input, output).await
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
fn test_create_parses_every_flag() {
    let cmd = parse(&[
        "create",
        "short",
        "--max-distance",
        "50",
        "--shape",
        "box",
        "--inner-half-width",
        "1",
        "--max-attenuation-factor",
        "0.5",
        "--curve-part",
        "0,1,1,0,Linear",
        "--curve-part",
        "1,0,2,0,Linear",
        "--curve-from-json",
        "gain.json",
        "--spread-curve-part",
        "0,1,1,1,Linear",
        "--spread-curve-from-json",
        "spread.json",
        "--air-absorption",
        "false",
        "--air-absorption-coefficients",
        "-0.1,0.2,0.3",
    ]);
    let AttenuationCommands::Create { name, args } = cmd else {
        panic!("expected create");
    };
    assert_eq!(name, "short");
    assert_eq!(args.max_distance, Some(50.0));
    assert_eq!(args.zone.shape.as_deref(), Some("box"));
    assert_eq!(args.zone.inner_half_width, Some(1.0));
    assert_eq!(args.max_attenuation_factor, Some(0.5));
    assert_eq!(args.curve_part.len(), 2);
    assert!(args.curve_from_json.is_some());
    assert_eq!(args.spread_curve_part.len(), 1);
    assert!(args.spread_curve_from_json.is_some());
    assert_eq!(args.air_absorption, Some(false));
    assert_eq!(
        args.air_absorption_coefficients.as_deref(),
        Some("-0.1,0.2,0.3")
    );
}

#[test]
fn test_update_list_delete_parse() {
    assert!(matches!(
        parse(&["update", "short", "--max-distance", "9"]),
        AttenuationCommands::Update { .. }
    ));
    assert!(matches!(parse(&["list"]), AttenuationCommands::List {}));
    let AttenuationCommands::Delete { name, force } = parse(&["delete", "short", "--force"]) else {
        panic!("expected delete");
    };
    assert_eq!(name, "short");
    assert!(force);
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_writes_a_file_that_round_trips() {
    let fx = AssetTestFixture::new("att_create").unwrap();
    let (result, _out) = run(&fx, &create_args()).await;
    result.unwrap();

    let att = read_att(&fx, "short");
    assert_eq!(att.max_distance, 50.0);
    assert_ne!(att.id, 0);
    let shape = att.shape.unwrap();
    assert_eq!(shape.max_attenuation_factor, 1.0);
    assert_eq!(shape.zone.unwrap().tag(), "Sphere");
    assert_eq!(att.gain_curve.unwrap().parts.unwrap().len(), 1);
    assert!(att.spread_curve.is_none());
    assert!(att.air_absorption.is_none());
}

fn flatc() -> Option<PathBuf> {
    ["flatc", "/opt/homebrew/bin/flatc"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| Command::new(p).arg("--version").output().is_ok())
}

#[tokio::test]
async fn test_created_file_is_accepted_by_flatc() {
    let fx = AssetTestFixture::new("att_flatc").unwrap();
    let mut args = create_args();
    args.extend(["--air-absorption-coefficients", "0.1,0.2,0.3"]);
    args.extend(["--spread-curve-part", "0,1,5,0.5,Linear"]);
    let (result, _out) = run(&fx, &args).await;
    result.unwrap();

    let Some(flatc) = flatc() else {
        eprintln!("skipped flatc check: flatc is not installed");
        return;
    };
    let Ok(sdk) = std::env::var("AM_SDK_PATH") else {
        eprintln!("skipped flatc check: AM_SDK_PATH is not set");
        return;
    };
    let sdk = PathBuf::from(sdk);
    let Some(schema) = [sdk.join("schemas"), sdk.join("../schemas")]
        .into_iter()
        .map(|d| d.join("attenuation_definition.fbs"))
        .find(|p| p.exists())
    else {
        eprintln!("skipped flatc check: attenuation_definition.fbs not found");
        return;
    };
    let out_dir = tempfile::tempdir().unwrap();
    let out = Command::new(flatc)
        .args(["-b", "--strict-json", "-o"])
        .arg(out_dir.path())
        .arg("-I")
        .arg(schema.parent().unwrap())
        .arg(&schema)
        .arg(att_path(&fx, "short"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "flatc rejected the file: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn test_coefficients_with_two_values_are_rejected() {
    let fx = AssetTestFixture::new("att_coeff2").unwrap();
    let mut args = create_args();
    args.extend(["--air-absorption-coefficients", "0.1,0.2"]);
    let (result, _out) = run(&fx, &args).await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("--air-absorption-coefficients"));
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_FIELD
    );
    assert!(!att_path(&fx, "short").exists());
}

#[tokio::test]
async fn test_coefficients_must_be_finite() {
    let fx = AssetTestFixture::new("att_coeff_nan").unwrap();
    let mut args = create_args();
    args.extend(["--air-absorption-coefficients", "0.1,NaN,0.3"]);
    let (result, _out) = run(&fx, &args).await;
    assert!(result.is_err());
    assert!(!att_path(&fx, "short").exists());
}

#[tokio::test]
async fn test_air_absorption_true_without_coefficients_uses_sdk_defaults() {
    let fx = AssetTestFixture::new("att_air_true").unwrap();
    let mut args = create_args();
    args.extend(["--air-absorption", "true"]);
    let (result, _out) = run(&fx, &args).await;
    result.unwrap();
    let air = read_att(&fx, "short").air_absorption.unwrap();
    assert!(air.enabled);
    assert_eq!(air.coefficients.unwrap().len(), 3);
}

#[tokio::test]
async fn test_missing_required_values_name_the_flag() {
    let cases: [(&[&str], &str); 3] = [
        (
            &[
                "create",
                "a",
                "--shape",
                "sphere",
                "--inner-radius",
                "1",
                "--outer-radius",
                "2",
                "--curve-part",
                "0,1,1,0,Linear",
            ],
            "--max-distance",
        ),
        (
            &[
                "create",
                "a",
                "--max-distance",
                "5",
                "--curve-part",
                "0,1,1,0,Linear",
            ],
            "--shape",
        ),
        (
            &[
                "create",
                "a",
                "--max-distance",
                "5",
                "--shape",
                "sphere",
                "--inner-radius",
                "1",
                "--outer-radius",
                "2",
            ],
            "--curve-part",
        ),
    ];
    for (index, (args, flag)) in cases.into_iter().enumerate() {
        let fx = AssetTestFixture::new(&format!("att_missing_{index}")).unwrap();
        let (result, _out) = run(&fx, args).await;
        let err = result.unwrap_err();
        assert!(format!("{err:#}").contains(flag), "{flag}: {err:#}");
        assert!(!att_path(&fx, "a").exists());
    }
}

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = AssetTestFixture::new("att_json_noprompt").unwrap();
    // An input that would happily answer prompts; JSON mode must not ask.
    let input = ScriptedInput::new(&[], &[]);
    let output = CaptureOutput::json();
    let result = run_with(
        &fx,
        &["create", "a", "--max-distance", "5"],
        &input,
        &output,
    )
    .await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
    assert!(!att_path(&fx, "a").exists());
}

#[tokio::test]
async fn test_create_rejects_duplicate_name() {
    let fx = AssetTestFixture::new("att_dup").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &create_args()).await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_ASSET_ALREADY_EXISTS
    );
}

#[tokio::test]
async fn test_interactive_create_walks_every_field() {
    let fx = AssetTestFixture::new("att_walk").unwrap();
    let input = ScriptedInput::new(
        &["sphere", "Linear"],
        &[
            "50", // max distance
            "5", "10", // inner / outer radius
            "0", "1", "1", "0",   // gain curve part 1
            "0.5", // max attenuation factor
            "0.1", "0.2", "0.3", // coefficients
        ],
    )
    .with_confirms(&[
        false, // add another gain curve part
        false, // accept defaults for remaining optional fields
        false, // add a spread curve
        true,  // configure air absorption
        true,  // enabled
        false, // use SDK default coefficients
    ]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["create", "walk"], &input, &output)
        .await
        .unwrap();

    for label in [
        "max distance",
        "zone shape",
        "part 1: start x",
        "max attenuation factor",
        "spread curve",
        "air absorption",
    ] {
        assert!(input.saw_prompt(label), "no prompt mentioning '{label}'");
    }
    input.assert_all_consumed();
    let att = read_att(&fx, "walk");
    assert_eq!(att.max_distance, 50.0);
    assert_eq!(att.shape.as_ref().unwrap().max_attenuation_factor, 0.5);
    assert_eq!(
        att.air_absorption.unwrap().coefficients.unwrap(),
        vec![0.1, 0.2, 0.3]
    );
}

#[tokio::test]
async fn test_interactive_accept_defaults_skips_optional_prompts() {
    let fx = AssetTestFixture::new("att_defaults").unwrap();
    let input = ScriptedInput::new(
        &["sphere", "Linear"],
        &["50", "5", "10", "0", "1", "1", "0"],
    )
    .with_confirms(&[false, true]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["create", "quick"], &input, &output)
        .await
        .unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("air absorption"));
    assert!(read_att(&fx, "quick").air_absorption.is_none());
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_json_returns_created_attenuation() {
    let fx = AssetTestFixture::new("att_list").unwrap();
    run(&fx, &create_args()).await.0.unwrap();

    let output = CaptureOutput::json();
    run_with(&fx, &["list"], &NonInteractiveInput, &output)
        .await
        .unwrap();
    let data = output.last_success().unwrap();
    assert_eq!(data["count"], 1);
    let first = &data["attenuations"][0];
    assert_eq!(first["name"], "short");
    assert_eq!(first["max_distance"], 50.0);
    assert_eq!(first["shape"], "sphere");
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_changes_only_provided_fields() {
    let fx = AssetTestFixture::new("att_update").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let before = read_att(&fx, "short");

    let (result, _out) = run(&fx, &["update", "short", "--max-distance", "80"]).await;
    result.unwrap();
    let after = read_att(&fx, "short");
    assert_eq!(after.max_distance, 80.0);
    assert_eq!(after.id, before.id);
    assert_eq!(after.shape, before.shape);
    assert_eq!(after.gain_curve, before.gain_curve);
}

#[tokio::test]
async fn test_update_single_zone_dimension_keeps_the_rest() {
    let fx = AssetTestFixture::new("att_update_zone").unwrap();
    run(&fx, &create_args()).await.0.unwrap();

    let (result, _out) = run(&fx, &["update", "short", "--outer-radius", "20"]).await;
    result.unwrap();
    let zone = read_att(&fx, "short").shape.unwrap().zone.unwrap();
    let value = serde_json::to_value(am::assets::generated::AttenuationShapeDefinition {
        max_attenuation_factor: 1.0,
        zone: Some(zone),
    })
    .unwrap();
    assert_eq!(value["zone"]["inner"]["radius"], 5.0);
    assert_eq!(value["zone"]["outer"]["radius"], 20.0);
}

#[tokio::test]
async fn test_update_validates_the_new_state() {
    let fx = AssetTestFixture::new("att_update_bad").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &["update", "short", "--max-attenuation-factor", "2"]).await;
    assert!(result.is_err());
    assert_eq!(
        read_att(&fx, "short").shape.unwrap().max_attenuation_factor,
        1.0
    );
}

#[tokio::test]
async fn test_update_without_changes_errors_non_interactively() {
    let fx = AssetTestFixture::new("att_update_none").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &["update", "short"]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_unknown_attenuation_is_not_found() {
    let fx = AssetTestFixture::new("att_update_missing").unwrap();
    let (result, _out) = run(&fx, &["update", "nope", "--max-distance", "1"]).await;
    let err = result.unwrap_err();
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_ASSET_NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_refuses_when_a_sound_references_it() {
    let fx = AssetTestFixture::new("att_delete_used").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let id = read_att(&fx, "short").id;
    fx.write_asset_json(
        "sounds",
        "boom",
        json!({"id": 9001, "name": "boom", "bus": 2, "attenuation": id}),
    )
    .unwrap();

    let (result, _out) = run(&fx, &["delete", "short", "--force"]).await;
    let err = result.unwrap_err();
    let cli = as_cli_error(&err).unwrap();
    assert_eq!(cli.code, codes::ERR_ASSET_IN_USE);
    assert!(format!("{err:#}").contains("boom"), "{err:#}");
    assert!(att_path(&fx, "short").exists());
}

#[tokio::test]
async fn test_delete_unreferenced_works() {
    let fx = AssetTestFixture::new("att_delete_free").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    fx.write_asset_json(
        "sounds",
        "boom",
        json!({"id": 9001, "name": "boom", "bus": 2, "attenuation": 0}),
    )
    .unwrap();

    let (result, _out) = run(&fx, &["delete", "short", "--force"]).await;
    result.unwrap();
    assert!(!att_path(&fx, "short").exists());
}

#[tokio::test]
async fn test_delete_without_force_needs_confirmation_non_interactively() {
    let fx = AssetTestFixture::new("att_delete_noforce").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &["delete", "short"]).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--force"));
    assert!(att_path(&fx, "short").exists());
}

#[tokio::test]
async fn test_delete_declined_confirmation_keeps_the_file() {
    let fx = AssetTestFixture::new("att_delete_decline").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let input = ScriptedInput::new(&[], &[]).with_confirms(&[false]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["delete", "short"], &input, &output)
        .await
        .unwrap();
    assert!(att_path(&fx, "short").exists());
}
