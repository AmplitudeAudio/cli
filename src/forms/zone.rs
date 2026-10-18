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

//! Input helpers for attenuation `ZoneDefinition` values: shape-dependent
//! flags (`--shape box --inner-half-width 2 ...`) and interactive entry.
//!
//! Per-shape fields come from the `*ShapeDefinition` structs in
//! `sdk/schemas/common.fbs`; each flag is `--inner-<field>` / `--outer-<field>`
//! with underscores in the field name replaced by dashes.

use anyhow::Result;
use clap::Args;
use inquire::validator::Validation;

use crate::assets::generated::{
    BoxShapeDefinition, BoxZoneSettings, CapsuleShapeDefinition, CapsuleZoneSettings,
    ConeShapeDefinition, ConeZoneSettings, SphereShapeDefinition, SphereZoneSettings,
    ZoneDefinition,
};
use crate::common::errors::{CliError, codes::ERR_VALIDATION_FIELD};
use crate::input::Input;

/// Zone shapes and the schema fields of their shape struct, in schema order.
const SHAPES: &[(&str, &[&str])] = &[
    ("box", &["half_width", "half_height", "half_depth"]),
    ("capsule", &["radius", "half_height"]),
    ("cone", &["radius", "height"]),
    ("sphere", &["radius"]),
];

/// Every distinct shape field across all shapes.
const ALL_FIELDS: &[&str] = &[
    "radius",
    "height",
    "half_width",
    "half_height",
    "half_depth",
];

/// Shape selection plus the shape-specific numeric options, flattened into clap.
#[derive(Args, Debug, Clone, Default)]
pub struct ZoneArgs {
    /// Zone shape: box, capsule, cone or sphere
    #[arg(long)]
    pub shape: Option<String>,
    /// Inner shape radius (capsule, cone, sphere)
    #[arg(long)]
    pub inner_radius: Option<f32>,
    /// Inner shape height (cone)
    #[arg(long)]
    pub inner_height: Option<f32>,
    /// Inner shape half width (box)
    #[arg(long)]
    pub inner_half_width: Option<f32>,
    /// Inner shape half height (box, capsule)
    #[arg(long)]
    pub inner_half_height: Option<f32>,
    /// Inner shape half depth (box)
    #[arg(long)]
    pub inner_half_depth: Option<f32>,
    /// Outer shape radius (capsule, cone, sphere)
    #[arg(long)]
    pub outer_radius: Option<f32>,
    /// Outer shape height (cone)
    #[arg(long)]
    pub outer_height: Option<f32>,
    /// Outer shape half width (box)
    #[arg(long)]
    pub outer_half_width: Option<f32>,
    /// Outer shape half height (box, capsule)
    #[arg(long)]
    pub outer_half_height: Option<f32>,
    /// Outer shape half depth (box)
    #[arg(long)]
    pub outer_half_depth: Option<f32>,
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Inner,
    Outer,
}

impl Side {
    const BOTH: [Side; 2] = [Side::Inner, Side::Outer];

    fn prefix(self) -> &'static str {
        match self {
            Side::Inner => "inner",
            Side::Outer => "outer",
        }
    }
}

fn flag(side: Side, field: &str) -> String {
    format!("--{}-{}", side.prefix(), field.replace('_', "-"))
}

impl ZoneArgs {
    fn get(&self, side: Side, field: &str) -> Option<f32> {
        match (side, field) {
            (Side::Inner, "radius") => self.inner_radius,
            (Side::Inner, "height") => self.inner_height,
            (Side::Inner, "half_width") => self.inner_half_width,
            (Side::Inner, "half_height") => self.inner_half_height,
            (Side::Inner, "half_depth") => self.inner_half_depth,
            (Side::Outer, "radius") => self.outer_radius,
            (Side::Outer, "height") => self.outer_height,
            (Side::Outer, "half_width") => self.outer_half_width,
            (Side::Outer, "half_height") => self.outer_half_height,
            (Side::Outer, "half_depth") => self.outer_half_depth,
            _ => None,
        }
    }
}

