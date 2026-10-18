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

//! Tests for `effect create` / `effect update` with the full effect field surface.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::commands::asset::EffectCommands;
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use common::effect_helpers::*;
use common::fixtures::{AssetTestFixture, CaptureOutput};
use common::scripted_input::ScriptedInput;
use serde_json::json;

// ---------------------------------------------------------------------------
// Flag parsing
// ---------------------------------------------------------------------------

#[test]
fn test_create_parses_every_parameter_flag() {
    match parse(&[
        "create",
        "fx",
        "--effect-type",
        "Delay",
        "--param",
        "0.5",
        "--param-rtpc",
        "1=speed",
        "--parameters-from-json",
        "p.json",
    ]) {
        EffectCommands::Create { extra, param, .. } => {
            assert_eq!(param, Some(vec![0.5]));
            assert_eq!(extra.param_rtpc, vec!["1=speed".to_string()]);
            assert_eq!(extra.parameters_from_json.unwrap().to_str(), Some("p.json"));
        }
        _ => panic!("expected create"),
    }
}

#[test]
fn test_update_parses_the_edit_flags() {
    match parse(&[
        "update",
        "fx",
        "--add-param",
        "0.2",
        "--add-param",
        "rtpc:wind",
        "--remove-param",
        "0",
        "--remove-param",
        "2",
        "--clear-params",
    ]) {
        EffectCommands::Update { edit, .. } => {
            assert_eq!(
                edit.add_param,
                vec!["0.2".to_string(), "rtpc:wind".to_string()]
            );
            assert_eq!(edit.remove_param, vec![0, 2]);
            assert!(edit.clear_params);
        }
        _ => panic!("expected update"),
    }
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_writes_static_and_bound_parameters_in_order() {
    let fx = fixture("fx_create_mixed");
    create_ok(
        &fx,
        &["--param", "0.5", "--param", "2", "--param-rtpc", "1=speed"],
    )
    .await;
    let raw = read_raw(&fx, "fx1");
    assert_eq!(raw["effect"], "Delay");
    let params = raw["parameters"].as_array().unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0], json!({"kind": "Static", "value": 0.5}));
    assert_eq!(params[1]["kind"], "RTPC");
    assert_eq!(params[1]["rtpc"]["id"], 401);
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_create_binds_parameters_by_rtpc_id_and_appends_at_the_end() {
    let fx = fixture("fx_create_bind_only");
    create_ok(&fx, &["--param-rtpc", "0=speed", "--param-rtpc", "1=402"]).await;
    let effect = read_effect(&fx, "fx1");
    let ids: Vec<u64> = effect
        .parameters
        .unwrap()
        .iter()
        .map(|p| p.rtpc.as_ref().unwrap().id)
        .collect();
    assert_eq!(ids, vec![401, 402]);
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_create_with_unregistered_filter_warns_but_succeeds() {
    let fx = fixture("fx_create_plugin");
    let (result, out) = run_with(
        &fx,
        &["create", "fx1", "--effect-type", "MyPluginReverb"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert_eq!(
        read_effect(&fx, "fx1").effect.as_deref(),
        Some("MyPluginReverb")
    );
    let warnings = out.all_warnings();
    assert!(
        warnings.iter().any(|w| w.contains("MyPluginReverb")),
        "{warnings:?}"
    );
    assert!(
        out.last_success().unwrap()["warnings"][0]
            .as_str()
            .unwrap()
            .contains("MyPluginReverb")
    );
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_create_with_registered_filter_does_not_warn() {
    let fx = fixture("fx_create_known");
    let (result, out) = run_with(
        &fx,
        &["create", "fx1", "--effect-type", "Equalizer"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    assert!(out.all_warnings().is_empty());
    assert!(out.last_success().unwrap().get("warnings").is_none());
}

#[tokio::test]
async fn test_create_missing_effect_type_names_the_flag_and_writes_nothing() {
    let fx = fixture("fx_no_type");
    let (result, _) = run(&fx, &["create", "fx1"], &NonInteractiveInput).await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("--effect-type"));
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_FIELD
    );
    assert!(!effect_path(&fx, "fx1").exists());
}

#[tokio::test]
async fn test_json_mode_never_prompts() {
    let fx = fixture("fx_json_no_prompt");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run_with(&fx, &["create", "fx1"], &input, CaptureOutput::json()).await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--effect-type"));
    assert!(input.prompts.lock().unwrap().is_empty());

    let (result, _) = run_with(
        &fx,
        &[
            "create",
            "fx1",
            "--effect-type",
            "Delay",
            "--param-rtpc",
            "0=nope",
        ],
        &input,
        CaptureOutput::json(),
    )
    .await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
    assert!(!effect_path(&fx, "fx1").exists());
}

#[tokio::test]
async fn test_create_unknown_rtpc_name_is_a_reference_error() {
    let fx = fixture("fx_unknown_rtpc");
    let (result, _) = run(
        &fx,
        &[
            "create",
            "fx1",
            "--effect-type",
            "Delay",
            "--param-rtpc",
            "0=nope",
        ],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    let suggestion = &as_cli_error(&err).unwrap().suggestion;
    assert!(
        suggestion.contains("speed"),
        "lists what exists: {suggestion}"
    );
    assert!(!effect_path(&fx, "fx1").exists());
}

#[tokio::test]
async fn test_create_numeric_rtpc_id_that_does_not_exist_is_a_reference_error() {
    let fx = fixture("fx_dangling_rtpc");
    let (result, _) = run(
        &fx,
        &[
            "create",
            "fx1",
            "--effect-type",
            "Delay",
            "--param-rtpc",
            "0=999",
        ],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert!(!effect_path(&fx, "fx1").exists());
}

#[tokio::test]
async fn test_create_bind_index_beyond_the_list_fails() {
    let fx = fixture("fx_bind_range");
    let (result, _) = run(
        &fx,
        &[
            "create",
            "fx1",
            "--effect-type",
            "Delay",
            "--param-rtpc",
            "2=speed",
        ],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_FIELD
    );
    assert!(!effect_path(&fx, "fx1").exists());
}

#[tokio::test]
async fn test_create_rejects_malformed_param_rtpc() {
    let fx = fixture("fx_bind_syntax");
    for bad in ["speed", "x=speed", "=speed", "0="] {
        let (result, _) = run(
            &fx,
            &[
                "create",
                "fx1",
                "--effect-type",
                "Delay",
                "--param-rtpc",
                bad,
            ],
            &NonInteractiveInput,
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(
            as_cli_error(&err).unwrap().code,
            codes::ERR_VALIDATION_FIELD,
            "{bad}"
        );
        assert!(format!("{err:#}").contains("INDEX=RTPC"), "{bad}: {err:#}");
    }
}

#[tokio::test]
async fn test_create_rejects_non_finite_param() {
    let fx = fixture("fx_nan");
    let (result, _) = run(
        &fx,
        &["create", "fx1", "--effect-type", "Delay", "--param", "NaN"],
        &NonInteractiveInput,
    )
    .await;
    assert!(result.is_err());
    assert!(!effect_path(&fx, "fx1").exists());
}

#[tokio::test]
async fn test_create_rejects_whitespace_effect_type() {
    let fx = fixture("fx_blank_type");
    let (result, _) = run(
        &fx,
        &["create", "fx1", "--effect-type", "  "],
        &NonInteractiveInput,
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_conflicting_replace_flags_are_rejected() {
    let fx = fixture("fx_conflict");
    let file = fx.project_root().join("p.json");
    std::fs::write(&file, "[0.5]").unwrap();
    let (result, _) = run(
        &fx,
        &[
            "create",
            "fx1",
            "--effect-type",
            "Delay",
            "--param",
            "1",
            "--parameters-from-json",
            file.to_str().unwrap(),
        ],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("--parameters-from-json"));
    assert!(!effect_path(&fx, "fx1").exists());
}

#[tokio::test]
async fn test_create_from_json_accepts_numbers_names_and_value_objects() {
    let fx = fixture("fx_create_json");
    let file = fx.project_root().join("p.json");
    std::fs::write(
        &file,
        serde_json::to_string(&json!([
            0.25,
            "rtpc:wind",
            {"kind": "RTPC", "rtpc": {"id": 401, "curve": {"parts": []}}}
        ]))
        .unwrap(),
    )
    .unwrap();
    create_ok(&fx, &["--parameters-from-json", file.to_str().unwrap()]).await;
    let raw = read_raw(&fx, "fx1");
    let params = raw["parameters"].as_array().unwrap();
    assert_eq!(params[0]["value"], 0.25);
    assert_eq!(params[1]["rtpc"]["id"], 402);
    assert_eq!(params[2]["rtpc"]["id"], 401);
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_create_from_sdk_effect_object_reads_its_parameters() {
    let fx = fixture("fx_create_json_obj");
    let file = fx.project_root().join("p.json");
    std::fs::write(
        &file,
        r#"{"id": 5, "name": "old", "effect": "Lofi", "parameters": [{"kind": "Static", "value": 3.0}]}"#,
    )
    .unwrap();
    create_ok(&fx, &["--parameters-from-json", file.to_str().unwrap()]).await;
    assert_eq!(read_effect(&fx, "fx1").effect.as_deref(), Some("Delay"));
    assert_eq!(
        read_effect(&fx, "fx1").parameters.unwrap()[0].as_static(),
        Some(3.0)
    );
}

#[tokio::test]
async fn test_create_from_json_with_dangling_rtpc_fails() {
    let fx = fixture("fx_create_json_dangling");
    let file = fx.project_root().join("p.json");
    std::fs::write(
        &file,
        r#"[{"kind": "RTPC", "rtpc": {"id": 999, "curve": {"parts": []}}}]"#,
    )
    .unwrap();
    let (result, _) = run(
        &fx,
        &[
            "create",
            "fx1",
            "--effect-type",
            "Delay",
            "--parameters-from-json",
            file.to_str().unwrap(),
        ],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert!(!effect_path(&fx, "fx1").exists());
}

#[tokio::test]
async fn test_create_from_json_rejects_unusable_files() {
    let fx = fixture("fx_create_json_bad");
    let file = fx.project_root().join("p.json");
    for (content, needle) in [
        ("not json", "not valid JSON"),
        ("{\"x\": 1}", "parameters"),
        ("[\"hello\"]", "hello"),
        ("[{\"nope\": 1}]", "kind"),
    ] {
        std::fs::write(&file, content).unwrap();
        let (result, _) = run(
            &fx,
            &[
                "create",
                "fx1",
                "--effect-type",
                "Delay",
                "--parameters-from-json",
                file.to_str().unwrap(),
            ],
            &NonInteractiveInput,
        )
        .await;
        let err = result.unwrap_err();
        assert_eq!(
            as_cli_error(&err).unwrap().code,
            codes::ERR_VALIDATION_FIELD,
            "{content}"
        );
        assert!(format!("{err:#}").contains(needle), "{content}: {err:#}");
    }
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

/// `fx1` with parameters [static 0.5, bound to speed, static 3].
async fn rich_effect(fx: &AssetTestFixture) {
    create_ok(
        fx,
        &[
            "--param",
            "0.5",
            "--param",
            "2",
            "--param",
            "3",
            "--param-rtpc",
            "1=speed",
        ],
    )
    .await;
}

#[tokio::test]
async fn test_update_effect_type_changes_only_that_field() {
    let fx = fixture("fx_up_type");
    rich_effect(&fx).await;
    let before = read_effect(&fx, "fx1");
    let (result, out) = run_with(
        &fx,
        &["update", "fx1", "--effect-type", "Lofi"],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();
    let mut expected = before.clone();
    expected.effect = Some("Lofi".to_string());
    assert_eq!(read_effect(&fx, "fx1"), expected);
    assert_eq!(
        out.last_success().unwrap()["updated_fields"],
        json!(["effect_type"])
    );
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_update_bind_add_and_remove_keep_the_other_parameters() {
    let fx = fixture("fx_up_edit");
    rich_effect(&fx).await;
    let before = read_effect(&fx, "fx1").parameters.unwrap();

    let (result, out) = run_with(
        &fx,
        &[
            "update",
            "fx1",
            "--param-rtpc",
            "0=wind",
            "--remove-param",
            "2",
            "--add-param",
            "rtpc:speed",
            "--add-param",
            "9.5",
        ],
        &NonInteractiveInput,
        CaptureOutput::json(),
    )
    .await;
    result.unwrap();

    let after = read_effect(&fx, "fx1").parameters.unwrap();
    assert_eq!(after.len(), 4);
    assert_eq!(after[0].rtpc.as_ref().unwrap().id, 402);
    assert_eq!(
        after[1], before[1],
        "the bound parameter is kept as written"
    );
    assert_eq!(after[2].rtpc.as_ref().unwrap().id, 401);
    assert_eq!(after[3].as_static(), Some(9.5));
    assert_eq!(
        out.last_success().unwrap()["updated_fields"],
        json!(["parameters"])
    );
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_update_param_replaces_and_clear_params_empties() {
    let fx = fixture("fx_up_replace");
    rich_effect(&fx).await;
    let (result, _) = run(
        &fx,
        &["update", "fx1", "--param", "7"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert_eq!(read_effect(&fx, "fx1").parameters.unwrap().len(), 1);

    let (result, _) = run(
        &fx,
        &["update", "fx1", "--clear-params"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert!(
        read_effect(&fx, "fx1")
            .parameters
            .unwrap_or_default()
            .is_empty()
    );
    assert_flatc(&fx, "fx1");
}

#[tokio::test]
async fn test_update_from_json_replaces_the_list() {
    let fx = fixture("fx_up_json");
    rich_effect(&fx).await;
    let file = fx.project_root().join("p.json");
    std::fs::write(&file, "[1.5, \"rtpc:wind\"]").unwrap();
    let (result, _) = run(
        &fx,
        &[
            "update",
            "fx1",
            "--parameters-from-json",
            file.to_str().unwrap(),
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let params = read_effect(&fx, "fx1").parameters.unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params[1].rtpc.as_ref().unwrap().id, 402);
}

#[tokio::test]
async fn test_update_remove_out_of_range_fails_and_writes_nothing() {
    let fx = fixture("fx_up_range");
    rich_effect(&fx).await;
    let before = std::fs::read_to_string(effect_path(&fx, "fx1")).unwrap();
    let (result, _) = run(
        &fx,
        &["update", "fx1", "--remove-param", "9"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_FIELD
    );
    assert_eq!(
        std::fs::read_to_string(effect_path(&fx, "fx1")).unwrap(),
        before
    );
}

#[tokio::test]
async fn test_update_unknown_rtpc_is_a_reference_error_and_writes_nothing() {
    let fx = fixture("fx_up_dangling");
    rich_effect(&fx).await;
    let before = std::fs::read_to_string(effect_path(&fx, "fx1")).unwrap();
    let (result, _) = run(
        &fx,
        &["update", "fx1", "--add-param", "rtpc:nope"],
        &NonInteractiveInput,
    )
    .await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert_eq!(
        std::fs::read_to_string(effect_path(&fx, "fx1")).unwrap(),
        before
    );
}

#[tokio::test]
async fn test_update_keeps_unknown_keys_and_their_position() {
    let fx = fixture("fx_up_unknown");
    fx.write_asset_json(
        "effects",
        "hand",
        json!({
            "_a": {"note": "kept"},
            "id": 900,
            "name": "hand",
            "effect": "Delay",
            "parameters": [{"kind": "Static", "value": 1.0, "_p": "verbatim"}],
            "_z": 3
        }),
    )
    .unwrap();
    let (result, _) = run(
        &fx,
        &["update", "hand", "--effect-type", "Lofi"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let raw = read_raw(&fx, "hand");
    let keys: Vec<&str> = raw
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let at = |k: &str| keys.iter().position(|x| *x == k).unwrap();
    assert!(
        at("parameters") < at("_a"),
        "unknown keys follow the modelled ones: {keys:?}"
    );
    assert!(
        at("_a") < at("_z"),
        "unknown keys keep their relative order: {keys:?}"
    );
    assert_eq!(raw["_a"], json!({"note": "kept"}));
    assert_eq!(raw["_z"], 3);
    assert_eq!(raw["effect"], "Lofi");
    assert_eq!(
        raw["parameters"][0]["_p"], "verbatim",
        "untouched parameters stay exactly as written"
    );
}

#[tokio::test]
async fn test_update_never_prompts_in_json_mode() {
    let fx = fixture("fx_up_json_no_prompt");
    rich_effect(&fx).await;
    let before = std::fs::read_to_string(effect_path(&fx, "fx1")).unwrap();
    let input = ScriptedInput::new(&[], &[]);
    let (result, _) = run_with(&fx, &["update", "fx1"], &input, CaptureOutput::json()).await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(effect_path(&fx, "fx1")).unwrap(),
        before
    );
}

#[tokio::test]
async fn test_update_of_a_file_without_effect_type_asks_for_the_flag_non_interactively() {
    let fx = fixture("fx_up_legacy");
    fx.write_asset_json("effects", "legacy", json!({"id": 901, "name": "legacy"}))
        .unwrap();
    let (result, _) = run(
        &fx,
        &["update", "legacy", "--param", "1"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert!(format!("{err:#}").contains("effect"), "{err:#}");

    let (result, _) = run(
        &fx,
        &["update", "legacy", "--effect-type", "Lofi", "--param", "1"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert_eq!(read_effect(&fx, "legacy").effect.as_deref(), Some("Lofi"));
}
