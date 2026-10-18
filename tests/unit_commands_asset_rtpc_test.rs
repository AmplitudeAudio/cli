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

//! Tests for `am asset rtpc create|list|update|delete`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use am::app::{App, Commands};
use am::assets::Rtpc;
use am::commands::asset::{AssetCommands, RtpcCommands, handle_rtpc_command};
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
    vec!["create", "speed", "--min-value", "0", "--max-value", "10"]
}

fn parse(args: &[&str]) -> RtpcCommands {
    let mut full = vec!["am", "asset", "rtpc"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Rtpc { command },
        } => command,
        _ => panic!("expected an rtpc command"),
    }
}

fn rtpc_path(fx: &AssetTestFixture, name: &str) -> PathBuf {
    fx.project_root()
        .join("sources/rtpc")
        .join(format!("{name}.json"))
}

fn read_rtpc(fx: &AssetTestFixture, name: &str) -> Rtpc {
    serde_json::from_str(&std::fs::read_to_string(rtpc_path(fx, name)).unwrap()).unwrap()
}

async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: &CaptureOutput,
) -> anyhow::Result<()> {
    let _cwd = CwdGuard::enter(fx.project_root());
    handle_rtpc_command(&parse(args), None, input, output).await
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
        "speed",
        "--min-value",
        "-5",
        "--max-value",
        "10",
        "--default-value",
        "2.5",
        "--fade-settings-enabled",
        "true",
        "--fade-attack-duration",
        "0.25",
        "--fade-attack-fader",
        "Ease",
        "--fade-release-duration",
        "0.5",
        "--fade-release-fader",
        "Linear",
    ]);
    let RtpcCommands::Create { name, args } = cmd else {
        panic!("expected create");
    };
    assert_eq!(name, "speed");
    assert_eq!(args.min_value, Some(-5.0));
    assert_eq!(args.max_value, Some(10.0));
    assert_eq!(args.default_value, Some(2.5));
    assert_eq!(args.fade_settings_enabled, Some(true));
    assert_eq!(args.fade_attack_duration, Some(0.25));
    assert_eq!(args.fade_attack_fader.as_deref(), Some("Ease"));
    assert_eq!(args.fade_release_duration, Some(0.5));
    assert_eq!(args.fade_release_fader.as_deref(), Some("Linear"));
}

