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

//! Tests for `am asset engine-config create|list|update|delete`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, EngineConfigCommands, handle_engine_config_command};
use am::common::errors::{as_cli_error, codes};
use am::input::{Input, NonInteractiveInput};
use clap::{Command, CommandFactory, Parser};
use common::fixtures::{AssetTestFixture, CaptureOutput};
use common::flatc::assert_flatc_accepts_schema;
use common::scripted_input::ScriptedInput;
use serde_json::{Value, json};

const TEMPLATE: &str = include_str!("../resources/default.config.json");

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

fn parse(args: &[&str]) -> EngineConfigCommands {
    let mut full = vec!["am", "asset", "engine-config"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::EngineConfig { command },
        } => command,
        _ => panic!("expected an engine-config command"),
    }
}

async fn run_with(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
    output: &CaptureOutput,
) -> anyhow::Result<()> {
    let _cwd = CwdGuard::enter(fx.project_root());
    handle_engine_config_command(&parse(args), None, input, output).await
}

async fn run(fx: &AssetTestFixture, args: &[&str]) -> (anyhow::Result<()>, CaptureOutput) {
    let output = CaptureOutput::interactive();
    let result = run_with(fx, args, &NonInteractiveInput, &output).await;
    (result, output)
}

/// A project with the `pc` pipeline asset and the `main` bus file.
fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.write_asset_json(
        "pipelines",
        "pc",
        json!({"id": 5, "name": "pc", "nodes": []}),
    )
    .unwrap();
    fx
}

fn config_path(fx: &AssetTestFixture, name: &str) -> PathBuf {
    fx.project_root().join(format!("{name}.config.json"))
}

fn read_config(fx: &AssetTestFixture, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(config_path(fx, name)).unwrap()).unwrap()
}

fn write_config(fx: &AssetTestFixture, name: &str, value: &Value) {
    std::fs::write(
        config_path(fx, name),
        serde_json::to_string_pretty(value).unwrap(),
    )
    .unwrap();
}

fn template() -> Value {
    serde_json::from_str(TEMPLATE).unwrap()
}

fn code_of(err: &anyhow::Error) -> i32 {
    as_cli_error(err).unwrap().code
}

const MINIMAL: [&str; 6] = [
    "--mixer-pipeline",
    "pc",
    "--mixer-active-channels",
    "32",
    "--buses-file",
    "main",
];

fn minimal(file: &str) -> Vec<&str> {
    let mut args = vec!["create", file];
    args.extend_from_slice(&MINIMAL);
    args
}

// ---------------------------------------------------------------------------
// Clap surface
// ---------------------------------------------------------------------------

#[test]
fn test_subcommands_parse_with_scheme_flags() {
    let EngineConfigCommands::Create { file_name, args } = parse(&[
        "create",
        "pc",
        "--output-frequency",
        "44100",
        "--game-track-rooms",
        "true",
        "--hrtf-hrir-sampling",
        "NearestNeighbor",
        "--from-json",
        "base.json",
    ]) else {
        panic!("expected create");
    };
    assert_eq!(file_name, "pc");
    assert_eq!(args.output_frequency.as_deref(), Some("44100"));
    assert_eq!(args.game_track_rooms.as_deref(), Some("true"));
    assert!(args.from_json.is_some());
    assert!(matches!(parse(&["list"]), EngineConfigCommands::List {}));
    assert!(matches!(
        parse(&["delete", "pc", "--force"]),
        EngineConfigCommands::Delete { force: true, .. }
    ));
    assert!(matches!(
        parse(&["update", "pc", "--driver", "null"]),
        EngineConfigCommands::Update { .. }
    ));
}

#[test]
fn test_every_modelled_field_has_a_flag() {
    let root = App::command();
    let asset = root.find_subcommand("asset").unwrap();
    let ec: &Command = asset.find_subcommand("engine-config").unwrap();
    let create = ec.find_subcommand("create").unwrap();
    for spec in am::assets::engine_config_fields::FIELDS {
        let long = spec.flag().trim_start_matches("--").to_string();
        assert!(
            create
                .get_arguments()
                .any(|a| a.get_long() == Some(long.as_str())),
            "no flag for {}",
            spec.key()
        );
    }
}

