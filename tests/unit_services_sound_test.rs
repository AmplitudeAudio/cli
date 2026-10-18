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

//! Tests for the sound service (value building and patching, no I/O front end).

mod common;

use am::assets::generated::{Scope, Spatialization};
use am::assets::{ProjectContext, RtpcCompatibleValue, Sound, SoundLoopConfig};
use am::common::errors::{as_cli_error, codes};
use am::services::sound::{SoundPatch, SoundValues, apply_patch, build_sound};
use common::fixtures::AssetTestFixture;

fn context(fx: &AssetTestFixture) -> ProjectContext {
    let validator = fx.create_project_validator().unwrap();
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator)
}

fn values(bus: u64) -> SoundValues {
    SoundValues {
        name: "boom".to_string(),
        audio_file: "a.wav".to_string(),
        bus,
        attenuation: 0,
        effect: 0,
        scope: Scope::World,
        fader: "Linear".to_string(),
        gain: RtpcCompatibleValue::static_value(1.0),
        pitch: None,
        near_field_gain: None,
        priority: RtpcCompatibleValue::static_value(128.0),
        spread: None,
        stream: false,
        loop_config: SoundLoopConfig::disabled(),
        spatialization: Spatialization::None,
    }
}

#[test]
fn test_build_sound_rejects_bus_zero() {
    let fx = AssetTestFixture::new("svc_zero").unwrap();
    fx.create_data_file("a.wav").unwrap();
    let err = build_sound(values(0), &context(&fx)).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_EMPTY_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("bus"));
}

#[test]
fn test_build_sound_rejects_bus_zero_without_validator() {
    let fx = AssetTestFixture::new("svc_zero_nov").unwrap();
    fx.create_data_file("a.wav").unwrap();
    let ctx = ProjectContext::new(fx.project_root().to_path_buf());
    assert!(build_sound(values(0), &ctx).is_err());
}

#[test]
fn test_build_sound_rejects_dangling_bus() {
    let fx = AssetTestFixture::new("svc_dangling").unwrap();
    fx.create_data_file("a.wav").unwrap();
    let err = build_sound(values(999), &context(&fx)).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
}

#[test]
fn test_build_sound_carries_every_value() {
    let fx = AssetTestFixture::new("svc_full").unwrap();
    fx.create_data_file("a.wav").unwrap();
    let v = SoundValues {
        scope: Scope::Entity,
        fader: "EaseIn".to_string(),
        pitch: Some(RtpcCompatibleValue::static_value(1.2)),
        near_field_gain: Some(RtpcCompatibleValue::static_value(0.5)),
        spread: Some(RtpcCompatibleValue::static_value(90.0)),
        stream: true,
        loop_config: SoundLoopConfig::count(3),
        spatialization: Spatialization::Position,
        ..values(2)
    };
    let sound = build_sound(v, &context(&fx)).unwrap();
    assert_ne!(sound.id, 0);
    assert_eq!(sound.bus, 2);
    assert_eq!(sound.scope, Scope::Entity);
    assert_eq!(sound.fader.as_deref(), Some("EaseIn"));
    assert_eq!(sound.pitch.as_ref().and_then(|p| p.as_static()), Some(1.2));
    assert_eq!(
        sound.near_field_gain.as_ref().and_then(|p| p.as_static()),
        Some(0.5)
    );
    assert_eq!(
        sound.spread.as_ref().and_then(|p| p.as_static()),
        Some(90.0)
    );
    assert!(sound.stream);
    assert_eq!(sound.loop_, Some(SoundLoopConfig::count(3)));
    assert_eq!(sound.spatialization, Spatialization::Position);
    assert_eq!(sound.path.as_deref(), Some("a.wav"));
}

#[test]
fn test_apply_patch_changes_only_provided_fields_and_does_not_mutate() {
    let original = Sound::builder(7, "boom")
        .path("a.wav")
        .bus(2)
        .gain(0.4)
        .priority(10)
        .build();
    let patch = SoundPatch {
        effect: Some(9),
        scope: Some(Scope::Entity),
        ..SoundPatch::default()
    };
    let updated = apply_patch(&original, &patch);
    assert_eq!(updated.effect, 9);
    assert_eq!(updated.scope, Scope::Entity);
    assert_eq!(updated.gain, original.gain);
    assert_eq!(updated.bus, 2);
    assert_eq!(updated.id, 7);
    // the input is untouched
    assert_eq!(original.effect, 0);
    assert_eq!(original.scope, Scope::World);
    assert_eq!(patch.changed_fields(), vec!["effect", "scope"]);
}

#[test]
fn test_validation_error_is_a_cli_error() {
    let fx = AssetTestFixture::new("svc_cli").unwrap();
    fx.create_data_file("a.wav").unwrap();
    let err = build_sound(values(0), &context(&fx)).unwrap_err();
    let any: anyhow::Error = err.into();
    assert!(as_cli_error(&any).is_some());
}
