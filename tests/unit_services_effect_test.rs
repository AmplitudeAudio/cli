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

//! Tests for the effect service (value building, patching) and the filter-name table.

mod common;

use am::assets::extensions::EFFECT_FILTER_NAMES;
use am::assets::generated::CurveDefinition;
use am::assets::{Effect, ProjectContext, RtpcCompatibleValue, ValidationError};
use am::common::errors::codes;
use am::services::effect::{
    EffectPatch, EffectValues, ParamsPatch, apply_params, apply_patch, build_effect, filter_warning,
};
use common::effect_helpers::fixture;
use common::fixtures::AssetTestFixture;

fn context(fx: &AssetTestFixture) -> ProjectContext {
    let validator = fx.create_project_validator().unwrap();
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator)
}

fn st(v: f32) -> RtpcCompatibleValue {
    RtpcCompatibleValue::static_value(v)
}

fn bound(id: u64) -> RtpcCompatibleValue {
    RtpcCompatibleValue::rtpc(id, CurveDefinition { parts: Some(Vec::new()) })
}

fn patch(effect: Option<&str>, params: Vec<RtpcCompatibleValue>) -> EffectPatch {
    EffectPatch {
        effect: effect.map(str::to_string),
        parameters: ParamsPatch {
            replace: Some(params),
            ..Default::default()
        },
    }
}

fn build(fx: &AssetTestFixture, p: &EffectPatch) -> Result<Effect, ValidationError> {
    build_effect(EffectValues::from_patch("fx", p)?, &context(fx))
}

#[test]
fn test_sdk_registered_filters_are_listed() {
    // `Engine::RegisterDefaultExtensions` registers exactly these filters.
    for name in [
        "BassBoost",
        "BiquadResonant",
        "CompressorFilter",
        "DCRemoval",
        "Delay",
        "Equalizer",
        "Flanger",
        "Lofi",
        "MonoPole",
        "PitchShift",
        "Robotize",
        "WaveShaper",
    ] {
        assert!(EFFECT_FILTER_NAMES.contains(&name), "{name} is missing");
    }
    assert_eq!(EFFECT_FILTER_NAMES.len(), 12);
}

#[test]
fn test_filter_warning_is_silent_for_registered_filters() {
    assert_eq!(filter_warning("Delay"), None);
}

#[test]
fn test_filter_warning_names_the_filter_and_lists_the_registered_ones() {
    let warning = filter_warning("reverb").unwrap();
    assert!(warning.contains("reverb"));
    assert!(warning.contains("Delay"));
    assert!(warning.to_lowercase().contains("plugin"));
}

#[test]
fn test_filter_warning_suggests_the_right_case() {
    let warning = filter_warning("delay").unwrap();
    assert!(warning.contains("Did you mean 'Delay'"), "{warning}");
}

#[test]
fn test_build_requires_an_effect_type() {
    let fx = fixture("svc_fx_req");
    let err = EffectValues::from_patch("fx", &patch(None, vec![])).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_FIELD);
    assert_eq!(err.field.as_deref(), Some("effect"));
    drop(fx);
}

#[test]
fn test_build_generates_an_id_and_keeps_the_values() {
    let fx = fixture("svc_fx_build");
    let effect = build(&fx, &patch(Some("Delay"), vec![st(0.5), bound(401), st(2.0)])).unwrap();
    assert_ne!(effect.id, 0);
    assert_eq!(effect.name.as_deref(), Some("fx"));
    assert_eq!(effect.effect.as_deref(), Some("Delay"));
    let params = effect.parameters.unwrap();
    assert_eq!(params.len(), 3);
    assert_eq!(params[0].as_static(), Some(0.5));
    assert_eq!(params[1].rtpc.as_ref().map(|r| r.id), Some(401));
}

#[test]
fn test_build_without_parameters_is_valid() {
    let fx = fixture("svc_fx_noparams");
    let effect = build(&fx, &patch(Some("Lofi"), vec![])).unwrap();
    assert_eq!(effect.parameters.unwrap_or_default().len(), 0);
}

#[test]
fn test_build_rejects_a_dangling_rtpc_binding() {
    let fx = fixture("svc_fx_dangling");
    let err = build(&fx, &patch(Some("Delay"), vec![bound(999)])).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("parameters"));
}

