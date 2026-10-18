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

//! Keeps the list of uncovered SDK schema fields honest.

use am::assets::AssetType;
use am::config::sdk::discover_sdk;
use am::forms::coverage::{KNOWN_GAPS, supported_fields};
use am::presentation::JsonOutput;
use am::schema::loader::{SchemaRegistry, load_schemas};

const ALL_TYPES: [AssetType; 10] = [
    AssetType::Attenuation,
    AssetType::Rtpc,
    AssetType::Pipeline,
    AssetType::Sound,
    AssetType::Collection,
    AssetType::Switch,
    AssetType::SwitchContainer,
    AssetType::Soundbank,
    AssetType::Event,
    AssetType::Effect,
];

fn registry() -> SchemaRegistry {
    let sdk = discover_sdk().expect("AM_SDK_PATH must point at an SDK with schemas/*.bfbs");
    load_schemas(&sdk, &JsonOutput::new()).expect("schemas must load")
}

#[test]
fn test_every_schema_field_is_supported_or_a_known_gap() {
    let registry = registry();
    let mut missing = Vec::new();
    for t in ALL_TYPES {
        let schema = registry
            .get(t)
            .unwrap_or_else(|| panic!("no schema for {t:?}"));
        for field in &schema.fields {
            let name = field.name.as_str();
            if !supported_fields(t).contains(&name) && !KNOWN_GAPS.contains(&(t, name)) {
                missing.push(format!("{t:?}.{name}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "schema fields neither supported nor listed as known gaps: {missing:?}"
    );
}

#[test]
fn test_known_gaps_are_real_schema_fields() {
    let registry = registry();
    for (t, name) in KNOWN_GAPS {
        let schema = registry
            .get(*t)
            .unwrap_or_else(|| panic!("no schema for {t:?}"));
        assert!(
            schema.has_field(name),
            "KNOWN_GAPS entry {t:?}.{name} is not a schema field"
        );
        assert!(
            !supported_fields(*t).contains(name),
            "{t:?}.{name} is listed as both supported and a known gap"
        );
    }
}

#[test]
fn test_supported_fields_are_real_schema_fields() {
    let registry = registry();
    for t in ALL_TYPES {
        let schema = registry.get(t).unwrap();
        for name in supported_fields(t) {
            assert!(
                schema.has_field(name),
                "supported field {t:?}.{name} is not a schema field"
            );
        }
    }
}