// ---------------------------------------------------------------------------
// create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_non_interactive_writes_a_config_flatc_accepts() {
    let fx = fixture("ec_create");
    let (result, _out) = run(&fx, &minimal("pc")).await;
    result.unwrap();

    let config = read_config(&fx, "pc");
    assert_eq!(config["mixer"]["pipeline"], json!("pc.ampipeline"));
    assert_eq!(config["buses_file"], json!("main.buses.ambus"));
    assert_eq!(config["mixer"]["active_channels"], json!(32));
    assert_eq!(config["output"]["frequency"], json!(48000));
    assert_eq!(config["game"]["listeners"], json!(1));
    assert_flatc_accepts_schema("engine_config_definition.fbs", &config_path(&fx, "pc"));
}

#[tokio::test]
async fn test_create_with_flags_sets_every_section() {
    let fx = fixture("ec_flags");
    let mut args = minimal("pc");
    args.extend([
        "--output-frequency",
        "44100",
        "--output-channels",
        "Surround_5_1",
        "--mixer-panning-mode",
        "BinauralLowQuality",
        "--hrtf-amir-file",
        "sphere.amir",
        "--game-doppler-factor",
        "0.5",
        "--game-track-rooms",
        "true",
        "--driver",
        "miniaudio",
    ]);
    let (result, _out) = run(&fx, &args).await;
    result.unwrap();
    let config = read_config(&fx, "pc");
    assert_eq!(config["output"]["frequency"], json!(44100));
    assert_eq!(config["output"]["channels"], json!("Surround_5_1"));
    assert_eq!(config["hrtf"]["amir_file"], json!("sphere.amir"));
    assert_eq!(config["hrtf"]["hrir_sampling"], json!("Bilinear"));
    assert_eq!(config["game"]["doppler_factor"], json!(0.5));
    assert_eq!(config["game"]["track_rooms"], json!(true));
    assert_eq!(config["driver"], json!("miniaudio"));
    assert_flatc_accepts_schema("engine_config_definition.fbs", &config_path(&fx, "pc"));
}

#[tokio::test]
async fn test_create_missing_required_flag_names_it_and_writes_nothing() {
    for json_mode in [false, true] {
        let fx = fixture("ec_missing");
        let output = if json_mode {
            CaptureOutput::json()
        } else {
            CaptureOutput::interactive()
        };
        let err = run_with(
            &fx,
            &["create", "pc", "--buses-file", "main"],
            &NonInteractiveInput,
            &output,
        )
        .await
        .unwrap_err();
        let text = format!("{err:#} {}", as_cli_error(&err).unwrap().suggestion);
        assert!(text.contains("--mixer-pipeline"), "{text}");
        assert!(text.contains("--mixer-active-channels"), "{text}");
        assert!(!config_path(&fx, "pc").exists());
    }
}

#[tokio::test]
async fn test_create_unknown_pipeline_or_bus_file_is_a_reference_error() {
    let fx = fixture("ec_refs");
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "pc",
            "--mixer-pipeline",
            "nope",
            "--mixer-active-channels",
            "8",
            "--buses-file",
            "main",
        ],
    )
    .await;
    assert_eq!(
        code_of(&result.unwrap_err()),
        codes::ERR_VALIDATION_REFERENCE
    );
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "pc",
            "--mixer-pipeline",
            "pc",
            "--mixer-active-channels",
            "8",
            "--buses-file",
            "ghost",
        ],
    )
    .await;
    assert_eq!(
        code_of(&result.unwrap_err()),
        codes::ERR_VALIDATION_REFERENCE
    );
    assert!(!config_path(&fx, "pc").exists());
}

