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

//! Tests for the RTPC service and the SDK-mirroring validation rules.

mod common;

use am::assets::generated::{FadeTransitionSettings, RtpcFadingSettings};
use am::assets::{Asset, ProjectContext, Rtpc};
use am::services::rtpc::{
    DEFAULT_FADER, FadePatch, FadeTransitionValues, FadeValues, RtpcPatch, RtpcValues, apply_patch,
    build_rtpc,
};
use common::fixtures::AssetTestFixture;

fn context(fx: &AssetTestFixture) -> ProjectContext {
    let validator = fx.create_project_validator().unwrap();
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator)
}

fn values() -> RtpcValues {
    RtpcValues {
        name: "speed".to_string(),
        min_value: 0.0,
        max_value: 10.0,
        default_value: None,
        fade: None,
    }
}

fn transition(duration: f64, fader: &str) -> FadeTransitionValues {
    FadeTransitionValues {
        duration,
        fader: fader.to_string(),
    }
}

fn fade(enabled: bool) -> FadeValues {
    FadeValues {
        enabled,
        attack: transition(0.25, "Ease"),
        release: transition(0.5, "Linear"),
    }
}

fn rtpc_with_fade(settings: RtpcFadingSettings) -> Rtpc {
    Rtpc::builder(1, "speed")
        .min_value(0.0)
        .max_value(10.0)
        .fade_settings(settings)
        .build()
}

fn settings(enabled: bool, attack: Option<(f64, Option<&str>)>) -> RtpcFadingSettings {
    let make = |t: (f64, Option<&str>)| FadeTransitionSettings {
        duration: t.0,
        fader: t.1.map(str::to_string),
    };
    RtpcFadingSettings {
        enabled,
        fade_attack: attack.map(make),
        fade_release: Some(make((0.0, Some("Linear")))),
    }
}

#[test]
fn test_build_rtpc_produces_a_valid_asset() {
    let fx = AssetTestFixture::new("rtpc_build").unwrap();
    let rtpc = build_rtpc(values(), &context(&fx)).unwrap();
    assert_ne!(rtpc.id, 0);
    assert_eq!(rtpc.name(), "speed");
    assert_eq!(rtpc.min_value, 0.0);
    assert_eq!(rtpc.max_value, 10.0);
    assert_eq!(rtpc.default_value, 0.0);
    assert!(rtpc.fade_settings.is_none());
}

#[test]
fn test_unset_default_is_the_schema_default_clamped_into_range() {
    let fx = AssetTestFixture::new("rtpc_default_clamp").unwrap();
    let v = RtpcValues {
        min_value: 5.0,
        max_value: 10.0,
        ..values()
    };
    assert_eq!(build_rtpc(v, &context(&fx)).unwrap().default_value, 5.0);
}

#[test]
fn test_build_rtpc_writes_fade_settings() {
    let fx = AssetTestFixture::new("rtpc_build_fade").unwrap();
    let v = RtpcValues {
        fade: Some(fade(true)),
        ..values()
    };
    let rtpc = build_rtpc(v, &context(&fx)).unwrap();
    let fade = rtpc.fade_settings.unwrap();
    assert!(fade.enabled);
    assert_eq!(fade.fade_attack.unwrap().fader.as_deref(), Some("Ease"));
    assert_eq!(fade.fade_release.unwrap().duration, 0.5);
}

#[test]
fn test_min_above_max_is_rejected_naming_the_field() {
    let fx = AssetTestFixture::new("rtpc_min_max").unwrap();
    let v = RtpcValues {
        min_value: 10.0,
        max_value: 1.0,
        ..values()
    };
    let err = build_rtpc(v, &context(&fx)).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("min_value"));
}

#[test]
fn test_min_equal_to_max_is_rejected_because_the_sdk_divides_by_the_range() {
    let fx = AssetTestFixture::new("rtpc_min_eq_max").unwrap();
    let v = RtpcValues {
        min_value: 3.0,
        max_value: 3.0,
        ..values()
    };
    let err = build_rtpc(v, &context(&fx)).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("min_value"));
}

#[test]
fn test_default_outside_range_is_rejected() {
    let fx = AssetTestFixture::new("rtpc_default_out").unwrap();
    let v = RtpcValues {
        default_value: Some(11.0),
        ..values()
    };
    let err = build_rtpc(v, &context(&fx)).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("default_value"));
}

#[test]
fn test_non_finite_values_are_rejected() {
    let fx = AssetTestFixture::new("rtpc_nan").unwrap();
    let v = RtpcValues {
        max_value: f64::NAN,
        ..values()
    };
    assert!(build_rtpc(v, &context(&fx)).is_err());
}

