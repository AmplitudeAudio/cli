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

//! Tests for engine config validation and the create/update service.

use am::assets::engine_config::validate_engine_config;
use am::assets::engine_config_fields::{FIELDS, parse_value};
use am::common::errors::codes;
use am::services::engine_config::{apply_update, build_config, check_references};
use serde_json::{Value, json};

const TEMPLATE: &str = include_str!("../resources/default.config.json");

fn template() -> Value {
    serde_json::from_str(TEMPLATE).unwrap()
}

fn minimal_overrides() -> Vec<(&'static str, Value)> {
    vec![
        ("mixer.pipeline", json!("pc.ampipeline")),
        ("mixer.active_channels", json!(32)),
        ("buses_file", json!("main.buses.ambus")),
    ]
}

fn invalid(config: &Value) -> String {
    let err = validate_engine_config(config).unwrap_err();
    format!("{err:?} {err}")
}

#[test]
fn test_default_template_validates_unchanged() {
    validate_engine_config(&template()).unwrap();
}

#[test]
fn test_missing_required_tables_and_strings_are_rejected() {
    for key in ["output", "mixer", "game", "buses_file"] {
        let mut config = template();
        config.as_object_mut().unwrap().remove(key);
        assert!(invalid(&config).contains(key), "{key}");
    }
    let mut config = template();
    config["mixer"].as_object_mut().unwrap().remove("pipeline");
    assert!(invalid(&config).contains("--mixer-pipeline"));
    let mut config = template();
    config["mixer"]["pipeline"] = json!("");
    assert!(invalid(&config).contains("--mixer-pipeline"));
}

#[test]
fn test_obstruction_and_occlusion_tables_are_required_because_sdk_dereferences_them() {
    for key in ["obstruction", "occlusion"] {
        let mut config = template();
        config["game"].as_object_mut().unwrap().remove(key);
        assert!(invalid(&config).contains(key), "{key}");
    }
}

#[test]
fn test_curves_are_optional_inside_obstruction() {
    let mut config = template();
    config["game"]["obstruction"] = json!({});
    validate_engine_config(&config).unwrap();
    config["game"]["obstruction"] = json!({"lpf_curve": {"parts": []}});
    assert!(invalid(&config).contains("lpf_curve"));
}

#[test]
fn test_non_stereo_panning_needs_hrtf_and_hrtf_needs_amir_file() {
    let mut config = template();
    config["mixer"]["panning_mode"] = json!("BinauralLowQuality");
    assert!(invalid(&config).contains("--hrtf-amir-file"));
    config["hrtf"] = json!({"hrir_sampling": "Bilinear"});
    assert!(invalid(&config).contains("--hrtf-amir-file"));
    config["hrtf"] = json!({"amir_file": "sphere.amir"});
    validate_engine_config(&config).unwrap();
}

#[test]
fn test_enum_and_number_types_are_checked_but_numeric_enums_are_accepted() {
    let mut config = template();
    config["output"]["channels"] = json!("Quadraphonic");
    assert!(invalid(&config).contains("channels"));
    config["output"]["channels"] = json!(6);
    validate_engine_config(&config).unwrap();
    config["output"]["channels"] = json!(3);
    assert!(invalid(&config).contains("channels"));
    let mut config = template();
    config["output"]["frequency"] = json!(-1);
    assert!(invalid(&config).contains("frequency"));
    let mut config = template();
    config["game"]["track_rooms"] = json!("yes");
    assert!(invalid(&config).contains("track_rooms"));
}

#[test]
fn test_unknown_keys_and_unusual_values_are_not_rejected() {
    let mut config = template();
    config["something_else"] = json!({"x": 1});
    config["output"]["buffer_size"] = json!(1);
    config["mixer"]["active_channels"] = json!(0);
    config["mixer"]["resampler"] = json!("not-registered");
    config["driver"] = json!("not-registered");
    validate_engine_config(&config).unwrap();
}

#[test]
fn test_parse_value_per_kind() {
    let spec = |key: &str| FIELDS.iter().find(|f| f.key() == key).unwrap();
    assert_eq!(
        parse_value(spec("output.frequency"), "44100").unwrap(),
        json!(44100)
    );
    assert!(parse_value(spec("output.frequency"), "-3").is_err());
    assert_eq!(
        parse_value(spec("output.format"), "int16").unwrap(),
        json!("Int16")
    );
    assert!(parse_value(spec("output.format"), "wav").is_err());
    assert_eq!(
        parse_value(spec("game.track_rooms"), "true").unwrap(),
        json!(true)
    );
    assert!(parse_value(spec("game.track_rooms"), "maybe").is_err());
    assert_eq!(
        parse_value(spec("game.sound_speed"), "340").unwrap(),
        json!(340.0)
    );
    assert_eq!(
        parse_value(spec("mixer.pipeline"), "pc").unwrap(),
        json!("pc.ampipeline")
    );
    assert_eq!(
        parse_value(spec("mixer.pipeline"), "pc.ampipeline").unwrap(),
        json!("pc.ampipeline")
    );
    assert_eq!(
        parse_value(spec("buses_file"), "main").unwrap(),
        json!("main.buses.ambus")
    );
    assert_eq!(
        parse_value(spec("buses_file"), "main.buses.json").unwrap(),
        json!("main.buses.ambus")
    );
    assert_eq!(parse_value(spec("driver"), "").unwrap(), Value::Null);
    assert!(parse_value(spec("mixer.pipeline"), "").is_err());
}