#[tokio::test]
async fn test_create_existing_file_is_refused() {
    let fx = fixture("ec_exists");
    run(&fx, &minimal("pc")).await.0.unwrap();
    let (result, _out) = run(&fx, &minimal("pc")).await;
    assert_eq!(
        code_of(&result.unwrap_err()),
        codes::ERR_ASSET_ALREADY_EXISTS
    );
}

#[tokio::test]
async fn test_create_from_template_json_with_flag_override() {
    let fx = fixture("ec_from_json");
    fx.write_buses_file(
        "pc.buses.json",
        json!([{"id": 1, "name": "master", "fader": "Linear"}]),
    )
    .unwrap();
    let base = fx.project_root().join("base.json");
    std::fs::write(&base, TEMPLATE).unwrap();

    let (result, _out) = run(
        &fx,
        &[
            "create",
            "from_base",
            "--from-json",
            base.to_str().unwrap(),
            "--output-frequency",
            "22050",
        ],
    )
    .await;
    result.unwrap();
    let config = read_config(&fx, "from_base");
    assert_eq!(config["output"]["frequency"], json!(22050));
    assert_eq!(config["output"]["buffer_size"], json!(4096));
    assert_eq!(config["driver"], json!("miniaudio"));
    assert_eq!(
        config["game"]["obstruction"],
        template()["game"]["obstruction"]
    );
    assert_flatc_accepts_schema(
        "engine_config_definition.fbs",
        &config_path(&fx, "from_base"),
    );
}