#[test]
fn test_update_list_delete_parse() {
    assert!(matches!(
        parse(&["update", "speed", "--max-value", "9"]),
        RtpcCommands::Update { .. }
    ));
    assert!(matches!(parse(&["list"]), RtpcCommands::List {}));
    let RtpcCommands::Delete { name, force } = parse(&["delete", "speed", "--force"]) else {
        panic!("expected delete");
    };
    assert_eq!(name, "speed");
    assert!(force);
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_writes_a_file_that_round_trips() {
    let fx = AssetTestFixture::new("rtpc_create").unwrap();
    let (result, _out) = run(&fx, &create_args()).await;
    result.unwrap();

    let rtpc = read_rtpc(&fx, "speed");
    assert_ne!(rtpc.id, 0);
    assert_eq!(rtpc.min_value, 0.0);
    assert_eq!(rtpc.max_value, 10.0);
    assert_eq!(rtpc.default_value, 0.0);
    assert!(rtpc.fade_settings.is_none());
}

fn flatc() -> Option<PathBuf> {
    ["flatc", "/opt/homebrew/bin/flatc"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| Command::new(p).arg("--version").output().is_ok())
}

#[tokio::test]
async fn test_created_file_is_accepted_by_flatc() {
    let fx = AssetTestFixture::new("rtpc_flatc").unwrap();
    let mut args = create_args();
    args.extend(["--default-value", "3"]);
    args.extend([
        "--fade-attack-duration",
        "0.25",
        "--fade-attack-fader",
        "Ease",
    ]);
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
        .map(|d| d.join("rtpc_definition.fbs"))
        .find(|p| p.exists())
    else {
        eprintln!("skipped flatc check: rtpc_definition.fbs not found");
        return;
    };
    let out_dir = tempfile::tempdir().unwrap();
    let out = Command::new(flatc)
        .args(["-b", "--strict-json", "-o"])
        .arg(out_dir.path())
        .arg("-I")
        .arg(schema.parent().unwrap())
        .arg(&schema)
        .arg(rtpc_path(&fx, "speed"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "flatc rejected the file: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn test_fade_transition_flags_enable_fading_with_a_complete_block() {
    let fx = AssetTestFixture::new("rtpc_fade_flags").unwrap();
    let mut args = create_args();
    args.extend(["--fade-release-duration", "2"]);
    run(&fx, &args).await.0.unwrap();

    let fade = read_rtpc(&fx, "speed").fade_settings.unwrap();
    assert!(fade.enabled);
    assert_eq!(fade.fade_release.unwrap().duration, 2.0);
    assert!(fade.fade_attack.unwrap().fader.is_some());
}

#[tokio::test]
async fn test_min_above_max_is_rejected_with_a_field_error() {
    let fx = AssetTestFixture::new("rtpc_min_max").unwrap();
    let (result, _out) = run(
        &fx,
        &["create", "speed", "--min-value", "10", "--max-value", "1"],
    )
    .await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("min_value"), "{err:#}");
    assert!(!rtpc_path(&fx, "speed").exists());
}

#[tokio::test]
async fn test_default_outside_range_is_rejected() {
    let fx = AssetTestFixture::new("rtpc_default_out").unwrap();
    let mut args = create_args();
    args.extend(["--default-value", "11"]);
    let (result, _out) = run(&fx, &args).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("default_value"));
    assert!(!rtpc_path(&fx, "speed").exists());
}

#[tokio::test]
async fn test_unknown_fader_is_rejected_naming_the_flag() {
    let fx = AssetTestFixture::new("rtpc_bad_fader").unwrap();
    let mut args = create_args();
    args.extend(["--fade-attack-fader", "Bouncy"]);
    let (result, _out) = run(&fx, &args).await;
    let err = result.unwrap_err();
    assert!(
        format!("{err:#}").contains("--fade-attack-fader"),
        "{err:#}"
    );
    assert!(!rtpc_path(&fx, "speed").exists());
}

#[tokio::test]
async fn test_non_finite_numbers_are_rejected_naming_the_flag() {
    let fx = AssetTestFixture::new("rtpc_nan").unwrap();
    let (result, _out) = run(
        &fx,
        &["create", "speed", "--min-value", "0", "--max-value", "inf"],
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--max-value"));
    assert!(!rtpc_path(&fx, "speed").exists());
}

#[tokio::test]
async fn test_missing_required_values_name_the_flag() {
    let cases: [(&[&str], &str); 2] = [
        (&["create", "a", "--max-value", "10"], "--min-value"),
        (&["create", "a", "--min-value", "0"], "--max-value"),
    ];
    for (index, (args, flag)) in cases.into_iter().enumerate() {
        let fx = AssetTestFixture::new(&format!("rtpc_missing_{index}")).unwrap();
        let (result, _out) = run(&fx, args).await;
        let err = result.unwrap_err();
        assert!(format!("{err:#}").contains(flag), "{flag}: {err:#}");
        assert!(!rtpc_path(&fx, "a").exists());
    }
}

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = AssetTestFixture::new("rtpc_json_noprompt").unwrap();
    let input = ScriptedInput::new(&[], &[]);
    let output = CaptureOutput::json();
    let result = run_with(&fx, &["create", "a", "--min-value", "0"], &input, &output).await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
    assert!(!rtpc_path(&fx, "a").exists());
}

#[tokio::test]
async fn test_create_rejects_duplicate_name() {
    let fx = AssetTestFixture::new("rtpc_dup").unwrap();
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
    let fx = AssetTestFixture::new("rtpc_walk").unwrap();
    let input = ScriptedInput::new(
        &["Ease", "SCurveSharp"],
        &[
            "0", "10",   // min, max
            "5",    // default value
            "0.25", // attack duration
            "0.5",  // release duration
        ],
    )
    .with_confirms(&[
        false, // accept defaults for remaining optional fields
        true,  // configure fading
        true,  // fading enabled
    ]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["create", "walk"], &input, &output)
        .await
        .unwrap();

    for label in [
        "min value",
        "max value",
        "default value",
        "fade settings",
        "attack duration",
        "attack fader",
        "release duration",
        "release fader",
    ] {
        assert!(input.saw_prompt(label), "no prompt mentioning '{label}'");
    }
    input.assert_all_consumed();
    let rtpc = read_rtpc(&fx, "walk");
    assert_eq!(rtpc.default_value, 5.0);
    let fade = rtpc.fade_settings.unwrap();
    assert!(fade.enabled);
    let attack = fade.fade_attack.unwrap();
    assert_eq!(
        (attack.duration, attack.fader.as_deref()),
        (0.25, Some("Ease"))
    );
    let release = fade.fade_release.unwrap();
    assert_eq!(
        (release.duration, release.fader.as_deref()),
        (0.5, Some("SCurveSharp"))
    );
}

#[tokio::test]
async fn test_interactive_accept_defaults_skips_optional_prompts() {
    let fx = AssetTestFixture::new("rtpc_defaults").unwrap();
    let input = ScriptedInput::new(&[], &["0", "10"]).with_confirms(&[true]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["create", "quick"], &input, &output)
        .await
        .unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("fade"));
    assert!(read_rtpc(&fx, "quick").fade_settings.is_none());
}