fn error(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(ERR_VALIDATION_FIELD, what, why)
}

fn shape_names() -> Vec<String> {
    SHAPES.iter().map(|(n, _)| n.to_string()).collect()
}

fn find_shape(name: &str) -> Result<(&'static str, &'static [&'static str])> {
    SHAPES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name.trim()))
        .copied()
        .ok_or_else(|| {
            error(
                format!("Unknown zone shape '{name}'"),
                format!("shape must be one of: {}", shape_names().join(", ")),
            )
            .with_suggestion("Use --shape box, capsule, cone or sphere")
            .into()
        })
}

/// Values for one side of a zone, keyed by schema field name.
type Values = Vec<(&'static str, f32)>;

fn value_of(values: &Values, field: &str) -> f32 {
    values
        .iter()
        .find(|(f, _)| *f == field)
        .map_or(0.0, |(_, v)| *v)
}

fn build_zone(shape: &str, inner: Values, outer: Values) -> ZoneDefinition {
    macro_rules! side {
        ($side:expr, $ty:ident { $($field:ident),+ }) => {
            Some($ty { $($field: value_of(&$side, stringify!($field))),+ })
        };
    }
    match shape {
        "box" => ZoneDefinition::Box(BoxZoneSettings {
            inner: side!(
                inner,
                BoxShapeDefinition {
                    half_width,
                    half_height,
                    half_depth
                }
            ),
            outer: side!(
                outer,
                BoxShapeDefinition {
                    half_width,
                    half_height,
                    half_depth
                }
            ),
        }),
        "capsule" => ZoneDefinition::Capsule(CapsuleZoneSettings {
            inner: side!(
                inner,
                CapsuleShapeDefinition {
                    radius,
                    half_height
                }
            ),
            outer: side!(
                outer,
                CapsuleShapeDefinition {
                    radius,
                    half_height
                }
            ),
        }),
        "cone" => ZoneDefinition::Cone(ConeZoneSettings {
            inner: side!(inner, ConeShapeDefinition { radius, height }),
            outer: side!(outer, ConeShapeDefinition { radius, height }),
        }),
        _ => ZoneDefinition::Sphere(SphereZoneSettings {
            inner: side!(inner, SphereShapeDefinition { radius }),
            outer: side!(outer, SphereShapeDefinition { radius }),
        }),
    }
}

/// Dimensions must be finite and not negative. Zero is valid (degenerate
/// shapes are handled by the SDK) except for cone height, which the SDK
/// divides by.
fn check_dimension(name: &str, shape: &str, field: &str, value: f32) -> Result<()> {
    let needs_positive = shape == "cone" && field == "height";
    let ok = value.is_finite() && (value > 0.0 || (!needs_positive && value == 0.0));
    if ok {
        return Ok(());
    }
    let rule = if needs_positive {
        "greater than 0 (the SDK divides by a cone's height)"
    } else {
        "finite and not negative"
    };
    Err(error(
        format!("Invalid value for {name}"),
        format!("zone dimensions must be {rule} (got {value})"),
    )
    .into())
}

fn check_dimensions(args: &ZoneArgs, shape: &str) -> Result<()> {
    for side in Side::BOTH {
        for field in ALL_FIELDS {
            if let Some(v) = args.get(side, field) {
                check_dimension(&flag(side, field), shape, field, v)?;
            }
        }
    }
    Ok(())
}

/// Builds a zone from `--shape` and the shape-specific flags.
///
/// Both the inner and the outer shape are required (the SDK dereferences both),
/// each with every field of the chosen shape.
pub fn zone_from_flags(args: &ZoneArgs) -> Result<ZoneDefinition> {
    let shape_name = args.shape.as_deref().ok_or_else(|| {
        error(
            "Missing --shape",
            format!(
                "zone flags need --shape, one of: {}",
                shape_names().join(", ")
            ),
        )
        .with_suggestion("Pass --shape box|capsule|cone|sphere")
    })?;
    let (shape, fields) = find_shape(shape_name)?;
    check_dimensions(args, shape)?;

    let mut stray = Vec::new();
    let mut owners: Vec<&str> = Vec::new();
    for side in Side::BOTH {
        for field in ALL_FIELDS {
            if !fields.contains(field) && args.get(side, field).is_some() {
                stray.push(flag(side, field));
                owners.extend(
                    SHAPES
                        .iter()
                        .filter(|(n, f)| *n != shape && f.contains(field))
                        .map(|(n, _)| *n),
                );
            }
        }
    }
    owners.sort_unstable();
    owners.dedup();
    if !stray.is_empty() {
        return Err(error(
            format!("Flags do not match --shape {shape}"),
            format!(
                "{} belong to the {} shape, not {shape} (a {shape} uses: {})",
                stray.join(", "),
                owners.join("/"),
                fields.join(", ").replace('_', "-")
            ),
        )
        .with_suggestion(format!(
            "Remove the extra flags, or change --shape to the shape they belong to ({shape} was requested)"
        ))
        .into());
    }

    let mut missing = Vec::new();
    let mut sides: Vec<Values> = Vec::new();
    for side in Side::BOTH {
        let given: Values = fields
            .iter()
            .filter_map(|f| args.get(side, f).map(|v| (*f, v)))
            .collect();
        missing.extend(
            fields
                .iter()
                .filter(|f| args.get(side, f).is_none())
                .map(|f| flag(side, f)),
        );
        sides.push(given);
    }
    if !missing.is_empty() {
        return Err(error(
            format!("Incomplete {shape} zone"),
            format!(
                "the SDK needs both an inner and an outer shape; missing required flags: {}",
                missing.join(", ")
            ),
        )
        .into());
    }

    let mut sides = sides.into_iter();
    let inner = sides.next().unwrap_or_default();
    let outer = sides.next().unwrap_or_default();
    Ok(build_zone(shape, inner, outer))
}

fn validate_number(text: &str) -> Result<Validation, inquire::CustomUserError> {
    Ok(match text.trim().parse::<f32>() {
        Ok(v) if v.is_finite() && v >= 0.0 => Validation::Valid,
        _ => Validation::Invalid("Enter a number that is not negative".into()),
    })
}

fn prompt_side(
    input: &dyn Input,
    side: Side,
    shape: &str,
    fields: &[&'static str],
) -> Result<Values> {
    let label_prefix = match side {
        Side::Inner => "Inner",
        Side::Outer => "Outer",
    };
    fields
        .iter()
        .map(|field| {
            let label = format!("{label_prefix} {}", field.replace('_', " "));
            let text = input.prompt_text(&label, None, None, Some(&validate_number))?;
            let value = text.trim().parse::<f32>().unwrap_or(f32::NAN);
            check_dimension(&label, shape, field, value)?;
            Ok((*field, value))
        })
        .collect()
}

/// Interactive zone entry: pick the shape, then prompt each inner and outer field.
pub fn prompt_zone(input: &dyn Input) -> Result<ZoneDefinition> {
    let chosen = input.select("Zone shape", &shape_names())?;
    let (shape, fields) = find_shape(&chosen)?;
    let inner = prompt_side(input, Side::Inner, shape, fields)?;
    let outer = prompt_side(input, Side::Outer, shape, fields)?;
    Ok(build_zone(shape, inner, outer))
}

/// The shape name and the values of each side of `zone`, keyed by schema field.
fn zone_sides(zone: &ZoneDefinition) -> (&'static str, [Option<Values>; 2]) {
    match zone {
        ZoneDefinition::Box(z) => (
            "box",
            [&z.inner, &z.outer].map(|s| {
                s.as_ref().map(|s| {
                    vec![
                        ("half_width", s.half_width),
                        ("half_height", s.half_height),
                        ("half_depth", s.half_depth),
                    ]
                })
            }),
        ),
        ZoneDefinition::Capsule(z) => (
            "capsule",
            [&z.inner, &z.outer].map(|s| {
                s.as_ref()
                    .map(|s| vec![("radius", s.radius), ("half_height", s.half_height)])
            }),
        ),
        ZoneDefinition::Cone(z) => (
            "cone",
            [&z.inner, &z.outer].map(|s| {
                s.as_ref()
                    .map(|s| vec![("radius", s.radius), ("height", s.height)])
            }),
        ),
        ZoneDefinition::Sphere(z) => (
            "sphere",
            [&z.inner, &z.outer].map(|s| s.as_ref().map(|s| vec![("radius", s.radius)])),
        ),
    }
}

/// Checks a zone that was not built by [`zone_from_flags`] (e.g. read from a
/// hand-edited file): both the inner and the outer shape must be present, as
/// the SDK dereferences both, and every dimension must satisfy the same rules
/// as the flags.
pub fn validate_zone(zone: &ZoneDefinition) -> Result<()> {
    let (shape, sides) = zone_sides(zone);
    for (side, values) in Side::BOTH.into_iter().zip(sides) {
        let Some(values) = values else {
            return Err(error(
                format!("Incomplete {shape} zone"),
                format!(
                    "the SDK needs both an inner and an outer shape; the {} shape is missing",
                    side.prefix()
                ),
            )
            .into());
        };
        for (field, value) in values {
            check_dimension(&flag(side, field), shape, field, value)?;
        }
    }
    Ok(())
}

/// Name of the zone's shape (`box`, `capsule`, `cone` or `sphere`).
pub fn zone_shape_name(zone: &ZoneDefinition) -> &'static str {
    zone_sides(zone).0
}

/// Expresses an existing zone as flag values, so `update` can merge partial
/// changes (e.g. only `--outer-radius`) into it.
pub fn zone_to_args(zone: &ZoneDefinition) -> ZoneArgs {
    let (shape, sides) = zone_sides(zone);
    let mut args = ZoneArgs {
        shape: Some(shape.to_string()),
        ..ZoneArgs::default()
    };
    for (side, values) in Side::BOTH.into_iter().zip(sides) {
        for (field, value) in values.unwrap_or_default() {
            args.set(side, field, value);
        }
    }
    args
}

impl ZoneArgs {
    /// Whether any zone flag was given.
    pub fn is_set(&self) -> bool {
        self.shape.is_some()
            || Side::BOTH
                .iter()
                .any(|s| ALL_FIELDS.iter().any(|f| self.get(*s, f).is_some()))
    }

    /// Fills the dimensions this struct lacks from `base`; given ones win.
    pub fn merged_over(&self, base: &ZoneArgs) -> ZoneArgs {
        let mut merged = self.clone();
        for side in Side::BOTH {
            for field in ALL_FIELDS {
                if merged.get(side, field).is_none()
                    && let Some(v) = base.get(side, field)
                {
                    merged.set(side, field, v);
                }
            }
        }
        merged
    }

    fn set(&mut self, side: Side, field: &str, value: f32) {
        let slot = match (side, field) {
            (Side::Inner, "radius") => &mut self.inner_radius,
            (Side::Inner, "height") => &mut self.inner_height,
            (Side::Inner, "half_width") => &mut self.inner_half_width,
            (Side::Inner, "half_height") => &mut self.inner_half_height,
            (Side::Inner, "half_depth") => &mut self.inner_half_depth,
            (Side::Outer, "radius") => &mut self.outer_radius,
            (Side::Outer, "height") => &mut self.outer_height,
            (Side::Outer, "half_width") => &mut self.outer_half_width,
            (Side::Outer, "half_height") => &mut self.outer_half_height,
            (Side::Outer, "half_depth") => &mut self.outer_half_depth,
            _ => return,
        };
        *slot = Some(value);
    }
}