#[test]
fn test_enabled_fade_without_attack_is_rejected() {
    let fx = AssetTestFixture::new("rtpc_fade_no_attack").unwrap();
    let rtpc = rtpc_with_fade(settings(true, None));
    let err = rtpc.validate_rules(&context(&fx)).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("fade_settings"));
}

#[test]
fn test_enabled_fade_without_fader_is_rejected() {
    let fx = AssetTestFixture::new("rtpc_fade_no_fader").unwrap();
    let rtpc = rtpc_with_fade(settings(true, Some((1.0, None))));
    let err = rtpc.validate_rules(&context(&fx)).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("fade_settings"));
}

#[test]
fn test_unknown_fader_and_negative_duration_are_rejected() {
    let fx = AssetTestFixture::new("rtpc_fade_bad").unwrap();
    let ctx = context(&fx);
    assert!(
        rtpc_with_fade(settings(true, Some((1.0, Some("Bouncy")))))
            .validate_rules(&ctx)
            .is_err()
    );
    assert!(
        rtpc_with_fade(settings(true, Some((-1.0, Some("Linear")))))
            .validate_rules(&ctx)
            .is_err()
    );
}

#[test]
fn test_disabled_fade_without_transitions_is_valid() {
    let fx = AssetTestFixture::new("rtpc_fade_off").unwrap();
    let rtpc = rtpc_with_fade(RtpcFadingSettings {
        enabled: false,
        fade_attack: None,
        fade_release: None,
    });
    rtpc.validate_rules(&context(&fx)).unwrap();
}

#[test]
fn test_apply_patch_changes_only_provided_fields() {
    let fx = AssetTestFixture::new("rtpc_patch").unwrap();
    let existing = build_rtpc(
        RtpcValues {
            fade: Some(fade(true)),
            ..values()
        },
        &context(&fx),
    )
    .unwrap();
    let patch = RtpcPatch {
        max_value: Some(20.0),
        ..RtpcPatch::default()
    };
    let updated = apply_patch(&existing, &patch);
    assert_eq!(updated.max_value, 20.0);
    assert_eq!(updated.min_value, existing.min_value);
    assert_eq!(updated.id, existing.id);
    assert_eq!(updated.fade_settings, existing.fade_settings);
    // The input is untouched.
    assert_eq!(existing.max_value, 10.0);
}

#[test]
fn test_apply_patch_merges_a_single_fade_field() {
    let fx = AssetTestFixture::new("rtpc_patch_fade").unwrap();
    let existing = build_rtpc(
        RtpcValues {
            fade: Some(fade(true)),
            ..values()
        },
        &context(&fx),
    )
    .unwrap();
    let patch = RtpcPatch {
        fade: FadePatch {
            release_fader: Some("EaseIn".to_string()),
            ..FadePatch::default()
        },
        ..RtpcPatch::default()
    };
    let updated = apply_patch(&existing, &patch).fade_settings.unwrap();
    assert!(updated.enabled);
    let release = updated.fade_release.unwrap();
    assert_eq!(release.fader.as_deref(), Some("EaseIn"));
    assert_eq!(release.duration, 0.5);
    assert_eq!(updated.fade_attack.unwrap().fader.as_deref(), Some("Ease"));
}

#[test]
fn test_apply_patch_transition_flag_on_a_bare_rtpc_enables_fading_with_defaults() {
    let existing = Rtpc::builder(1, "speed").max_value(10.0).build();
    let patch = RtpcPatch {
        fade: FadePatch {
            attack_duration: Some(1.5),
            ..FadePatch::default()
        },
        ..RtpcPatch::default()
    };
    let fade = apply_patch(&existing, &patch).fade_settings.unwrap();
    assert!(fade.enabled);
    assert_eq!(fade.fade_attack.as_ref().unwrap().duration, 1.5);
    // Every table the SDK dereferences is present with a registered fader.
    assert_eq!(
        fade.fade_attack.unwrap().fader.as_deref(),
        Some(DEFAULT_FADER)
    );
    assert_eq!(
        fade.fade_release.unwrap().fader.as_deref(),
        Some(DEFAULT_FADER)
    );
}

#[test]
fn test_patch_changed_fields_lists_schema_names() {
    let patch = RtpcPatch {
        min_value: Some(1.0),
        fade: FadePatch {
            enabled: Some(false),
            ..FadePatch::default()
        },
        ..RtpcPatch::default()
    };
    assert_eq!(patch.changed_fields(), vec!["min_value", "fade_settings"]);
    assert!(RtpcPatch::default().is_empty());
}