#[tokio::test]
async fn test_create_interactive_picks_references_and_accepts_defaults() {
    let fx = fixture("ec_interactive");
    let input = ScriptedInput::new(&["pc", "main"], &["16"]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["create", "pc"], &input, &output)
        .await
        .unwrap();
    input.assert_all_consumed();
    let config = read_config(&fx, "pc");
    assert_eq!(config["mixer"]["pipeline"], json!("pc.ampipeline"));
    assert_eq!(config["buses_file"], json!("main.buses.ambus"));
    assert_eq!(config["mixer"]["active_channels"], json!(16));
    assert!(input.saw_prompt("Accept defaults"));
    assert!(config.get("hrtf").is_none());
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_json_shows_key_fields_and_accepts_the_template_unchanged() {
    let fx = fixture("ec_list");
    write_config(&fx, "pc", &template());
    let output = CaptureOutput::json();
    run_with(&fx, &["list"], &NonInteractiveInput, &output)
        .await
        .unwrap();
    let data = output.last_success().unwrap();
    assert_eq!(data["count"], json!(1));
    let row = &data["files"][0];
    assert_eq!(row["file"], json!("pc"));
    assert_eq!(row["pipeline"], json!("pc.ampipeline"));
    assert_eq!(row["buses_file"], json!("pc.buses.ambus"));
    assert_eq!(row["driver"], json!("miniaudio"));
    assert_eq!(row["frequency"], json!(48000));
    assert_eq!(row["active_channels"], json!(50));
    assert!(data["warnings"].as_array().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_changes_only_given_fields_and_keeps_unknown_keys_and_order() {
    let fx = fixture("ec_update");
    let mut existing = template();
    existing["custom"] = json!({"keep": true});
    existing["mixer"]["pipeline"] = json!("pc.ampipeline");
    existing["buses_file"] = json!("main.buses.ambus");
    write_config(&fx, "pc", &existing);

    let output = CaptureOutput::json();
    run_with(
        &fx,
        &["update", "pc", "--output-buffer-size", "512"],
        &NonInteractiveInput,
        &output,
    )
    .await
    .unwrap();

    let updated = read_config(&fx, "pc");
    let mut expected = existing.clone();
    expected["output"]["buffer_size"] = json!(512);
    assert_eq!(updated, expected);
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&updated), keys(&existing));
    assert_eq!(
        output.last_success().unwrap()["updated_fields"],
        json!(["output.buffer_size"])
    );
}

#[tokio::test]
async fn test_update_invalid_result_is_rejected_and_file_untouched() {
    let fx = fixture("ec_update_bad");
    let mut existing = template();
    existing["mixer"]["pipeline"] = json!("pc.ampipeline");
    existing["buses_file"] = json!("main.buses.ambus");
    write_config(&fx, "pc", &existing);
    let before = std::fs::read_to_string(config_path(&fx, "pc")).unwrap();
    let (result, _out) = run(
        &fx,
        &["update", "pc", "--mixer-panning-mode", "BinauralLowQuality"],
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--hrtf-amir-file"));
    assert_eq!(
        std::fs::read_to_string(config_path(&fx, "pc")).unwrap(),
        before
    );
}

#[tokio::test]
async fn test_update_from_json_replaces_sections_then_flags_apply() {
    let fx = fixture("ec_update_json");
    let mut existing = template();
    existing["mixer"]["pipeline"] = json!("pc.ampipeline");
    existing["buses_file"] = json!("main.buses.ambus");
    write_config(&fx, "pc", &existing);
    let curves = fx.project_root().join("game.json");
    let mut game = template()["game"].clone();
    game["obstruction"]["gain_curve"]["parts"][0]["end"]["y"] = json!(0.5);
    std::fs::write(&curves, json!({"game": game}).to_string()).unwrap();

    let (result, _out) = run(
        &fx,
        &[
            "update",
            "pc",
            "--from-json",
            curves.to_str().unwrap(),
            "--game-listeners",
            "8",
        ],
    )
    .await;
    result.unwrap();
    let updated = read_config(&fx, "pc");
    assert_eq!(
        updated["game"]["obstruction"]["gain_curve"]["parts"][0]["end"]["y"],
        json!(0.5)
    );
    assert_eq!(updated["game"]["listeners"], json!(8));
    assert_eq!(updated["output"], existing["output"]);
}

#[tokio::test]
async fn test_update_interactive_changes_only_what_the_user_edits() {
    let fx = fixture("ec_update_interactive");
    let mut existing = template();
    existing["mixer"]["pipeline"] = json!("pc.ampipeline");
    existing["buses_file"] = json!("main.buses.ambus");
    write_config(&fx, "pc", &existing);

    // Edit the output section: keep every field except the buffer size, then finish.
    let input = ScriptedInput::new(&["output", "Float32", "Stereo", "Done"], &["48000", "256"]);
    let output = CaptureOutput::interactive();
    run_with(&fx, &["update", "pc"], &input, &output)
        .await
        .unwrap();
    input.assert_all_consumed();
    let updated = read_config(&fx, "pc");
    let mut expected = existing.clone();
    expected["output"]["buffer_size"] = json!(256);
    assert_eq!(updated, expected);
}

#[tokio::test]
async fn test_update_unknown_pipeline_is_a_reference_error() {
    let fx = fixture("ec_update_ref");
    let mut existing = template();
    existing["mixer"]["pipeline"] = json!("pc.ampipeline");
    existing["buses_file"] = json!("main.buses.ambus");
    write_config(&fx, "pc", &existing);
    let (result, _out) = run(&fx, &["update", "pc", "--mixer-pipeline", "ghost"]).await;
    assert_eq!(
        code_of(&result.unwrap_err()),
        codes::ERR_VALIDATION_REFERENCE
    );
    assert_eq!(read_config(&fx, "pc"), existing);
}

#[tokio::test]
async fn test_update_without_flags_non_interactive_errors() {
    let fx = fixture("ec_update_none");
    write_config(&fx, "pc", &template());
    let (result, _out) = run(&fx, &["update", "pc"]).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_VALIDATION_FIELD);
}

// ---------------------------------------------------------------------------
// delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_last_config_is_refused_and_second_can_be_deleted() {
    let fx = fixture("ec_delete");
    run(&fx, &minimal("one")).await.0.unwrap();
    let (result, _out) = run(&fx, &["delete", "one", "--force"]).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("last"));
    assert!(config_path(&fx, "one").exists());

    run(&fx, &minimal("two")).await.0.unwrap();
    run(&fx, &["delete", "two", "--force"]).await.0.unwrap();
    assert!(!config_path(&fx, "two").exists());
    assert!(config_path(&fx, "one").exists());
}

