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

//! Tests that generated types round-trip flatc's JSON representation of
//! FlatBuffers unions: a `<field>_type` tag sibling plus the `<field>` payload,
//! and parallel arrays for vectors of unions.

use am::assets::generated::*;
use serde_json::json;

fn box_zone() -> ZoneDefinition {
    ZoneDefinition::Box(BoxZoneSettings {
        inner: Some(BoxShapeDefinition {
            half_width: 1.0,
            half_height: 2.0,
            half_depth: 3.0,
        }),
        outer: None,
    })
}

fn shape_with(zone: Option<ZoneDefinition>) -> AttenuationShapeDefinition {
    AttenuationShapeDefinition {
        max_attenuation_factor: 1.0,
        zone,
    }
}

#[test]
fn zone_box_serializes_with_type_tag_and_round_trips() {
    let shape = shape_with(Some(box_zone()));
    let value = serde_json::to_value(&shape).unwrap();

    assert_eq!(value["zone_type"], "Box");
    assert_eq!(value["zone"]["inner"]["half_height"], 2.0);

    let parsed: AttenuationShapeDefinition = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, shape);
}

#[test]
fn zone_sphere_serializes_with_type_tag_and_round_trips() {
    let shape = shape_with(Some(ZoneDefinition::Sphere(SphereZoneSettings {
        inner: Some(SphereShapeDefinition { radius: 4.0 }),
        outer: Some(SphereShapeDefinition { radius: 8.0 }),
    })));
    let value = serde_json::to_value(&shape).unwrap();

    assert_eq!(value["zone_type"], "Sphere");
    assert_eq!(value["zone"]["outer"]["radius"], 8.0);

    let parsed: AttenuationShapeDefinition = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, shape);
}

#[test]
fn zone_type_key_precedes_zone_key_in_text() {
    // flatc's parser requires the tag to appear before the payload.
    let text = serde_json::to_string(&shape_with(Some(box_zone()))).unwrap();
    let tag = text.find("\"zone_type\"").unwrap();
    let payload = text.find("\"zone\":").unwrap();
    assert!(tag < payload, "{text}");
}

#[test]
fn ambiguous_payload_is_disambiguated_by_tag() {
    // An empty payload is valid for every variant; only the tag decides.
    for (tag, check) in [
        ("Box", "Box"),
        ("Capsule", "Capsule"),
        ("Cone", "Cone"),
        ("Sphere", "Sphere"),
    ] {
        let shape: AttenuationShapeDefinition = serde_json::from_value(json!({
            "max_attenuation_factor": 0.5,
            "zone_type": tag,
            "zone": {}
        }))
        .unwrap();
        let got = match shape.zone.unwrap() {
            ZoneDefinition::Box(_) => "Box",
            ZoneDefinition::Capsule(_) => "Capsule",
            ZoneDefinition::Cone(_) => "Cone",
            ZoneDefinition::Sphere(_) => "Sphere",
        };
        assert_eq!(got, check);
    }
}

#[test]
fn unknown_tag_is_an_error() {
    let err = serde_json::from_value::<AttenuationShapeDefinition>(json!({
        "max_attenuation_factor": 1.0,
        "zone_type": "Pyramid",
        "zone": {}
    }))
    .unwrap_err()
    .to_string();
    assert!(err.contains("Pyramid"), "{err}");
}

#[test]
fn payload_without_tag_is_an_error() {
    let result = serde_json::from_value::<AttenuationShapeDefinition>(json!({
        "max_attenuation_factor": 1.0,
        "zone": {}
    }));
    assert!(result.is_err());
}

#[test]
fn tag_without_payload_is_an_error() {
    let result = serde_json::from_value::<AttenuationShapeDefinition>(json!({
        "max_attenuation_factor": 1.0,
        "zone_type": "Box"
    }));
    assert!(result.is_err());
}

#[test]
fn absent_zone_emits_no_keys() {
    let value = serde_json::to_value(shape_with(None)).unwrap();
    let object = value.as_object().unwrap();
    assert!(!object.contains_key("zone"));
    assert!(!object.contains_key("zone_type"));

    let parsed: AttenuationShapeDefinition = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.zone, None);
}

#[test]
fn none_tag_reads_as_absent() {
    let parsed: AttenuationShapeDefinition = serde_json::from_value(json!({
        "max_attenuation_factor": 1.0,
        "zone_type": "NONE"
    }))
    .unwrap();
    assert_eq!(parsed.zone, None);
}

