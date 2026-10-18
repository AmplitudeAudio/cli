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

//! Tests for the reference picker and RTPC value input helpers.

mod common;

use std::fs;

use am::assets::generated::ValueKind;
use am::assets::{AssetType, ProjectValidator};
use am::forms::pick::pick_reference;
use am::forms::references::{RefRule, rule_for};
use am::forms::rtpc_value::{prompt_rtpc_value, rtpc_value_from_flags};
use common::fixtures::AssetTestFixture;
use common::scripted_input::ScriptedInput;
use serde_json::json;

fn bus_rule(required: bool) -> RefRule {
    RefRule {
        required,
        ..rule_for(AssetType::Sound, "bus").unwrap()
    }
}

fn fixture() -> (AssetTestFixture, ProjectValidator) {
    let fx = AssetTestFixture::new("pick").unwrap();
    fx.write_buses_file(
        "main.buses.json",
        json!([{"id": 1, "name": "master"}, {"id": 2, "name": "sfx"}]),
    )
    .unwrap();
    fx.write_asset_json("rtpc", "speed", json!({"id": 77, "name": "speed"}))
        .unwrap();
    let v = fx.create_project_validator().unwrap();
    (fx, v)
}

#[test]
fn test_required_picker_returns_chosen_id() {
    let (_fx, v) = fixture();
    let input = ScriptedInput::new(&["sfx"], &[]);
    let got = pick_reference(&input, &v, &bus_rule(true)).unwrap();
    assert_eq!(got, Some(2));
    let seen = input.options_seen.lock().unwrap();
    assert_eq!(seen[0], vec!["master".to_string(), "sfx".to_string()]);
}

#[test]
fn test_required_picker_empty_errors_with_create_hint() {
    let fx = AssetTestFixture::new("pick_empty").unwrap();
    // The fixture ships a bus file; this test needs a project with no buses.
    fs::remove_file(fx.project_root().join("main.buses.json")).unwrap();
    let v = fx.create_project_validator().unwrap();
    let input = ScriptedInput::new(&[], &[]);
    let err = pick_reference(&input, &v, &bus_rule(true)).unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("create"), "{text}");
    assert!(text.contains("am asset bus create"), "{text}");
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[test]
fn test_optional_picker_none_entry() {
    let (_fx, v) = fixture();
    let input = ScriptedInput::new(&["(none)"], &[]);
    assert_eq!(pick_reference(&input, &v, &bus_rule(false)).unwrap(), None);
    assert_eq!(
        input.options_seen.lock().unwrap()[0].last().unwrap(),
        "(none)"
    );
}

#[test]
fn test_optional_picker_empty_offers_only_none() {
    let fx = AssetTestFixture::new("pick_opt_empty").unwrap();
    // The fixture ships a bus file; this test needs a project with no buses.
    fs::remove_file(fx.project_root().join("main.buses.json")).unwrap();
    let v = fx.create_project_validator().unwrap();
    let input = ScriptedInput::new(&["(none)"], &[]);
    assert_eq!(pick_reference(&input, &v, &bus_rule(false)).unwrap(), None);
    assert_eq!(
        input.options_seen.lock().unwrap()[0],
        vec!["(none)".to_string()]
    );
}

#[test]
fn test_required_picker_has_no_none_entry() {
    let (_fx, v) = fixture();
    let input = ScriptedInput::new(&["master"], &[]);
    pick_reference(&input, &v, &bus_rule(true)).unwrap();
    assert!(!input.options_seen.lock().unwrap()[0].contains(&"(none)".to_string()));
}

#[test]
fn test_flags_both_set_errors_naming_both() {
    let (_fx, v) = fixture();
    let err = rtpc_value_from_flags(Some(0.5), Some("speed"), &v).unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("value") && text.contains("rtpc"), "{text}");
}

#[test]
fn test_flags_neither_is_none() {
    let (_fx, v) = fixture();
    assert!(rtpc_value_from_flags(None, None, &v).unwrap().is_none());
}

#[test]
fn test_flags_value_only_is_static() {
    let (_fx, v) = fixture();
    let got = rtpc_value_from_flags(Some(0.8), None, &v).unwrap().unwrap();
    assert_eq!(got.kind, ValueKind::Static);
    assert_eq!(got.value, 0.8);
}

#[test]
fn test_flags_name_resolves_rtpc() {
    let (_fx, v) = fixture();
    let got = rtpc_value_from_flags(None, Some("speed"), &v)
        .unwrap()
        .unwrap();
    assert_eq!(got.kind, ValueKind::RTPC);
    assert_eq!(got.rtpc.unwrap().id, 77);
}

#[test]
fn test_flags_unknown_name_errors_with_suggestion() {
    let (_fx, v) = fixture();
    let err = rtpc_value_from_flags(None, Some("nope"), &v).unwrap_err();
    let text = format!("{err:#}");
    assert!(
        text.contains("nope") && text.contains("am asset rtpc create"),
        "{text}"
    );
}

#[test]
fn test_prompt_static_value() {
    let (_fx, v) = fixture();
    let input = ScriptedInput::new(&["static"], &["0.25"]);
    let got = prompt_rtpc_value(&input, &v, "Gain", 1.0).unwrap();
    assert_eq!(got.kind, ValueKind::Static);
    assert_eq!(got.value, 0.25);
}

#[test]
fn test_prompt_static_rejects_non_number() {
    let (_fx, v) = fixture();
    let input = ScriptedInput::new(&["static"], &["abc"]);
    assert!(prompt_rtpc_value(&input, &v, "Gain", 1.0).is_err());
}

#[test]
fn test_prompt_bind_to_rtpc_uses_linear_curve() {
    let (_fx, v) = fixture();
    let input = ScriptedInput::new(&["bind to RTPC", "speed"], &[]);
    let got = prompt_rtpc_value(&input, &v, "Gain", 1.0).unwrap();
    assert_eq!(got.kind, ValueKind::RTPC);
    let p = got.rtpc.unwrap();
    assert_eq!(p.id, 77);
    let parts = p.curve.unwrap().parts.unwrap();
    assert_eq!(parts.len(), 1);
    let (s, e) = (
        parts[0].start.as_ref().unwrap(),
        parts[0].end.as_ref().unwrap(),
    );
    assert_eq!((s.x, s.y, e.x, e.y), (0.0, 0.0, 1.0, 1.0));
}

#[test]
fn test_prompt_bind_without_rtpc_assets_errors() {
    let fx = AssetTestFixture::new("pick_no_rtpc").unwrap();
    let v = fx.create_project_validator().unwrap();
    let input = ScriptedInput::new(&["bind to RTPC"], &[]);
    let err = prompt_rtpc_value(&input, &v, "Gain", 1.0).unwrap_err();
    assert!(format!("{err:#}").contains("create"));
}

#[test]
fn test_flags_reject_non_finite_values() {
    let (_fx, v) = fixture();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let err = rtpc_value_from_flags(Some(bad), None, &v).unwrap_err();
        let cli = am::common::errors::as_cli_error(&err).expect("structured error");
        assert_eq!(cli.code, am::common::errors::codes::ERR_VALIDATION_FIELD);
    }
}
