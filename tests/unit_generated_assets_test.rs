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

//! Tests for build-time generated asset types from SDK FlatBuffer schemas.
//!
//! Verifies that generated types can serialize/deserialize correctly (roundtrip)
//! and that their JSON output shape matches the existing hand-written types
//! for types that overlap (Sound, Spatialization, Scope, etc.).

use am::assets::generated;

// =============================================================================
// Enum roundtrip tests
// =============================================================================

#[test]
fn test_generated_spatialization_serde_roundtrip() {
    for variant in [
        generated::Spatialization::None,
        generated::Spatialization::Position,
        generated::Spatialization::PositionOrientation,
        generated::Spatialization::HRTF,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let parsed: generated::Spatialization = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, variant);
    }
}

#[test]
fn test_generated_scope_serde_roundtrip() {
    for variant in [generated::Scope::World, generated::Scope::Entity] {
        let json = serde_json::to_string(&variant).unwrap();
        let parsed: generated::Scope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, variant);
    }
}

#[test]
fn test_generated_value_kind_serde_roundtrip() {
    for variant in [
        generated::ValueKind::None,
        generated::ValueKind::Static,
        generated::ValueKind::RTPC,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let parsed: generated::ValueKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, variant);
    }
}

#[test]
fn test_generated_panning_mode_serde_roundtrip() {
    for variant in [
        generated::PanningMode::Stereo,
        generated::PanningMode::BinauralLowQuality,
        generated::PanningMode::BinauralMediumQuality,
        generated::PanningMode::BinauralHighQuality,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let parsed: generated::PanningMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, variant);
    }
}

// =============================================================================
// Enum default tests
// =============================================================================

#[test]
fn test_generated_spatialization_default() {
    assert_eq!(
        generated::Spatialization::default(),
        generated::Spatialization::None
    );
}

#[test]
fn test_generated_scope_default() {
    assert_eq!(generated::Scope::default(), generated::Scope::World);
}

#[test]
fn test_generated_value_kind_default() {
    assert_eq!(generated::ValueKind::default(), generated::ValueKind::None);
}

// =============================================================================
// Enum serde rename tests (SDK format compatibility)
// =============================================================================

#[test]
fn test_generated_spatialization_hrtf_serializes_as_hrtf() {
    assert_eq!(
        serde_json::to_string(&generated::Spatialization::HRTF).unwrap(),
        "\"HRTF\""
    );
}

#[test]
fn test_generated_value_kind_rtpc_serializes_as_rtpc() {
    assert_eq!(
        serde_json::to_string(&generated::ValueKind::RTPC).unwrap(),
        "\"RTPC\""
    );
}

// =============================================================================
// Struct roundtrip tests
// =============================================================================

#[test]
fn test_generated_sound_loop_config_roundtrip() {
    let config = generated::SoundLoopConfig {
        enabled: true,
        loop_count: 5,
    };
    let json = serde_json::to_string(&config).unwrap();
    let parsed: generated::SoundLoopConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.enabled, config.enabled);
    assert_eq!(parsed.loop_count, config.loop_count);
}

#[test]
fn test_generated_rtpc_compatible_value_roundtrip() {
    let value = generated::RtpcCompatibleValue {
        kind: generated::ValueKind::Static,
        value: 0.75,
        rtpc: None,
    };
    let json = serde_json::to_string(&value).unwrap();
    let parsed: generated::RtpcCompatibleValue = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.kind, generated::ValueKind::Static);
    assert_eq!(parsed.value, 0.75);
    assert!(parsed.rtpc.is_none());
}

#[test]
fn test_generated_rtpc_compatible_value_with_rtpc_roundtrip() {
    let value = generated::RtpcCompatibleValue {
        kind: generated::ValueKind::RTPC,
        value: 0.0,
        rtpc: Some(generated::RtpcParameter {
            id: 42,
            curve: Some(generated::CurveDefinition {
                parts: Some(vec![generated::CurvePartDefinition {
                    start: Some(generated::CurvePointDefinition { x: 0.0, y: 0.0 }),
                    end: Some(generated::CurvePointDefinition { x: 100.0, y: 1.0 }),
                    fader: Some("Linear".to_string()),
                }]),
            }),
        }),
    };
    let json = serde_json::to_string(&value).unwrap();
    let parsed: generated::RtpcCompatibleValue = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.kind, generated::ValueKind::RTPC);
    assert!(parsed.rtpc.is_some());
    let rtpc = parsed.rtpc.unwrap();
    assert_eq!(rtpc.id, 42);
}