#[test]
fn test_build_rejects_an_rtpc_binding_with_id_zero() {
    let fx = fixture("svc_fx_zero");
    let err = build(&fx, &patch(Some("Delay"), vec![bound(0)])).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_EMPTY_REFERENCE);
}

#[test]
fn test_build_rejects_an_rtpc_value_without_its_rtpc() {
    let fx = fixture("svc_fx_nortpc");
    let mut value = bound(401);
    value.rtpc = None;
    let err = build(&fx, &patch(Some("Delay"), vec![value])).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("parameters"));
}

#[test]
fn test_build_rejects_non_finite_static_parameters() {
    let fx = fixture("svc_fx_nan");
    let err = build(&fx, &patch(Some("Delay"), vec![st(f32::NAN)])).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("parameters"));
}

#[test]
fn test_build_accepts_unknown_filter_names_because_plugins_register_filters() {
    let fx = fixture("svc_fx_plugin");
    let effect = build(&fx, &patch(Some("MyPluginReverb"), vec![])).unwrap();
    assert_eq!(effect.effect.as_deref(), Some("MyPluginReverb"));
}

#[test]
fn test_build_rejects_a_whitespace_effect_type() {
    let fx = fixture("svc_fx_blank");
    let err = build(&fx, &patch(Some("   "), vec![])).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("effect"));
}

#[test]
fn test_apply_params_orders_replace_bind_remove_add() {
    let existing = vec![st(1.0), st(2.0), st(3.0)];
    let p = ParamsPatch {
        replace: Some(vec![st(10.0), st(20.0), st(30.0)]),
        bind: vec![(1, bound(401))],
        remove: vec![0],
        add: vec![st(99.0)],
    };
    let out = apply_params(&existing, &p).unwrap();
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].rtpc.as_ref().map(|r| r.id), Some(401));
    assert_eq!(out[1].as_static(), Some(30.0));
    assert_eq!(out[2].as_static(), Some(99.0));
    assert_eq!(existing.len(), 3, "the input is not modified");
}

#[test]
fn test_apply_params_bind_may_append_right_after_the_end() {
    let out = apply_params(&[st(1.0)], &ParamsPatch { bind: vec![(1, bound(402))], ..Default::default() }).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[1].rtpc.as_ref().map(|r| r.id), Some(402));
}

#[test]
fn test_apply_params_bind_beyond_the_end_fails() {
    let err = apply_params(&[st(1.0)], &ParamsPatch { bind: vec![(3, bound(402))], ..Default::default() }).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_FIELD);
    assert_eq!(err.field.as_deref(), Some("parameters"));
}

#[test]
fn test_apply_params_remove_out_of_range_fails() {
    let err = apply_params(&[st(1.0)], &ParamsPatch { remove: vec![1], ..Default::default() }).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_FIELD);
}

#[test]
fn test_apply_params_empty_replace_clears() {
    let out = apply_params(&[st(1.0)], &ParamsPatch { replace: Some(vec![]), ..Default::default() }).unwrap();
    assert!(out.is_empty());
}

#[test]
fn test_apply_patch_keeps_unset_fields_and_the_original() {
    let existing = Effect::builder(7, "fx")
        .effect_type("Delay")
        .parameters(vec![st(1.0), st(2.0)])
        .build();
    let only_type = EffectPatch {
        effect: Some("Lofi".to_string()),
        ..Default::default()
    };
    let out = apply_patch(&existing, &only_type).unwrap();
    assert_eq!(out.effect.as_deref(), Some("Lofi"));
    assert_eq!(out.parameters, existing.parameters);
    assert_eq!(existing.effect.as_deref(), Some("Delay"));

    let only_add = EffectPatch {
        parameters: ParamsPatch { add: vec![st(3.0)], ..Default::default() },
        ..Default::default()
    };
    let out = apply_patch(&existing, &only_add).unwrap();
    assert_eq!(out.effect, existing.effect);
    assert_eq!(out.parameters.unwrap().len(), 3);
}

#[test]
fn test_patch_reports_the_fields_it_sets() {
    assert!(EffectPatch::default().is_empty());
    let p = EffectPatch {
        effect: Some("Lofi".to_string()),
        parameters: ParamsPatch { remove: vec![0], ..Default::default() },
    };
    assert_eq!(p.changed_fields(), vec!["effect_type", "parameters"]);
}
