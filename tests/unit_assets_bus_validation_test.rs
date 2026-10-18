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

//! Tests for `assets::bus::validate_bus_file`, which mirrors what the SDK
//! needs from a `*.buses.json` file to load it (Engine.cpp, BusInternalState.cpp).

use am::assets::bus::validate_bus_file;
use serde_json::{Value, json};

fn valid() -> Value {
    json!({"buses": [
        {"id": 1, "name": "master", "gain": 1.0, "child_buses": [2, 3], "fader": "Linear"},
        {"id": 2, "name": "sfx", "gain": 1.0, "fader": "Linear",
         "duck_buses": [{
            "id": 3, "target_gain": 0.5,
            "fade_in": {"duration": 1.0, "fader": "Linear"},
            "fade_out": {"duration": 1.0, "fader": "Ease"}
         }]},
        {"id": 3, "name": "music", "gain": 1.0, "fader": "Linear"}
    ]})
}

fn message(v: &Value) -> String {
    format!("{:#}", validate_bus_file(v).unwrap_err())
}

#[test]
fn test_valid_file_is_accepted() {
    validate_bus_file(&valid()).unwrap();
}

#[test]
fn test_unknown_keys_are_not_an_error() {
    let mut v = valid();
    v["editor"] = json!({"zoom": 2});
    v["buses"][1]["color"] = json!("red");
    validate_bus_file(&v).unwrap();
}

#[test]
fn test_missing_or_empty_buses_array_is_rejected() {
    assert!(message(&json!({})).contains("buses"));
    assert!(message(&json!({"buses": []})).contains("master"));
}

#[test]
fn test_master_with_id_1_is_required() {
    let v = json!({"buses": [{"id": 5, "name": "sfx", "fader": "Linear"}]});
    assert!(message(&v).contains("master"), "{}", message(&v));
}

#[test]
fn test_master_name_must_use_id_1() {
    let v = json!({"buses": [
        {"id": 1, "name": "root", "fader": "Linear", "child_buses": [2]},
        {"id": 2, "name": "master", "fader": "Linear"}]});
    assert!(message(&v).contains("master"), "{}", message(&v));
}

#[test]
fn test_duplicate_names_and_ids_are_rejected() {
    let mut v = valid();
    v["buses"][2]["name"] = json!("sfx");
    assert!(message(&v).contains("sfx"), "{}", message(&v));

    let mut v = valid();
    v["buses"][2]["id"] = json!(2);
    v["buses"][0]["child_buses"] = json!([2]);
    assert!(message(&v).contains("id 2"), "{}", message(&v));
}

#[test]
fn test_zero_id_and_empty_name_are_rejected() {
    let mut v = valid();
    v["buses"][2]["id"] = json!(0);
    assert!(message(&v).contains("id"), "{}", message(&v));

    let mut v = valid();
    v["buses"][2]["name"] = json!("");
    assert!(message(&v).contains("name"), "{}", message(&v));
}

#[test]
fn test_missing_fader_is_rejected_because_the_sdk_dereferences_it() {
    let mut v = valid();
    v["buses"][2].as_object_mut().unwrap().remove("fader");
    let m = message(&v);
    assert!(m.contains("music") && m.contains("fader"), "{m}");
}

#[test]
fn test_unknown_fader_is_rejected() {
    let mut v = valid();
    v["buses"][2]["fader"] = json!("Bouncy");
    assert!(message(&v).contains("Bouncy"));
}

#[test]
fn test_dangling_child_is_rejected() {
    let mut v = valid();
    v["buses"][0]["child_buses"] = json!([2, 3, 99]);
    assert!(message(&v).contains("99"), "{}", message(&v));
}

#[test]
fn test_bus_with_two_parents_is_rejected() {
    let mut v = valid();
    v["buses"][1]["child_buses"] = json!([3]);
    assert!(message(&v).contains("music"), "{}", message(&v));
}

#[test]
fn test_cycle_and_master_as_child_are_rejected() {
    let mut v = valid();
    v["buses"][0]["child_buses"] = json!([2]);
    v["buses"][1]["child_buses"] = json!([3]);
    v["buses"][2]["child_buses"] = json!([2]);
    assert!(
        message(&v).to_lowercase().contains("parent"),
        "{}",
        message(&v)
    );

    let mut v = valid();
    v["buses"][1]["child_buses"] = json!([1]);
    assert!(message(&v).contains("master"), "{}", message(&v));

    let mut v = valid();
    v["buses"][0]["child_buses"] = json!([2, 3, 1]);
    assert!(message(&v).contains("itself"), "{}", message(&v));
}

#[test]
fn test_unreachable_cycle_is_rejected() {
    // 2 -> 3 -> 2 with nobody above them: each has exactly one parent.
    let v = json!({"buses": [
        {"id": 1, "name": "master", "fader": "Linear"},
        {"id": 2, "name": "a", "fader": "Linear", "child_buses": [3]},
        {"id": 3, "name": "b", "fader": "Linear", "child_buses": [2]}]});
    assert!(
        message(&v).to_lowercase().contains("cycle"),
        "{}",
        message(&v)
    );
}

#[test]
fn test_duck_bus_rules() {
    let base = valid();

    let mut v = base.clone();
    v["buses"][1]["duck_buses"][0]["id"] = json!(99);
    assert!(message(&v).contains("99"), "{}", message(&v));

    let mut v = base.clone();
    v["buses"][1]["duck_buses"][0]["id"] = json!(2);
    assert!(message(&v).contains("itself"), "{}", message(&v));

    let mut v = base.clone();
    v["buses"][1]["duck_buses"][0]
        .as_object_mut()
        .unwrap()
        .remove("fade_in");
    assert!(message(&v).contains("fade_in"), "{}", message(&v));

    let mut v = base.clone();
    v["buses"][1]["duck_buses"][0]["fade_out"]
        .as_object_mut()
        .unwrap()
        .remove("fader");
    assert!(message(&v).contains("fade_out"), "{}", message(&v));

    let mut v = base.clone();
    v["buses"][1]["duck_buses"][0]["fade_out"]["fader"] = json!("Nope");
    assert!(message(&v).contains("Nope"), "{}", message(&v));

    let mut v = base;
    v["buses"][1]["duck_buses"][0]["fade_in"]["duration"] = json!(-1.0);
    assert!(message(&v).contains("duration"), "{}", message(&v));
}

#[test]
fn test_gain_must_not_be_negative() {
    let mut v = valid();
    v["buses"][2]["gain"] = json!(-0.5);
    assert!(message(&v).contains("gain"), "{}", message(&v));
}