#[test]
fn test_generated_sound_definition_roundtrip() {
    let sound = generated::SoundDefinition {
        id: 12345,
        name: Some("explosion".to_string()),
        path: Some("sfx/explosion_01.wav".to_string()),
        bus: 0,
        gain: Some(generated::RtpcCompatibleValue {
            kind: generated::ValueKind::Static,
            value: 0.8,
            rtpc: None,
        }),
        priority: Some(generated::RtpcCompatibleValue {
            kind: generated::ValueKind::Static,
            value: 128.0,
            rtpc: None,
        }),
        stream: false,
        loop_: Some(generated::SoundLoopConfig {
            enabled: false,
            loop_count: 0,
        }),
        spatialization: generated::Spatialization::Position,
        attenuation: 0,
        scope: generated::Scope::World,
        fader: Some("Linear".to_string()),
        effect: 0,
        near_field_gain: None,
        pitch: None,
    };

    let json = serde_json::to_string(&sound).unwrap();
    let parsed: generated::SoundDefinition = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.id, 12345);
    assert_eq!(parsed.name.as_deref(), Some("explosion"));
    assert_eq!(parsed.path.as_deref(), Some("sfx/explosion_01.wav"));
    assert_eq!(parsed.bus, 0);
    assert!(!parsed.stream);
    assert_eq!(parsed.spatialization, generated::Spatialization::Position);
    assert_eq!(parsed.scope, generated::Scope::World);
}

#[test]
fn test_generated_sound_definition_loop_field_renamed() {
    // Verify that the "loop" field is properly renamed in JSON output
    let sound = generated::SoundDefinition {
        id: 1,
        name: Some("test".to_string()),
        path: Some("test.wav".to_string()),
        bus: 0,
        gain: None,
        priority: None,
        stream: false,
        loop_: Some(generated::SoundLoopConfig {
            enabled: true,
            loop_count: 3,
        }),
        spatialization: generated::Spatialization::None,
        attenuation: 0,
        scope: generated::Scope::World,
        fader: None,
        effect: 0,
        near_field_gain: None,
        pitch: None,
    };

    let json_value: serde_json::Value = serde_json::to_value(&sound).unwrap();
    let obj = json_value.as_object().unwrap();
    // Must use "loop" (not "loop_") as the JSON key
    assert!(obj.contains_key("loop"), "Expected 'loop' key in JSON");
    assert!(
        !obj.contains_key("loop_"),
        "Should not have 'loop_' key in JSON"
    );
    // Verify the loop value roundtrips correctly
    let loop_val = &obj["loop"];
    assert_eq!(loop_val["enabled"], true);
    assert_eq!(loop_val["loop_count"], 3);
}

// Cross-comparison tests removed — re-exported types are now the same as generated types
// (Story 2c.2: Sound & Shared Types Migration)

// =============================================================================
// Default value tests
// =============================================================================

#[test]
fn test_generated_rtpc_compatible_value_default_kind() {
    // ValueKind default in schema is Static (value 1), not None (value 0)
    let json = r#"{"value": 0.5}"#;
    let parsed: generated::RtpcCompatibleValue = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.kind, generated::ValueKind::Static);
}

#[test]
fn test_generated_event_action_active_default() {
    // EventActionDefinition.active defaults to true
    let json = r#"{"type": "Play", "scope": "World"}"#;
    let parsed: generated::EventActionDefinition = serde_json::from_str(json).unwrap();
    assert!(parsed.active);
}

// =============================================================================
// Collection and other asset types roundtrip
// =============================================================================

#[test]
fn test_generated_collection_definition_roundtrip() {
    let collection = generated::CollectionDefinition {
        id: 100,
        name: Some("ambient_sounds".to_string()),
        bus: 1,
        gain: None,
        priority: None,
        pitch: None,
        scope: generated::Scope::World,
        spatialization: generated::Spatialization::None,
        attenuation: 0,
        effect: 0,
        fader: None,
        play_mode: generated::CollectionPlayMode::PlayOne,
        scheduler: None,
        sounds: Some(Vec::new()),
    };

    let json = serde_json::to_string(&collection).unwrap();
    let parsed: generated::CollectionDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, 100);
    assert_eq!(parsed.name.as_deref(), Some("ambient_sounds"));
    assert_eq!(parsed.play_mode, generated::CollectionPlayMode::PlayOne);
}

#[test]
fn test_generated_switch_definition_roundtrip() {
    let switch = generated::SwitchDefinition {
        id: 200,
        name: Some("surface_material".to_string()),
        states: Some(vec![
            generated::SwitchStateDefinition {
                id: 1,
                name: Some("concrete".to_string()),
            },
            generated::SwitchStateDefinition {
                id: 2,
                name: Some("grass".to_string()),
            },
        ]),
    };

    let json = serde_json::to_string(&switch).unwrap();
    let parsed: generated::SwitchDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, 200);
    assert_eq!(parsed.states.as_ref().unwrap().len(), 2);
}

#[test]
fn test_generated_event_definition_roundtrip() {
    let event = generated::EventDefinition {
        id: 300,
        name: Some("play_explosion".to_string()),
        run_mode: generated::EventActionRunningMode::Parallel,
        actions: Some(vec![generated::EventActionDefinition {
            type_: generated::EventActionType::Play,
            active: true,
            scope: generated::Scope::Entity,
            targets: Some(vec![12345]),
        }]),
    };

    let json = serde_json::to_string(&event).unwrap();
    let parsed: generated::EventDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, 300);
    assert_eq!(parsed.actions.as_ref().unwrap().len(), 1);
    assert_eq!(
        parsed.actions.unwrap()[0].type_,
        generated::EventActionType::Play
    );
}
