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

//! Tests for the curve and zone input helpers.

mod common;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use am::assets::generated::{AttenuationShapeDefinition, ZoneDefinition};
use am::forms::curve::{curve_from_flags, parse_curve_part, prompt_curve};
use am::forms::zone::{ZoneArgs, prompt_zone, zone_from_flags};
use common::scripted_input::ScriptedInput;

fn box_args() -> ZoneArgs {
    ZoneArgs {
        shape: Some("box".to_string()),
        inner_half_width: Some(1.0),
        inner_half_height: Some(2.0),
        inner_half_depth: Some(3.0),
        outer_half_width: Some(2.0),
        outer_half_height: Some(4.0),
        outer_half_depth: Some(6.0),
        ..ZoneArgs::default()
    }
}

#[test]
fn parse_curve_part_reads_five_fields() {
    let part = parse_curve_part("0,0,1,1,linear").unwrap();
    let start = part.start.unwrap();
    let end = part.end.unwrap();
    assert_eq!((start.x, start.y, end.x, end.y), (0.0, 0.0, 1.0, 1.0));
    assert_eq!(part.fader.as_deref(), Some("Linear"));
}

#[test]
fn parse_curve_part_rejects_malformed_input_naming_format() {
    for bad in ["0,0,1", "a,0,1,1,linear", "0,0,1,1,wobble", "0,0,NaN,1,linear"] {
        let err = parse_curve_part(bad).unwrap_err().to_string();
        assert!(
            err.contains("start_x,start_y,end_x,end_y,fader") || err.contains("fader"),
            "{bad}: {err}"
        );
    }
    let err = parse_curve_part("0,0,1").unwrap_err().to_string();
    assert!(err.contains("start_x,start_y,end_x,end_y,fader"), "{err}");
    let err = parse_curve_part("0,0,1,1,wobble").unwrap_err().to_string();
    assert!(err.contains("wobble") && err.contains("SCurveSmooth"), "{err}");
}

#[test]
fn parse_curve_part_rejects_non_increasing_x() {
    assert!(parse_curve_part("1,0,1,1,linear").is_err());
    assert!(parse_curve_part("2,0,1,1,linear").is_err());
}

#[test]
fn curve_from_flags_neither_is_none_and_both_is_error() {
    assert!(curve_from_flags(&[], None).unwrap().is_none());
    let both = curve_from_flags(&["0,0,1,1,linear".to_string()], Some(&PathBuf::from("x.json")));
    assert!(both.is_err());
}

#[test]
fn curve_from_flags_collects_parts_in_order() {
    let curve = curve_from_flags(
        &["0,0,1,1,linear".to_string(), "1,1,2,0,EaseIn".to_string()],
        None,
    )
    .unwrap()
    .unwrap();
    let parts = curve.parts.unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[1].fader.as_deref(), Some("EaseIn"));
}

#[test]
fn curve_from_flags_accepts_gaps_and_out_of_order_parts() {
    let gap = curve_from_flags(
        &["0,0,1,1,linear".to_string(), "2,1,3,0,linear".to_string()],
        None,
    );
    assert!(gap.unwrap().is_some());
    let unordered = curve_from_flags(
        &["2,1,3,0,linear".to_string(), "0,0,1,1,linear".to_string()],
        None,
    );
    assert!(unordered.unwrap().is_some());
}

fn json_curve_error(body: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("curve.json");
    fs::write(&path, body).unwrap();
    let err = curve_from_flags(&[], Some(&path)).unwrap_err().to_string();
    err
}

#[test]
fn json_curve_requires_a_valid_fader_on_every_part() {
    let missing = json_curve_error(
        r#"{"parts":[{"start":{"x":0.0,"y":0.0},"end":{"x":1.0,"y":1.0},"fader":"Linear"},{"start":{"x":1.0,"y":0.0},"end":{"x":2.0,"y":1.0}}]}"#,
    );
    assert!(missing.contains("part 2") && missing.contains("fader"), "{missing}");
    assert!(missing.contains("SCurveSharp"), "{missing}");
    let unknown = json_curve_error(
        r#"{"parts":[{"start":{"x":0.0,"y":0.0},"end":{"x":1.0,"y":1.0},"fader":"Wobble"}]}"#,
    );
    assert!(unknown.contains("part 1") && unknown.contains("Wobble"), "{unknown}");
    assert!(unknown.contains("EaseInOut"), "{unknown}");
}

#[test]
fn json_curve_errors_include_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.json");
    let err = format!("{:?}", curve_from_flags(&[], Some(&missing)).unwrap_err());
    assert!(err.contains("nope.json"), "{err}");
    let bad = dir.path().join("bad.json");
    fs::write(&bad, "{not json").unwrap();
    let err = format!("{:?}", curve_from_flags(&[], Some(&bad)).unwrap_err());
    assert!(err.contains("bad.json"), "{err}");
}

