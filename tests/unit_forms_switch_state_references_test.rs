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

//! Which switch containers use a given state of a switch.

mod common;

use am::assets::ProjectValidator;
use am::forms::references::switch_state_referrers;
use am::presentation::JsonOutput;
use common::fixtures::AssetTestFixture;
use serde_json::json;

fn validator(fx: &AssetTestFixture) -> ProjectValidator {
    ProjectValidator::new(fx.project_root().to_path_buf(), &JsonOutput::new()).unwrap()
}

fn names(found: Vec<am::assets::validator::AssetRef>) -> Vec<String> {
    let mut names: Vec<String> = found.into_iter().map(|r| r.name).collect();
    names.sort();
    names
}

#[test]
fn test_state_referrers_match_the_default_state_and_the_entry_states_of_that_switch() {
    let fx = AssetTestFixture::new("ref_state").unwrap();
    fx.write_asset_json(
        "switch_containers",
        "by_default",
        json!({"id": 1, "name": "by_default", "bus": 2, "switch_group": 10, "default_switch_state": 11}),
    )
    .unwrap();
    fx.write_asset_json(
        "switch_containers",
        "by_entry",
        json!({"id": 2, "name": "by_entry", "bus": 2, "switch_group": 10, "default_switch_state": 11,
               "entries": [{"object": 5, "switch_states": [12, 13]}]}),
    )
    .unwrap();
    fx.write_asset_json(
        "switch_containers",
        "other_switch",
        json!({"id": 3, "name": "other_switch", "bus": 2, "switch_group": 20, "default_switch_state": 12,
               "entries": [{"object": 5, "switch_states": [11, 12]}]}),
    )
    .unwrap();

    let v = validator(&fx);
    assert_eq!(
        names(switch_state_referrers(&v, 10, 11)),
        vec!["by_default", "by_entry"]
    );
    assert_eq!(names(switch_state_referrers(&v, 10, 12)), vec!["by_entry"]);
    assert_eq!(names(switch_state_referrers(&v, 10, 13)), vec!["by_entry"]);
    assert_eq!(
        names(switch_state_referrers(&v, 20, 12)),
        vec!["other_switch"]
    );
    assert!(switch_state_referrers(&v, 10, 99).is_empty());
    assert!(
        switch_state_referrers(&v, 10, 0).is_empty(),
        "0 is never a reference"
    );
}