#[tokio::test]
async fn test_delete_unknown_and_unconfirmed_non_interactive() {
    let fx = fixture("ec_delete_misc");
    run(&fx, &minimal("one")).await.0.unwrap();
    run(&fx, &minimal("two")).await.0.unwrap();
    let (result, _out) = run(&fx, &["delete", "ghost", "--force"]).await;
    assert_eq!(code_of(&result.unwrap_err()), codes::ERR_ASSET_NOT_FOUND);
    let (result, _out) = run(&fx, &["delete", "two"]).await;
    assert!(result.is_err());
    assert!(config_path(&fx, "two").exists());
}

// ---------------------------------------------------------------------------
// carry-over: pipeline referrers, active bus file, sound warning
// ---------------------------------------------------------------------------

mod carry_over {
    use super::*;
    use am::assets::ProjectValidator;
    use am::commands::asset::{
        handle_bus_command, handle_bus_file_command, handle_pipeline_command, handle_sound_command,
    };
    use am::forms::references::RefTarget;

    fn parse_asset(args: &[&str]) -> AssetCommands {
        let mut full = vec!["am", "asset"];
        full.extend_from_slice(args);
        match App::try_parse_from(full).expect("should parse").command {
            Commands::Asset { command } => command,
            _ => panic!("expected an asset command"),
        }
    }

    async fn run_asset(
        fx: &AssetTestFixture,
        args: &[&str],
        output: &CaptureOutput,
    ) -> anyhow::Result<()> {
        let _cwd = CwdGuard::enter(fx.project_root());
        match parse_asset(args) {
            AssetCommands::Pipeline { command } => {
                handle_pipeline_command(&command, None, &NonInteractiveInput, output).await
            }
            AssetCommands::BusFile { command } => {
                handle_bus_file_command(&command, None, &NonInteractiveInput, output).await
            }
            AssetCommands::Bus { command } => {
                handle_bus_command(&command, None, &NonInteractiveInput, output).await
            }
            AssetCommands::Sound { command } => {
                handle_sound_command(&command, None, &NonInteractiveInput, output).await
            }
            _ => panic!("unsupported"),
        }
    }

    #[tokio::test]
    async fn test_pipeline_delete_refused_when_a_config_references_it_even_with_force() {
        let fx = fixture("ec_pipeline_in_use");
        run(&fx, &minimal("pc")).await.0.unwrap();
        let output = CaptureOutput::interactive();
        let err = run_asset(&fx, &["pipeline", "delete", "pc", "--force"], &output)
            .await
            .unwrap_err();
        assert_eq!(code_of(&err), codes::ERR_ASSET_IN_USE);
        assert!(
            format!("{err:#} {}", as_cli_error(&err).unwrap().suggestion)
                .contains("pc.config.json")
        );
        assert!(fx.project_root().join("sources/pipelines/pc.json").exists());
    }

    #[tokio::test]
    async fn test_pipeline_delete_works_when_no_config_references_it() {
        let fx = fixture("ec_pipeline_free");
        let output = CaptureOutput::interactive();
        run_asset(&fx, &["pipeline", "delete", "pc", "--force"], &output)
            .await
            .unwrap();
        assert!(!fx.project_root().join("sources/pipelines/pc.json").exists());
    }

    #[tokio::test]
    async fn test_bus_file_delete_refused_when_the_active_config_points_at_it() {
        let fx = fixture("ec_busfile_active");
        fx.write_buses_file(
            "extra.buses.json",
            json!([{"id": 1, "name": "master", "fader": "Linear"}]),
        )
        .unwrap();
        run(&fx, &minimal("pc")).await.0.unwrap();
        let output = CaptureOutput::interactive();
        let err = run_asset(&fx, &["bus-file", "delete", "main", "--force"], &output)
            .await
            .unwrap_err();
        assert_eq!(code_of(&err), codes::ERR_ASSET_IN_USE);
        assert!(
            format!("{err:#} {}", as_cli_error(&err).unwrap().suggestion)
                .contains("pc.config.json")
        );
        assert!(fx.project_root().join("main.buses.json").exists());

        run_asset(&fx, &["bus-file", "delete", "extra", "--force"], &output)
            .await
            .unwrap();
    }