#[test]
fn prompt_curve_rejects_a_bad_part_immediately() {
    // start_x 2, end_x 1: fails before any confirm is asked.
    let input = ScriptedInput::new(&["Linear"], &["2", "0", "1", "1"]);
    assert!(prompt_curve(&input).is_err());
}

#[test]
fn curve_from_flags_reads_json_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("curve.json");
    fs::write(
        &path,
        r#"{"parts":[{"start":{"x":0.0,"y":0.0},"end":{"x":1.0,"y":1.0},"fader":"Linear"}]}"#,
    )
    .unwrap();
    let curve = curve_from_flags(&[], Some(&path)).unwrap().unwrap();
    assert_eq!(curve.parts.unwrap().len(), 1);
}

#[test]
fn curve_from_flags_rejects_empty_json_curve() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("curve.json");
    fs::write(&path, r#"{"parts":[]}"#).unwrap();
    assert!(curve_from_flags(&[], Some(&path)).is_err());
    assert!(curve_from_flags(&[], Some(&dir.path().join("missing.json"))).is_err());
}

#[test]
fn prompt_curve_collects_two_parts() {
    let input = ScriptedInput::new(
        &["Linear", "EaseIn"],
        &["0", "0", "1", "1", "1", "1", "2", "0"],
    )
    .with_confirms(&[true, false]);
    let curve = prompt_curve(&input).unwrap();
    let parts = curve.parts.unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[1].end.as_ref().unwrap().x, 2.0);
    assert_eq!(parts[1].fader.as_deref(), Some("EaseIn"));
    input.assert_all_consumed();
}

#[test]
fn zone_from_flags_builds_box_zone() {
    let zone = zone_from_flags(&box_args()).unwrap();
    match zone {
        ZoneDefinition::Box(settings) => {
            let inner = settings.inner.unwrap();
            assert_eq!(
                (inner.half_width, inner.half_height, inner.half_depth),
                (1.0, 2.0, 3.0)
            );
            assert!(settings.outer.is_some());
        }
        other => panic!("expected box, got {other:?}"),
    }
}

#[test]
fn zone_from_flags_rejects_flags_for_another_shape() {
    let args = ZoneArgs {
        shape: Some("sphere".to_string()),
        ..box_args()
    };
    let err = zone_from_flags(&args).unwrap_err().to_string();
    assert!(err.contains("sphere") && err.contains("box"), "{err}");
}

#[test]
fn zone_from_flags_requires_both_sides() {
    let only_inner = ZoneArgs {
        outer_half_width: None,
        outer_half_height: None,
        outer_half_depth: None,
        ..box_args()
    };
    let err = zone_from_flags(&only_inner).unwrap_err().to_string();
    assert!(err.contains("--outer-half-width") && err.contains("--outer-half-depth"), "{err}");
    let only_outer = ZoneArgs {
        inner_half_width: None,
        inner_half_height: None,
        inner_half_depth: None,
        ..box_args()
    };
    let err = zone_from_flags(&only_outer).unwrap_err().to_string();
    assert!(err.contains("--inner-half-height"), "{err}");
}

#[test]
fn zone_from_flags_rejects_negative_and_non_finite_dimensions() {
    for bad in [-1.0_f32, f32::NAN, f32::INFINITY] {
        let args = ZoneArgs {
            inner_half_width: Some(bad),
            ..box_args()
        };
        let err = zone_from_flags(&args).unwrap_err().to_string();
        assert!(err.contains("--inner-half-width"), "{err}");
    }
}

#[test]
fn zone_from_flags_accepts_zero_except_cone_height() {
    let zero_box = ZoneArgs {
        inner_half_width: Some(0.0),
        inner_half_height: Some(0.0),
        inner_half_depth: Some(0.0),
        ..box_args()
    };
    assert!(zone_from_flags(&zero_box).is_ok());
    let sphere = ZoneArgs {
        shape: Some("sphere".to_string()),
        inner_radius: Some(0.0),
        outer_radius: Some(5.0),
        ..ZoneArgs::default()
    };
    assert!(zone_from_flags(&sphere).is_ok());
    let capsule = ZoneArgs {
        shape: Some("capsule".to_string()),
        inner_radius: Some(0.0),
        inner_half_height: Some(0.0),
        outer_radius: Some(1.0),
        outer_half_height: Some(2.0),
        ..ZoneArgs::default()
    };
    assert!(zone_from_flags(&capsule).is_ok());
    let cone = ZoneArgs {
        shape: Some("cone".to_string()),
        inner_radius: Some(0.0),
        inner_height: Some(1.0),
        outer_radius: Some(2.0),
        outer_height: Some(3.0),
        ..ZoneArgs::default()
    };
    assert!(zone_from_flags(&cone).is_ok());
    for (inner_h, outer_h, flag) in [(0.0, 3.0, "--inner-height"), (1.0, 0.0, "--outer-height")] {
        let bad = ZoneArgs {
            inner_height: Some(inner_h),
            outer_height: Some(outer_h),
            ..cone.clone()
        };
        let err = zone_from_flags(&bad).unwrap_err().to_string();
        assert!(err.contains(flag) && err.contains("greater than 0"), "{err}");
    }
}

