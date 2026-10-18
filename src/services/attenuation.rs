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

//! Attenuation creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI produce an [`AttenuationValues`] (create)
//! or an [`AttenuationPatch`] (update); this module turns them into a validated
//! [`Attenuation`] without touching the terminal or the file system. Functions
//! return new values and never mutate their inputs.

use crate::assets::generated::{
    AttenuationAirAbsorptionDefinition, AttenuationShapeDefinition, CurveDefinition, ZoneDefinition,
};
use crate::assets::{Asset, Attenuation, ProjectContext, ValidationError, ValidationLayer};
use crate::common::errors::codes::ERR_ASSET_ALREADY_EXISTS;
use crate::common::utils::generate_unique_id;

/// Number of times id generation is retried after a collision.
const ID_COLLISION_RETRIES: usize = 3;

/// The SDK's own air absorption coefficients (`AttenuationImpl` constructor),
/// used when absorption is enabled without explicit values.
pub const DEFAULT_AIR_ABSORPTION_COEFFICIENTS: [f32; 3] = [0.0002, 0.0017, 0.0182];

/// Factor applied to the whole zone when none is given: no extra attenuation.
/// (The schema default is 0, which would mute everything inside the zone.)
pub const DEFAULT_MAX_ATTENUATION_FACTOR: f32 = 1.0;

/// Air absorption settings requested for a new attenuation.
#[derive(Debug, Clone, PartialEq)]
pub struct AirAbsorptionValues {
    pub enabled: bool,
    /// Exactly 3 values when set; SDK defaults are used when `None` and enabled.
    pub coefficients: Option<Vec<f32>>,
}

/// Every value a new attenuation needs.
#[derive(Debug, Clone, PartialEq)]
pub struct AttenuationValues {
    pub name: String,
    pub max_distance: f64,
    pub zone: ZoneDefinition,
    pub max_attenuation_factor: f32,
    pub gain_curve: CurveDefinition,
    pub spread_curve: Option<CurveDefinition>,
    /// `None` leaves the block out, so the SDK applies its own defaults.
    pub air_absorption: Option<AirAbsorptionValues>,
}

/// Requested air absorption changes; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AirAbsorptionPatch {
    pub enabled: Option<bool>,
    pub coefficients: Option<Vec<f32>>,
}

impl AirAbsorptionPatch {
    fn is_empty(&self) -> bool {
        self.enabled.is_none() && self.coefficients.is_none()
    }
}

/// The fields of an existing attenuation a caller wants to change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttenuationPatch {
    pub max_distance: Option<f64>,
    pub zone: Option<ZoneDefinition>,
    pub max_attenuation_factor: Option<f32>,
    pub gain_curve: Option<CurveDefinition>,
    pub spread_curve: Option<CurveDefinition>,
    pub air_absorption: AirAbsorptionPatch,
}

impl AttenuationPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("max_distance", self.max_distance.is_some()),
            ("shape", self.zone.is_some()),
            (
                "max_attenuation_factor",
                self.max_attenuation_factor.is_some(),
            ),
            ("gain_curve", self.gain_curve.is_some()),
            ("spread_curve", self.spread_curve.is_some()),
            ("air_absorption", !self.air_absorption.is_empty()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

fn unique_id(name: &str, ctx: &ProjectContext) -> Result<u64, ValidationError> {
    let mut id = generate_unique_id(name);
    for attempt in 0..ID_COLLISION_RETRIES {
        if !ctx.has_id(id) {
            break;
        }
        id = generate_unique_id(&format!("{name}{attempt}"));
    }
    if ctx.has_id(id) {
        return Err(ValidationError::new(
            ValidationLayer::IdUniqueness,
            ERR_ASSET_ALREADY_EXISTS,
            format!("Generated ID {id} for attenuation '{name}' collides with an existing asset"),
            "All generated ID attempts collided with existing assets in the project",
        )
        .with_suggestion("Try a different name or wait a moment and retry")
        .with_field("id"));
    }
    Ok(id)
}

fn air_absorption(
    enabled: bool,
    coefficients: Option<Vec<f32>>,
) -> AttenuationAirAbsorptionDefinition {
    // Enabled absorption without values would crash the SDK, so fall back to its defaults.
    let coefficients = match (enabled, coefficients) {
        (_, Some(values)) => Some(values),
        (true, None) => Some(DEFAULT_AIR_ABSORPTION_COEFFICIENTS.to_vec()),
        (false, None) => None,
    };
    AttenuationAirAbsorptionDefinition {
        enabled,
        coefficients,
    }
}

/// Builds a validated attenuation from `values`, generating a unique id.
pub fn build_attenuation(
    values: AttenuationValues,
    ctx: &ProjectContext,
) -> Result<Attenuation, ValidationError> {
    let id = unique_id(&values.name, ctx)?;

    let mut builder = Attenuation::builder(id, &values.name)
        .max_distance(values.max_distance)
        .shape(AttenuationShapeDefinition {
            max_attenuation_factor: values.max_attenuation_factor,
            zone: Some(values.zone),
        })
        .gain_curve(values.gain_curve);
    if let Some(spread) = values.spread_curve {
        builder = builder.spread_curve(spread);
    }
    if let Some(air) = values.air_absorption {
        builder = builder.air_absorption(air_absorption(air.enabled, air.coefficients));
    }
    let attenuation = builder.build();

    attenuation.validate_rules(ctx)?;
    Ok(attenuation)
}

fn merged_air_absorption(
    current: Option<&AttenuationAirAbsorptionDefinition>,
    patch: &AirAbsorptionPatch,
) -> Option<AttenuationAirAbsorptionDefinition> {
    if patch.is_empty() {
        return current.cloned();
    }
    // An absent block means "enabled" in the SDK, so merge against that.
    let enabled = patch.enabled.or(current.map(|c| c.enabled)).unwrap_or(true);
    let coefficients = patch
        .coefficients
        .clone()
        .or_else(|| current.and_then(|c| c.coefficients.clone()));
    Some(air_absorption(enabled, coefficients))
}

/// Merges `patch` into a copy of `existing`; fields the patch leaves unset are preserved.
pub fn apply_patch(existing: &Attenuation, patch: &AttenuationPatch) -> Attenuation {
    let mut attenuation = existing.clone();

    if let Some(distance) = patch.max_distance {
        attenuation.max_distance = distance;
    }
    if patch.zone.is_some() || patch.max_attenuation_factor.is_some() {
        let current = existing
            .shape
            .clone()
            .unwrap_or(AttenuationShapeDefinition {
                max_attenuation_factor: DEFAULT_MAX_ATTENUATION_FACTOR,
                zone: None,
            });
        attenuation.shape = Some(AttenuationShapeDefinition {
            max_attenuation_factor: patch
                .max_attenuation_factor
                .unwrap_or(current.max_attenuation_factor),
            zone: patch.zone.clone().or(current.zone),
        });
    }
    if let Some(curve) = &patch.gain_curve {
        attenuation.gain_curve = Some(curve.clone());
    }
    if let Some(curve) = &patch.spread_curve {
        attenuation.spread_curve = Some(curve.clone());
    }
    attenuation.air_absorption =
        merged_air_absorption(existing.air_absorption.as_ref(), &patch.air_absorption);
    attenuation
}
