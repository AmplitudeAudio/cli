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

//! Tests for strict asset reference validation.

mod common;

use am::assets::AssetType;
use am::common::errors::codes::{ERR_VALIDATION_EMPTY_REFERENCE, ERR_VALIDATION_REFERENCE};
use am::forms::references::{RefRule, RefTarget, rule_for};
use am::forms::validate_refs::validate_reference;
use common::fixtures::AssetTestFixture;

fn fixture_with_buses() -> AssetTestFixture {
    let fixture = AssetTestFixture::new("refs").unwrap();
    fixture
        .write_buses_file(
            "main.buses.json",
            serde_json::json!([{ "id": 1, "name": "master" }, { "id": 2, "name": "sfx" }]),
        )
        .unwrap();
    fixture
}

fn bus_rule() -> RefRule {
    rule_for(AssetType::Sound, "bus").unwrap()
}

#[test]
fn test_required_rule_with_zero_is_empty_reference() {
    let fixture = fixture_with_buses();
    let validator = fixture.create_project_validator().unwrap();
    let err = validate_reference(&bus_rule(), 0, &validator).unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_EMPTY_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("bus"));
    assert!(err.suggestion().contains("am asset bus"));
}

#[test]
fn test_dangling_id_is_reference_error() {
    let fixture = fixture_with_buses();
    let validator = fixture.create_project_validator().unwrap();
    let err = validate_reference(&bus_rule(), 999, &validator).unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("bus"));
    assert!(err.what().contains("999") || err.why().contains("999"));
}

#[test]
fn test_resolving_id_passes() {
    let fixture = fixture_with_buses();
    let validator = fixture.create_project_validator().unwrap();
    assert!(validate_reference(&bus_rule(), 2, &validator).is_ok());
}

#[test]
fn test_optional_rule_with_zero_passes() {
    let fixture = fixture_with_buses();
    let validator = fixture.create_project_validator().unwrap();
    let rule = rule_for(AssetType::Sound, "attenuation").unwrap();
    assert!(!rule.required);
    assert!(validate_reference(&rule, 0, &validator).is_ok());
}

#[test]
fn test_optional_rule_with_dangling_id_fails() {
    let fixture = fixture_with_buses();
    let validator = fixture.create_project_validator().unwrap();
    let rule = rule_for(AssetType::Sound, "attenuation").unwrap();
    assert_eq!(rule.target, RefTarget::Asset(AssetType::Attenuation));
    let err = validate_reference(&rule, 42, &validator).unwrap_err();
    assert_eq!(err.code(), ERR_VALIDATION_REFERENCE);
}
