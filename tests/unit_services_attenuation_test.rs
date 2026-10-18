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

//! Tests for the attenuation service and the SDK-mirroring validation rules.

mod common;

use am::assets::generated::{
    AttenuationAirAbsorptionDefinition, CurveDefinition, CurvePartDefinition, CurvePointDefinition,
    SphereShapeDefinition, SphereZoneSettings, ZoneDefinition,
};
use am::assets::{Asset, ProjectContext};
use am::services::attenuation::{
    AirAbsorptionPatch, AttenuationPatch, AttenuationValues, DEFAULT_AIR_ABSORPTION_COEFFICIENTS,
    apply_patch, build_attenuation,
};
use common::fixtures::AssetTestFixture;

fn context(fx: &AssetTestFixture) -> ProjectContext {
    let validator = fx.create_project_validator().unwrap();
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator)
}

fn sphere(inner: f32, outer: f32) -> ZoneDefinition {
    ZoneDefinition::Sphere(SphereZoneSettings {
        inner: Some(SphereShapeDefinition { radius: inner }),
        outer: Some(SphereShapeDefinition { radius: outer }),
    })
}

fn curve() -> CurveDefinition {
    CurveDefinition {
        parts: Some(vec![CurvePartDefinition {
            start: Some(CurvePointDefinition { x: 0.0, y: 1.0 }),
            end: Some(CurvePointDefinition { x: 1.0, y: 0.0 }),
            fader: Some("Linear".to_string()),
        }]),
    }
}

fn values() -> AttenuationValues {
    AttenuationValues {
        name: "short".to_string(),
        max_distance: 50.0,
        zone: sphere(5.0, 10.0),
        max_attenuation_factor: 1.0,
        gain_curve: curve(),
        spread_curve: None,
        air_absorption: None,
    }
}

fn air(
    enabled: bool,
    coefficients: Option<Vec<f32>>,
) -> Option<AttenuationAirAbsorptionDefinition> {
    Some(AttenuationAirAbsorptionDefinition {
        enabled,
        coefficients,
    })
}

#[test]
fn test_build_attenuation_produces_a_complete_valid_asset() {
    let fx = AssetTestFixture::new("att_build").unwrap();
    let att = build_attenuation(values(), &context(&fx)).unwrap();
    assert_ne!(att.id, 0);
    assert_eq!(att.name(), "short");
    assert_eq!(att.max_distance, 50.0);
    assert!(att.shape.as_ref().unwrap().zone.is_some());
    assert!(att.gain_curve.is_some());
    assert!(att.air_absorption.is_none());
}

#[test]
fn test_build_attenuation_rejects_factor_out_of_range() {
    let fx = AssetTestFixture::new("att_factor").unwrap();
    let mut v = values();
    v.max_attenuation_factor = 1.5;
    let err = build_attenuation(v, &context(&fx)).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("max_attenuation_factor"));
}

#[test]
fn test_validate_rejects_missing_shape_zone_and_gain_curve() {
    let fx = AssetTestFixture::new("att_missing").unwrap();
    let ctx = context(&fx);
    let full = build_attenuation(values(), &ctx).unwrap();

    let mut no_shape = full.clone();
    no_shape.shape = None;
    assert_eq!(
        no_shape.validate_rules(&ctx).unwrap_err().field.as_deref(),
        Some("shape")
    );

    let mut no_zone = full.clone();
    no_zone.shape.as_mut().unwrap().zone = None;
    assert_eq!(
        no_zone.validate_rules(&ctx).unwrap_err().field.as_deref(),
        Some("shape")
    );

    let mut no_curve = full;
    no_curve.gain_curve = None;
    assert_eq!(
        no_curve.validate_rules(&ctx).unwrap_err().field.as_deref(),
        Some("gain_curve")
    );
}

#[test]
fn test_validate_rejects_one_sided_zone_and_bad_spread_curve() {
    let fx = AssetTestFixture::new("att_sided").unwrap();
    let ctx = context(&fx);
    let mut att = build_attenuation(values(), &ctx).unwrap();
    att.shape.as_mut().unwrap().zone = Some(ZoneDefinition::Sphere(SphereZoneSettings {
        inner: Some(SphereShapeDefinition { radius: 1.0 }),
        outer: None,
    }));
    assert_eq!(
        att.validate_rules(&ctx).unwrap_err().field.as_deref(),
        Some("shape")
    );

    let mut att = build_attenuation(values(), &ctx).unwrap();
    att.spread_curve = Some(CurveDefinition {
        parts: Some(vec![]),
    });
    assert_eq!(
        att.validate_rules(&ctx).unwrap_err().field.as_deref(),
        Some("spread_curve")
    );
}

#[test]
fn test_validate_enabled_air_absorption_needs_three_coefficients() {
    let fx = AssetTestFixture::new("att_air").unwrap();
    let ctx = context(&fx);
    let base = build_attenuation(values(), &ctx).unwrap();

    // The SDK reads coefficients()->Get(0..2) unconditionally when enabled.
    let mut enabled_without = base.clone();
    enabled_without.air_absorption = air(true, None);
    let err = enabled_without.validate_rules(&ctx).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("air_absorption"));

    let mut two = base.clone();
    two.air_absorption = air(false, Some(vec![0.1, 0.2]));
    assert!(two.validate_rules(&ctx).is_err());

    let mut disabled = base;
    disabled.air_absorption = air(false, None);
    assert!(disabled.validate_rules(&ctx).is_ok());
}

#[test]
fn test_apply_patch_changes_only_provided_fields() {
    let fx = AssetTestFixture::new("att_patch").unwrap();
    let existing = build_attenuation(values(), &context(&fx)).unwrap();

    let patch = AttenuationPatch {
        max_distance: Some(80.0),
        ..Default::default()
    };
    let updated = apply_patch(&existing, &patch);
    assert_eq!(updated.max_distance, 80.0);
    assert_eq!(updated.id, existing.id);
    assert_eq!(updated.shape, existing.shape);
    assert_eq!(updated.gain_curve, existing.gain_curve);
    assert_eq!(updated.air_absorption, existing.air_absorption);
    // The input is untouched.
    assert_eq!(existing.max_distance, 50.0);
}

#[test]
fn test_enabling_air_absorption_fills_sdk_default_coefficients() {
    let fx = AssetTestFixture::new("att_air_defaults").unwrap();
    let existing = build_attenuation(values(), &context(&fx)).unwrap();
    let patch = AttenuationPatch {
        air_absorption: AirAbsorptionPatch {
            enabled: Some(true),
            coefficients: None,
        },
        ..Default::default()
    };
    let updated = apply_patch(&existing, &patch);
    let absorption = updated.air_absorption.unwrap();
    assert!(absorption.enabled);
    assert_eq!(
        absorption.coefficients.unwrap(),
        DEFAULT_AIR_ABSORPTION_COEFFICIENTS.to_vec()
    );
}

#[test]
fn test_factor_patch_keeps_zone() {
    let fx = AssetTestFixture::new("att_factor_patch").unwrap();
    let existing = build_attenuation(values(), &context(&fx)).unwrap();
    let patch = AttenuationPatch {
        max_attenuation_factor: Some(0.25),
        ..Default::default()
    };
    let updated = apply_patch(&existing, &patch);
    let shape = updated.shape.unwrap();
    assert_eq!(shape.max_attenuation_factor, 0.25);
    assert_eq!(shape.zone, existing.shape.unwrap().zone);
}