#[test]
fn abstract_shape_in_shape_definition_round_trips() {
    let shape = ShapeDefinition {
        shape: Some(AbstractShapeDefinition::Capsule(CapsuleShapeDefinition {
            radius: 1.5,
            half_height: 2.5,
        })),
    };
    let value = serde_json::to_value(&shape).unwrap();
    assert_eq!(value["shape_type"], "Capsule");
    assert_eq!(value["shape"]["half_height"], 2.5);

    let parsed: ShapeDefinition = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, shape);

    // Same payload shape, different tag: Sphere only has `radius`.
    let sphere: ShapeDefinition = serde_json::from_value(json!({
        "shape_type": "Sphere",
        "shape": { "radius": 3.0 }
    }))
    .unwrap();
    assert_eq!(
        sphere.shape,
        Some(AbstractShapeDefinition::Sphere(SphereShapeDefinition {
            radius: 3.0
        }))
    );
}

#[test]
fn scheduler_config_union_round_trips() {
    let settings = SoundSchedulerSettings {
        mode: SoundSchedulerMode::Random,
        config: Some(SoundSchedulerConfig::Random(RandomSoundSchedulerConfig {
            avoid_repeat: true,
            repeat_count: 2,
        })),
    };
    let value = serde_json::to_value(&settings).unwrap();
    assert_eq!(value["config_type"], "Random");
    assert_eq!(value["config"]["repeat_count"], 2);

    let parsed: SoundSchedulerSettings = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, settings);

    let without = SoundSchedulerSettings {
        mode: SoundSchedulerMode::Sequence,
        config: None,
    };
    let value = serde_json::to_value(&without).unwrap();
    assert!(!value.as_object().unwrap().contains_key("config_type"));
    assert!(!value.as_object().unwrap().contains_key("config"));
}

fn collection_with(sounds: Option<Vec<CollectionEntry>>) -> CollectionDefinition {
    let mut value = json!({
        "id": 1,
        "name": "c",
        "effect": 0,
        "bus": 0,
        "attenuation": 0,
        "spatialization": "None",
        "play_mode": "PlayOne",
        "scope": "World"
    });
    // Deserialize through JSON so the helper stays independent of field churn.
    value["fader"] = json!("Linear");
    let mut collection: CollectionDefinition = serde_json::from_value(value).unwrap();
    collection.sounds = sounds;
    collection
}

#[test]
fn collection_entries_are_parallel_type_and_value_arrays() {
    let collection = collection_with(Some(vec![
        CollectionEntry::Random(RandomSchedulerCollectionEntry {
            sound: 5,
            gain: None,
            pitch: None,
            weight: 2.0,
        }),
        CollectionEntry::Default(DefaultCollectionEntry {
            sound: 6,
            gain: None,
            pitch: None,
        }),
    ]));
    let value = serde_json::to_value(&collection).unwrap();

    assert_eq!(value["sounds_type"], json!(["Random", "Default"]));
    assert_eq!(value["sounds"][0]["sound"], 5);
    assert_eq!(value["sounds"][0]["weight"], 2.0);
    assert_eq!(value["sounds"][1]["sound"], 6);

    let parsed: CollectionDefinition = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, collection);
}

#[test]
fn collection_entry_ambiguous_payload_uses_tag() {
    let mut value = serde_json::to_value(collection_with(None)).unwrap();
    value["sounds_type"] = json!(["Sequence", "Default", "Random"]);
    value["sounds"] = json!([{ "sound": 1 }, { "sound": 2 }, { "sound": 3 }]);

    let parsed: CollectionDefinition = serde_json::from_value(value).unwrap();
    let sounds = parsed.sounds.unwrap();
    assert!(matches!(sounds[0], CollectionEntry::Sequence(_)));
    assert!(matches!(sounds[1], CollectionEntry::Default(_)));
    assert!(matches!(sounds[2], CollectionEntry::Random(_)));
}

#[test]
fn collection_entry_length_mismatch_and_unknown_tag_are_errors() {
    let mut value = serde_json::to_value(collection_with(None)).unwrap();
    value["sounds_type"] = json!(["Default"]);
    value["sounds"] = json!([{ "sound": 1 }, { "sound": 2 }]);
    assert!(serde_json::from_value::<CollectionDefinition>(value.clone()).is_err());

    value["sounds_type"] = json!(["Bogus", "Default"]);
    let err = serde_json::from_value::<CollectionDefinition>(value)
        .unwrap_err()
        .to_string();
    assert!(err.contains("Bogus"), "{err}");
}

#[test]
fn collection_without_sounds_emits_no_union_keys() {
    let value = serde_json::to_value(collection_with(None)).unwrap();
    let object = value.as_object().unwrap();
    assert!(!object.contains_key("sounds"));
    assert!(!object.contains_key("sounds_type"));
}
