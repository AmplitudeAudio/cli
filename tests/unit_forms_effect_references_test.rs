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

//! Who references effects and switches: `referrers` and the reference table.

mod common;

use am::assets::{AssetType, ProjectValidator};
use am::forms::references::{RefTarget, referrers, rules_for};
use am::presentation::JsonOutput;
use common::fixtures::AssetTestFixture;
use serde_json::json;

const EFFECT: RefTarget = RefTarget::Asset(AssetType::Effect);
const RTPC: RefTarget = RefTarget::Asset(AssetType::Rtpc);
const SWITCH: RefTarget = RefTarget::Asset(AssetType::Switch);

fn validator(fx: &AssetTestFixture) -> ProjectValidator {
    ProjectValidator::new(fx.project_root().to_path_buf(), &JsonOutput::new()).unwrap()
}

fn names(found: Vec<am::assets::validator::AssetRef>) -> Vec<String> {
    let mut names: Vec<String> = found.into_iter().map(|r| r.name).collect();
    names.sort();
    names
}

fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.write_asset_json("rtpc", "speed", json!({"id": 401, "name": "speed"}))
        .unwrap();
    fx.write_asset_json(
        "effects",
        "echo",
        json!({"id": 66, "name": "echo", "effect": "Delay", "parameters": [
            {"kind": "Static", "value": 1.0},
            {"kind": "RTPC", "rtpc": {"id": 401, "curve": {"parts": []}}}
        ]}),
    )
    .unwrap();
    fx.write_asset_json(
        "effects",
        "plain",
        json!({"id": 67, "name": "plain", "effect": "Lofi", "parameters": [{"kind": "Static", "value": 1.0}]}),
    )
    .unwrap();
    fx.write_asset_json("switches", "surface", json!({"id": 10, "name": "surface", "states": [
        {"id": 11, "name": "wood"}]}))
        .unwrap();
    fx
}

#[test]
fn test_effect_is_referenced_by_sounds_collections_and_switch_containers() {
    let fx = fixture("ref_fx_users");
    fx.write_asset_json("sounds", "s", json!({"id": 1, "name": "s", "bus": 1, "effect": 66}))
        .unwrap();
    fx.write_asset_json("collections", "c", json!({"id": 2, "name": "c", "bus": 1, "effect": 66}))
        .unwrap();
    fx.write_asset_json(
        "switch_containers",
        "sc",
        json!({"id": 3, "name": "sc", "bus": 1, "switch_group": 10, "effect": 66}),
    )
    .unwrap();
    fx.write_asset_json("sounds", "other", json!({"id": 4, "name": "other", "bus": 1, "effect": 67}))
        .unwrap();

    assert_eq!(names(referrers(&validator(&fx), EFFECT, 66)), vec!["c", "s", "sc"]);
    assert_eq!(names(referrers(&validator(&fx), EFFECT, 67)), vec!["other"]);
}

#[test]
fn test_an_rtpc_is_referenced_by_the_effects_whose_parameters_it_drives() {
    let fx = fixture("ref_fx_rtpc");
    assert_eq!(names(referrers(&validator(&fx), RTPC, 401)), vec!["echo"]);
}

#[test]
fn test_a_switch_is_referenced_by_its_switch_containers() {
    let fx = fixture("ref_sw_users");
    fx.write_asset_json(
        "switch_containers",
        "sc",
        json!({"id": 3, "name": "sc", "bus": 1, "switch_group": 10}),
    )
    .unwrap();
    assert_eq!(names(referrers(&validator(&fx), SWITCH, 10)), vec!["sc"]);
    assert!(referrers(&validator(&fx), SWITCH, 99).is_empty());
}

#[test]
fn test_effects_and_switches_hold_no_asset_references_of_their_own() {
    assert!(rules_for(AssetType::Effect).is_empty());
    assert!(rules_for(AssetType::Switch).is_empty());
}