#[test]
fn test_build_config_from_minimal_values_fills_schema_defaults() {
    let config = build_config(None, &minimal_overrides()).unwrap();
    assert_eq!(config["output"]["frequency"], json!(48000));
    assert_eq!(config["output"]["buffer_size"], json!(1024));
    assert_eq!(config["output"]["format"], json!("Float32"));
    assert_eq!(config["output"]["channels"], json!("Stereo"));
    assert_eq!(config["mixer"]["max_instance_pipelines"], json!(32));
    assert_eq!(config["mixer"]["virtual_channels"], json!(0));
    assert_eq!(config["game"]["listener_fetch_mode"], json!("Nearest"));
    assert_eq!(config["game"]["track_environments"], json!(true));
    assert!(config["game"]["obstruction"]["lpf_curve"]["parts"].is_array());
    assert!(config.get("hrtf").is_none());
    assert!(config.get("driver").is_none());
    let keys: Vec<&String> = config.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["output", "mixer", "game", "buses_file"]);
    validate_engine_config(&config).unwrap();
}

#[test]
fn test_build_config_names_every_missing_required_flag() {
    let err = build_config(None, &[]).unwrap_err();
    assert_eq!(err.code, codes::ERR_VALIDATION_FIELD);
    let text = format!("{err} {}", err.suggestion);
    for flag in [
        "--mixer-pipeline",
        "--mixer-active-channels",
        "--buses-file",
    ] {
        assert!(text.contains(flag), "{flag} missing from {text}");
    }
}

#[test]
fn test_build_config_requires_amir_file_when_hrtf_is_started() {
    let mut set = minimal_overrides();
    set.push(("hrtf.hrir_sampling", json!("NearestNeighbor")));
    let err = build_config(None, &set).unwrap_err();
    assert!(err.to_string().contains("--hrtf-amir-file"));
    set.push(("hrtf.amir_file", json!("sphere.amir")));
    let config = build_config(None, &set).unwrap();
    assert_eq!(config["hrtf"]["hrir_sampling"], json!("NearestNeighbor"));
}

#[test]
fn test_build_config_base_is_overridden_by_flags_and_not_mutated() {
    let base = template();
    let before = base.clone();
    let config = build_config(Some(&base), &[("output.frequency", json!(44100))]).unwrap();
    assert_eq!(config["output"]["frequency"], json!(44100));
    assert_eq!(config["output"]["buffer_size"], json!(4096));
    assert_eq!(config["driver"], json!("miniaudio"));
    assert_eq!(base, before);
}

#[test]
fn test_apply_update_changes_only_given_fields_and_keeps_unknown_keys_and_order() {
    let mut existing = template();
    existing["custom"] = json!({"keep": true});
    existing["output"]["extra"] = json!(1);
    let updated = apply_update(&existing, None, &[("output.buffer_size", json!(512))]).unwrap();
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&updated), keys(&existing));
    assert_eq!(keys(&updated["output"]), keys(&existing["output"]));
    let mut expected = existing.clone();
    expected["output"]["buffer_size"] = json!(512);
    assert_eq!(updated, expected);
}

#[test]
fn test_apply_update_null_removes_optional_field_and_invalid_state_is_rejected() {
    let existing = template();
    let updated = apply_update(&existing, None, &[("driver", Value::Null)]).unwrap();
    assert!(updated.get("driver").is_none());
    let err = apply_update(
        &existing,
        None,
        &[("mixer.panning_mode", json!("BinauralLowQuality"))],
    )
    .unwrap_err();
    assert!(err.to_string().contains("--hrtf-amir-file"));
}

#[test]
fn test_apply_update_section_override_replaces_whole_top_level_key() {
    let existing = template();
    let section = json!({"output": {"frequency": 22050}});
    let updated = apply_update(&existing, Some(&section), &[]).unwrap();
    assert_eq!(updated["output"], json!({"frequency": 22050}));
}

#[test]
fn test_check_references_reports_unknown_pipeline_and_bus_file() {
    let config = build_config(None, &minimal_overrides()).unwrap();
    let pipelines = vec!["pc".to_string()];
    let files = vec!["main".to_string()];
    check_references(&config, &pipelines, &files).unwrap();
    let err = check_references(&config, &[], &files).unwrap_err();
    assert_eq!(err.code, codes::ERR_VALIDATION_REFERENCE);
    assert!(err.to_string().contains("pc"));
    let err = check_references(&config, &pipelines, &[]).unwrap_err();
    assert_eq!(err.code, codes::ERR_VALIDATION_REFERENCE);
    assert!(err.to_string().contains("main"));
}
