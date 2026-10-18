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

//! Attenuation asset type.
//!
//! Describes how a sound's volume and spread fall off with distance.

use super::generated::{
    AttenuationAirAbsorptionDefinition, AttenuationDefinition, AttenuationShapeDefinition,
    CurveDefinition,
};
use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// Attenuation definition.
///
/// Type alias to the build-time generated `AttenuationDefinition`.
///
/// # Example
///
/// ```
/// use am::assets::Attenuation;
///
/// let attenuation = Attenuation::builder(1, "short").max_distance(50.0).build();
/// assert_eq!(attenuation.max_distance, 50.0);
/// ```
pub type Attenuation = AttenuationDefinition;

impl Attenuation {
    /// Creates a builder for constructing an Attenuation.
    pub fn builder(id: u64, name: impl Into<String>) -> AttenuationBuilder {
        AttenuationBuilder::new(id, name)
    }
}

/// Builder for constructing Attenuation instances.
#[derive(Debug, Clone)]
pub struct AttenuationBuilder {
    attenuation: Attenuation,
}

impl AttenuationBuilder {
    /// Creates a new builder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            attenuation: Attenuation {
                id,
                name: Some(name.into()),
                max_distance: 0.0,
                shape: None,
                gain_curve: None,
                spread_curve: None,
                air_absorption: None,
            },
        }
    }

    /// Sets the maximum audible distance.
    pub fn max_distance(mut self, distance: f64) -> Self {
        self.attenuation.max_distance = distance;
        self
    }

    /// Sets the attenuation shape.
    pub fn shape(mut self, shape: AttenuationShapeDefinition) -> Self {
        self.attenuation.shape = Some(shape);
        self
    }

    /// Sets the gain curve.
    pub fn gain_curve(mut self, curve: CurveDefinition) -> Self {
        self.attenuation.gain_curve = Some(curve);
        self
    }

    /// Sets the spread curve.
    pub fn spread_curve(mut self, curve: CurveDefinition) -> Self {
        self.attenuation.spread_curve = Some(curve);
        self
    }

    /// Sets the air absorption settings.
    pub fn air_absorption(mut self, absorption: AttenuationAirAbsorptionDefinition) -> Self {
        self.attenuation.air_absorption = Some(absorption);
        self
    }

    /// Builds the Attenuation instance.
    pub fn build(self) -> Attenuation {
        self.attenuation
    }
}

impl Asset for Attenuation {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Attenuation
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Attenuation.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        Ok(())
    }

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        if self.name.as_deref().unwrap_or("").is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Attenuation asset has no name",
                "Attenuation assets must have a non-empty name",
            )
            .with_suggestion("Set the 'name' field to a valid identifier (e.g., \"short\")")
            .with_field("name"));
        }

        if !self.max_distance.is_finite() || self.max_distance < 0.0 {
            return Err(ValidationError::type_rule_violation(
                format!("Invalid max_distance: {}", self.max_distance),
                "Attenuation max_distance must be a finite, non-negative number",
            )
            .with_suggestion("Set 'max_distance' to a value >= 0 (e.g., 50.0)")
            .with_field("max_distance"));
        }

        self.validate_shape()?;
        self.validate_curves()?;
        self.validate_air_absorption()
    }
}

/// Number of bands in the SDK's air absorption EQ.
pub const AIR_ABSORPTION_BANDS: usize = 3;

fn field_error(field: &str, what: impl Into<String>, why: impl Into<String>) -> ValidationError {
    ValidationError::type_rule_violation(what, why).with_field(field)
}

impl Attenuation {
    /// The SDK builds the attenuation zone from `shape` without a null check
    /// and reads both the inner and the outer part of the zone.
    fn validate_shape(&self) -> Result<(), ValidationError> {
        let Some(shape) = &self.shape else {
            return Err(field_error(
                "shape",
                "Attenuation has no shape",
                "The SDK dereferences the shape unconditionally; every attenuation needs one",
            )
            .with_suggestion(
                "Set a zone with --shape box|capsule|cone|sphere and its dimensions",
            ));
        };

        let factor = shape.max_attenuation_factor;
        if !factor.is_finite() || !(0.0..=1.0).contains(&factor) {
            return Err(field_error(
                "max_attenuation_factor",
                format!("Invalid max_attenuation_factor: {factor}"),
                "The maximum attenuation factor must be in the range [0..1]",
            )
            .with_suggestion("Use --max-attenuation-factor with a value between 0 and 1"));
        }

        let Some(zone) = &shape.zone else {
            return Err(field_error(
                "shape",
                "Attenuation shape has no zone",
                "Without a zone the SDK reads a missing cone and crashes; the shape needs a zone",
            )
            .with_suggestion(
                "Set a zone with --shape box|capsule|cone|sphere and its dimensions",
            ));
        };
        crate::forms::zone::validate_zone(zone)
            .map_err(|e| field_error("shape", "Invalid attenuation zone", format!("{e:#}")))
    }

    /// The gain curve decides the gain at every distance, so it is required;
    /// the spread curve is optional ("absent means 1").
    fn validate_curves(&self) -> Result<(), ValidationError> {
        let Some(gain) = &self.gain_curve else {
            return Err(field_error(
                "gain_curve",
                "Attenuation has no gain curve",
                "Without a gain curve the sound is silent at every distance",
            )
            .with_suggestion("Add a gain curve with --curve-part or --curve-from-json"));
        };
        crate::forms::curve::validate_curve(gain)
            .map_err(|e| field_error("gain_curve", "Invalid gain curve", format!("{e:#}")))?;
        if let Some(spread) = &self.spread_curve {
            crate::forms::curve::validate_curve(spread).map_err(|e| {
                field_error("spread_curve", "Invalid spread curve", format!("{e:#}"))
            })?;
        }
        Ok(())
    }

    /// When air absorption is enabled (the schema default) the SDK reads
    /// `coefficients[0..3]` without a null or size check.
    fn validate_air_absorption(&self) -> Result<(), ValidationError> {
        let Some(air) = &self.air_absorption else {
            return Ok(());
        };
        match &air.coefficients {
            Some(c) if c.len() != AIR_ABSORPTION_BANDS => Err(field_error(
                "air_absorption",
                format!("Air absorption has {} coefficients", c.len()),
                format!(
                    "Exactly {AIR_ABSORPTION_BANDS} coefficients are required, one per EQ band"
                ),
            )
            .with_suggestion("Use --air-absorption-coefficients a,b,c")),
            Some(c) if c.iter().any(|v| !v.is_finite() || *v < 0.0) => Err(field_error(
                "air_absorption",
                "Invalid air absorption coefficients",
                "Coefficients must be finite and not negative",
            )),
            None if air.enabled => Err(field_error(
                "air_absorption",
                "Air absorption is enabled without coefficients",
                "The SDK reads the 3 coefficients unconditionally when air absorption is enabled",
            )
            .with_suggestion(
                "Pass --air-absorption-coefficients a,b,c, or --air-absorption false",
            )),
            _ => Ok(()),
        }
    }
}