#[test]
fn prompt_zone_accepts_zero_but_rejects_negative_and_zero_cone_height() {
    let ok = ScriptedInput::new(&["sphere"], &["0", "3"]);
    assert!(prompt_zone(&ok).is_ok());
    let negative = ScriptedInput::new(&["sphere"], &["-2", "3"]);
    assert!(prompt_zone(&negative).is_err());
    let zero_height = ScriptedInput::new(&["cone"], &["1", "0", "2", "3"]);
    assert!(prompt_zone(&zero_height).is_err());
}

#[test]
fn zone_from_flags_lists_missing_fields() {
    let args = ZoneArgs {
        shape: Some("box".to_string()),
        inner_half_width: Some(1.0),
        ..ZoneArgs::default()
    };
    let err = zone_from_flags(&args).unwrap_err().to_string();
    assert!(
        err.contains("--inner-half-height") && err.contains("--inner-half-depth"),
        "{err}"
    );
}

#[test]
fn zone_from_flags_requires_known_shape_and_some_dimensions() {
    assert!(zone_from_flags(&ZoneArgs::default()).is_err());
    let unknown = ZoneArgs {
        shape: Some("torus".to_string()),
        ..ZoneArgs::default()
    };
    let err = zone_from_flags(&unknown).unwrap_err().to_string();
    assert!(err.contains("torus") && err.contains("capsule") && err.contains("sphere"), "{err}");
    let empty = ZoneArgs {
        shape: Some("sphere".to_string()),
        ..ZoneArgs::default()
    };
    let err = zone_from_flags(&empty).unwrap_err().to_string();
    assert!(err.contains("--inner-radius") || err.contains("--outer-radius"), "{err}");
}

#[test]
fn zone_from_flags_builds_sphere_with_both_sides() {
    let args = ZoneArgs {
        shape: Some("Sphere".to_string()),
        inner_radius: Some(1.0),
        outer_radius: Some(5.0),
        ..ZoneArgs::default()
    };
    match zone_from_flags(&args).unwrap() {
        ZoneDefinition::Sphere(s) => {
            assert_eq!(s.inner.unwrap().radius, 1.0);
            assert_eq!(s.outer.unwrap().radius, 5.0);
        }
        other => panic!("expected sphere, got {other:?}"),
    }
}

#[test]
fn prompt_zone_walks_inner_and_outer_fields() {
    let input = ScriptedInput::new(&["capsule"], &["1", "2", "3", "4"]);
    match prompt_zone(&input).unwrap() {
        ZoneDefinition::Capsule(s) => {
            assert_eq!(s.inner.unwrap().radius, 1.0);
            assert_eq!(s.outer.unwrap().half_height, 4.0);
        }
        other => panic!("expected capsule, got {other:?}"),
    }
    assert!(input.saw_prompt("inner radius"));
    assert!(input.saw_prompt("outer half height"));
    input.assert_all_consumed();
}

#[test]
fn zone_serializes_with_flatc_union_tag_and_is_accepted_by_flatc() {
    let zone = zone_from_flags(&box_args()).unwrap();
    let shape = AttenuationShapeDefinition {
        max_attenuation_factor: 1.0,
        zone: Some(zone),
    };
    let json = serde_json::to_value(&shape).unwrap();
    assert_eq!(json["zone_type"], "Box");
    assert!(json["zone"]["inner"]["half_width"].is_number());

    // Validate with flatc when both flatc and the SDK schemas are available.
    let Ok(sdk) = std::env::var("AM_SDK_PATH") else {
        eprintln!("skipped flatc check: AM_SDK_PATH is not set");
        return;
    };
    let sdk = PathBuf::from(sdk);
    let schema = [sdk.join("schemas"), sdk.join("../schemas")]
        .into_iter()
        .map(|d| d.join("attenuation_definition.fbs"))
        .find(|p| p.exists());
    let Some(schema) = schema else {
        eprintln!("skipped flatc check: attenuation_definition.fbs not found under AM_SDK_PATH");
        return;
    };
    if Command::new("flatc").arg("--version").output().is_err() {
        eprintln!("skipped flatc check: flatc is not installed");
        return;
    }
    let doc = serde_json::json!({
        "id": 1, "name": "a", "max_distance": 10.0,
        "shape": json,
    });
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("att.json");
    fs::write(&file, serde_json::to_string(&doc).unwrap()).unwrap();
    let out = Command::new("flatc")
        .args(["-b", "--strict-json", "-o"])
        .arg(dir.path())
        .arg("-I")
        .arg(schema.parent().unwrap())
        .arg(&schema)
        .arg(&file)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "flatc rejected the JSON: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
