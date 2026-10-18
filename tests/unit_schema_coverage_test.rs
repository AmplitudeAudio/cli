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
use am::forms::coverage::{
    BUS_KNOWN_GAPS, BUS_SUPPORTED_FIELDS, DUCK_BUS_KNOWN_GAPS, DUCK_BUS_SUPPORTED_FIELDS,
    KNOWN_GAPS, PIPELINE_NODE_KNOWN_GAPS, PIPELINE_NODE_SUPPORTED_FIELDS, supported_fields,
};
use am::presentation::JsonOutput;
use am::schema::loader::{SchemaRegistry, load_schemas, table_field_names};

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

fn bus_table_fields(table: &str) -> Vec<String> {
    let sdk = discover_sdk().expect("AM_SDK_PATH must point at an SDK with schemas/*.bfbs");
    table_field_names(&sdk, "buses_definition.bfbs", table).expect("bus schema must load")
}

/// Buses are not an `AssetType`, so their tables are checked separately.
#[test]
fn test_every_bus_schema_field_is_supported_or_a_known_gap() {
    for (table, supported, gaps) in [
        ("BusDefinition", BUS_SUPPORTED_FIELDS, BUS_KNOWN_GAPS),
        (
            "DuckBusDefinition",
            DUCK_BUS_SUPPORTED_FIELDS,
            DUCK_BUS_KNOWN_GAPS,
        ),
    ] {
        let fields = bus_table_fields(table);
        let missing: Vec<&String> = fields
            .iter()
            .filter(|f| !supported.contains(&f.as_str()) && !gaps.contains(&f.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "{table} fields neither supported nor known gaps: {missing:?}"
        );
        for name in supported.iter().chain(gaps) {
            assert!(
                fields.iter().any(|f| f == name),
                "{table}.{name} is not a schema field"
            );
        }
        assert!(
            gaps.iter().all(|g| !supported.contains(g)),
            "{table}: a field is both supported and a known gap"
        );
    }
}

/// Pipeline nodes are a nested table, so their fields are checked separately.
#[test]
fn test_every_pipeline_node_schema_field_is_supported_or_a_known_gap() {
    let sdk = discover_sdk().expect("AM_SDK_PATH must point at an SDK with schemas/*.bfbs");
    let fields = table_field_names(&sdk, "pipeline_definition.bfbs", "PipelineNodeDefinition")
        .expect("pipeline schema must load");
    for field in &fields {
        assert!(
            PIPELINE_NODE_SUPPORTED_FIELDS.contains(&field.as_str())
                || PIPELINE_NODE_KNOWN_GAPS.contains(&field.as_str()),
            "PipelineNodeDefinition.{field} is neither supported nor a known gap"
        );
    }
    for name in PIPELINE_NODE_SUPPORTED_FIELDS
        .iter()
        .chain(PIPELINE_NODE_KNOWN_GAPS)
    {
        assert!(
            fields.iter().any(|f| f == name),
            "PipelineNodeDefinition.{name} is not a schema field"
        );
    }
}