// ---------------------------------------------------------------------------
// List
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_json_returns_created_rtpc() {
    let fx = AssetTestFixture::new("rtpc_list").unwrap();
    run(&fx, &create_args()).await.0.unwrap();

    let output = CaptureOutput::json();
    run_with(&fx, &["list"], &NonInteractiveInput, &output)
        .await
        .unwrap();
    let data = output.last_success().unwrap();
    assert_eq!(data["count"], 1);
    let first = &data["rtpcs"][0];
    assert_eq!(first["name"], "speed");
    assert_eq!(first["min_value"], 0.0);
    assert_eq!(first["max_value"], 10.0);
    assert_eq!(first["default_value"], 0.0);
    assert_eq!(first["fade_enabled"], false);
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_changes_only_provided_fields() {
    let fx = AssetTestFixture::new("rtpc_update").unwrap();
    let mut args = create_args();
    args.extend(["--fade-attack-duration", "1"]);
    run(&fx, &args).await.0.unwrap();
    let before = read_rtpc(&fx, "speed");

    let (result, _out) = run(&fx, &["update", "speed", "--max-value", "20"]).await;
    result.unwrap();
    let after = read_rtpc(&fx, "speed");
    assert_eq!(after.max_value, 20.0);
    assert_eq!(after.id, before.id);
    assert_eq!(after.min_value, before.min_value);
    assert_eq!(after.default_value, before.default_value);
    assert_eq!(after.fade_settings, before.fade_settings);
}

#[tokio::test]
async fn test_update_preserves_unknown_fields() {
    let fx = AssetTestFixture::new("rtpc_update_unknown").unwrap();
    fx.write_asset_json(
        "rtpc",
        "speed",
        json!({"id": 12, "name": "speed", "min_value": 0, "max_value": 10, "x_custom": {"a": 1}}),
    )
    .unwrap();
    run(&fx, &["update", "speed", "--max-value", "20"])
        .await
        .0
        .unwrap();
    let text = std::fs::read_to_string(rtpc_path(&fx, "speed")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["x_custom"], json!({"a": 1}));
    assert_eq!(value["max_value"], 20.0);
}

#[tokio::test]
async fn test_update_validates_the_new_state() {
    let fx = AssetTestFixture::new("rtpc_update_bad").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &["update", "speed", "--max-value", "-1"]).await;
    assert!(result.is_err());
    assert_eq!(read_rtpc(&fx, "speed").max_value, 10.0);
}

#[tokio::test]
async fn test_update_without_changes_errors_non_interactively() {
    let fx = AssetTestFixture::new("rtpc_update_none").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &["update", "speed"]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_unknown_rtpc_is_not_found() {
    let fx = AssetTestFixture::new("rtpc_update_missing").unwrap();
    let (result, _out) = run(&fx, &["update", "nope", "--max-value", "1"]).await;
    let err = result.unwrap_err();
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_ASSET_NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

fn bound_sound(rtpc_id: u64) -> serde_json::Value {
    json!({
        "id": 9001,
        "name": "boom",
        "bus": 2,
        "gain": {"kind": "RTPC", "rtpc": {"id": rtpc_id}}
    })
}

#[tokio::test]
async fn test_delete_refuses_when_a_sound_binds_it() {
    let fx = AssetTestFixture::new("rtpc_delete_used").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let id = read_rtpc(&fx, "speed").id;
    fx.write_asset_json("sounds", "boom", bound_sound(id))
        .unwrap();

    let (result, _out) = run(&fx, &["delete", "speed", "--force"]).await;
    let err = result.unwrap_err();
    assert_eq!(as_cli_error(&err).unwrap().code, codes::ERR_ASSET_IN_USE);
    assert!(format!("{err:#}").contains("boom"), "{err:#}");
    assert!(rtpc_path(&fx, "speed").exists());
}

#[tokio::test]
async fn test_delete_unreferenced_works() {
    let fx = AssetTestFixture::new("rtpc_delete_free").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    fx.write_asset_json(
        "sounds",
        "boom",
        json!({"id": 9001, "name": "boom", "bus": 2, "gain": {"kind": "Static", "value": 1.0}}),
    )
    .unwrap();

    let (result, _out) = run(&fx, &["delete", "speed", "--force"]).await;
    result.unwrap();
    assert!(!rtpc_path(&fx, "speed").exists());
}

#[tokio::test]
async fn test_delete_without_force_needs_confirmation_non_interactively() {
    let fx = AssetTestFixture::new("rtpc_delete_noforce").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let (result, _out) = run(&fx, &["delete", "speed"]).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--force"));
    assert!(rtpc_path(&fx, "speed").exists());
}

#[tokio::test]
async fn test_delete_declined_confirmation_keeps_the_file() {
    let fx = AssetTestFixture::new("rtpc_delete_decline").unwrap();
    run(&fx, &create_args()).await.0.unwrap();
    let input = ScriptedInput::new(&[], &[]).with_confirms(&[false]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["delete", "speed"], &input, &output)
        .await
        .unwrap();
    assert!(rtpc_path(&fx, "speed").exists());
}
