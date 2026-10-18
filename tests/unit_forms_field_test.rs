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

//! Tests for the schema-driven field model.

use am::assets::AssetType;
use am::config::sdk::discover_sdk;
use am::forms::describe::describe;
use am::forms::field::{FieldKind, FieldSpec};
use am::forms::field_docs::doc_for;
use am::forms::references::RefTarget;
use am::presentation::JsonOutput;
use am::schema::loader::{SchemaRegistry, load_schemas};
use serde_json::json;

fn registry() -> SchemaRegistry {
    let sdk = discover_sdk().expect("AM_SDK_PATH must point at an SDK with schemas/*.bfbs");
    load_schemas(&sdk, &JsonOutput::new()).expect("schemas must load")
}

fn sound_fields() -> Vec<FieldSpec> {
    describe(&registry(), AssetType::Sound).expect("describe sound")
}

fn field<'a>(fields: &'a [FieldSpec], name: &str) -> &'a FieldSpec {
    fields
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("missing field {name}"))
}

#[test]
fn test_sound_has_exactly_16_fields() {
    assert_eq!(sound_fields().len(), 16);
}

#[test]
fn test_sound_gain_is_rtpc_value() {
    assert_eq!(field(&sound_fields(), "gain").kind, FieldKind::RtpcValue);
}

#[test]
fn test_sound_stream_is_bool() {
    assert_eq!(field(&sound_fields(), "stream").kind, FieldKind::Bool);
}

#[test]
fn test_sound_spatialization_is_enum_with_position() {
    let fields = sound_fields();
    match &field(&fields, "spatialization").kind {
        FieldKind::Enum(variants) => assert!(variants.iter().any(|v| v == "Position")),
        other => panic!("expected Enum, got {other:?}"),
    }
}

#[test]
fn test_sound_bus_is_reference_to_bus() {
    assert_eq!(
        field(&sound_fields(), "bus").kind,
        FieldKind::Reference(RefTarget::Bus)
    );
}

#[test]
fn test_sound_loop_is_nested_and_name_is_text() {
    let fields = sound_fields();
    assert_eq!(
        field(&fields, "loop").kind,
        FieldKind::Nested("SoundLoopConfig".to_string())
    );
    assert_eq!(field(&fields, "name").kind, FieldKind::Text);
}

#[test]
fn test_sound_defaults_come_from_schema() {
    let fields = sound_fields();
    assert_eq!(field(&fields, "stream").default, Some(json!(false)));
    assert_eq!(
        field(&fields, "spatialization").default,
        Some(json!("None"))
    );
    assert_eq!(field(&fields, "scope").default, Some(json!("World")));
    assert_eq!(field(&fields, "name").default, None);
}

#[test]
fn test_sound_required_comes_from_schema() {
    assert!(!field(&sound_fields(), "stream").required);
}

#[test]
fn test_sound_docs_filled_for_every_field() {
    for f in sound_fields() {
        assert!(f.doc.is_some(), "no doc for sound field {}", f.name);
    }
}

#[test]
fn test_doc_for_known_and_unknown() {
    assert!(doc_for(AssetType::Sound, "bus").unwrap().contains("bus"));
    assert_eq!(doc_for(AssetType::Sound, "nonexistent"), None);
    assert_eq!(doc_for(AssetType::Event, "nonexistent"), None);
}

#[test]
fn test_no_union_discriminator_fields_leak() {
    let reg = registry();
    for t in reg.asset_types() {
        let names: Vec<String> = describe(&reg, t)
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        for name in &names {
            if let Some(base) = name.strip_suffix("_type") {
                assert!(
                    !names.iter().any(|n| n == base),
                    "{t:?} leaks union discriminator {name}"
                );
            }
        }
    }
}

#[test]
fn test_describe_all_types_succeeds() {
    let reg = registry();
    for t in reg.asset_types() {
        assert!(!describe(&reg, t).unwrap().is_empty(), "{t:?} empty");
    }
}