    fn two_bus_files(fx: &AssetTestFixture) {
        fx.write_buses_file(
            "other.buses.json",
            json!([
                {"id": 1, "name": "master", "child_buses": [77], "fader": "Linear"},
                {"id": 77, "name": "cutscene", "fader": "Linear"}
            ]),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn test_pickers_and_resolve_only_offer_the_active_bus_file() {
        let fx = fixture("ec_active_scope");
        two_bus_files(&fx);
        let validator = fx.create_project_validator().unwrap();
        let names = |v: &ProjectValidator| {
            v.list(RefTarget::Bus)
                .into_iter()
                .map(|b| b.name)
                .collect::<Vec<_>>()
        };
        assert!(
            names(&validator).contains(&"cutscene".to_string()),
            "no config: all files"
        );

        run(&fx, &minimal("pc")).await.0.unwrap();
        let validator = fx.create_project_validator().unwrap();
        let listed = names(&validator);
        assert!(listed.contains(&"sfx".to_string()));
        assert!(!listed.contains(&"cutscene".to_string()));
        assert!(validator.resolve(RefTarget::Bus, 77).is_none());
        assert!(validator.resolve(RefTarget::Bus, 2).is_some());

        run(
            &fx,
            &[
                "create",
                "alt",
                "--mixer-pipeline",
                "pc",
                "--mixer-active-channels",
                "4",
                "--buses-file",
                "other",
            ],
        )
        .await
        .0
        .unwrap();
        let validator = fx.create_project_validator().unwrap();
        assert!(
            names(&validator).contains(&"cutscene".to_string()),
            "configs disagree: all files"
        );
    }

    #[tokio::test]
    async fn test_sound_bus_outside_the_active_file_warns_instead_of_failing() {
        let fx = fixture("ec_sound_warn");
        two_bus_files(&fx);
        fx.create_data_file("a.wav").unwrap();
        run(&fx, &minimal("pc")).await.0.unwrap();

        let output = CaptureOutput::json();
        run_asset(
            &fx,
            &[
                "sound", "create", "boom", "--file", "a.wav", "--bus", "cutscene",
            ],
            &output,
        )
        .await
        .unwrap();
        assert!(fx.project_root().join("sources/sounds/boom.json").exists());
        let warnings = output.all_warnings();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("cutscene") && w.contains("main")),
            "{warnings:?}"
        );
        let data = output.last_success().unwrap();
        assert!(
            data["warnings"].as_array().is_some_and(|w| !w.is_empty()),
            "{data}"
        );

        let output = CaptureOutput::json();
        run_asset(
            &fx,
            &["sound", "create", "fine", "--file", "a.wav", "--bus", "sfx"],
            &output,
        )
        .await
        .unwrap();
        assert!(output.all_warnings().is_empty());
    }

    #[tokio::test]
    async fn test_same_bus_id_in_two_files_is_reported_by_bus_list() {
        let fx = fixture("ec_id_collision");
        fx.write_buses_file(
            "other.buses.json",
            json!([
                {"id": 1, "name": "master", "child_buses": [2], "fader": "Linear"},
                {"id": 2, "name": "weather", "fader": "Linear"}
            ]),
        )
        .unwrap();
        let output = CaptureOutput::json();
        run_asset(&fx, &["bus", "list"], &output).await.unwrap();
        let data = output.last_success().unwrap();
        let warnings = data["warnings"].as_array().unwrap();
        assert!(
            warnings.iter().any(|w| {
                let w = w.as_str().unwrap();
                w.contains("id 2") && w.contains("sfx") && w.contains("weather")
            }),
            "{warnings:?}"
        );
    }
}
