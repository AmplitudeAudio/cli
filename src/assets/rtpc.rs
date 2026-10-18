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

//! RTPC (Real-Time Parameter Control) asset type.

use super::generated::{RtpcDefinition, RtpcFadingSettings};
use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// RTPC definition.
///
/// Type alias to the build-time generated `RtpcDefinition`.
///
/// # Example
///
/// ```
/// use am::assets::Rtpc;
///
/// let rtpc = Rtpc::builder(1, "speed").min_value(0.0).max_value(10.0).build();
/// assert_eq!(rtpc.max_value, 10.0);
/// ```
pub type Rtpc = RtpcDefinition;

impl Rtpc {
    /// Creates a builder for constructing an Rtpc.
    pub fn builder(id: u64, name: impl Into<String>) -> RtpcBuilder {
        RtpcBuilder::new(id, name)
    }
}

/// Builder for constructing Rtpc instances.
#[derive(Debug, Clone)]
pub struct RtpcBuilder {
    rtpc: Rtpc,
}

impl RtpcBuilder {
    /// Creates a new builder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            rtpc: Rtpc {
                id,
                name: Some(name.into()),
                min_value: 0.0,
                max_value: 1.0,
                default_value: 0.0,
                fade_settings: None,
            },
        }
    }

    /// Sets the minimum value.
    pub fn min_value(mut self, value: f64) -> Self {
        self.rtpc.min_value = value;
        self
    }

    /// Sets the maximum value.
    pub fn max_value(mut self, value: f64) -> Self {
        self.rtpc.max_value = value;
        self
    }

    /// Sets the default value.
    pub fn default_value(mut self, value: f64) -> Self {
        self.rtpc.default_value = value;
        self
    }

    /// Sets the fading settings.
    pub fn fade_settings(mut self, settings: RtpcFadingSettings) -> Self {
        self.rtpc.fade_settings = Some(settings);
        self
    }

    /// Builds the Rtpc instance.
    pub fn build(self) -> Rtpc {
        self.rtpc
    }
}

impl Asset for Rtpc {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Rtpc
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Rtpc.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        Ok(())
    }

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        if self.name.as_deref().unwrap_or("").is_empty() {
            return Err(ValidationError::type_rule_violation(
                "RTPC asset has no name",
                "RTPC assets must have a non-empty name",
            )
            .with_suggestion("Set the 'name' field to a valid identifier (e.g., \"speed\")")
            .with_field("name"));
        }

        let values = [
            ("min_value", self.min_value),
            ("max_value", self.max_value),
            ("default_value", self.default_value),
        ];
        for (field, value) in values {
            if !value.is_finite() {
                return Err(ValidationError::type_rule_violation(
                    format!("Invalid {}: {}", field, value),
                    "RTPC values must be finite numbers",
                )
                .with_suggestion(format!("Set '{}' to a finite number", field))
                .with_field(field));
            }
        }

        if self.min_value > self.max_value {
            return Err(ValidationError::type_rule_violation(
                format!(
                    "min_value ({}) is greater than max_value ({})",
                    self.min_value, self.max_value
                ),
                "RTPC min_value must not exceed max_value",
            )
            .with_suggestion("Swap or adjust 'min_value' and 'max_value'")
            .with_field("min_value"));
        }

        if self.default_value < self.min_value || self.default_value > self.max_value {
            return Err(ValidationError::type_rule_violation(
                format!(
                    "default_value ({}) is outside [{}, {}]",
                    self.default_value, self.min_value, self.max_value
                ),
                "RTPC default_value must lie within min_value and max_value",
            )
            .with_suggestion("Set 'default_value' between 'min_value' and 'max_value'")
            .with_field("default_value"));
        }

        Ok(())
    }
}
