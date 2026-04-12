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

//! Effect asset type.
//!
//! Represents an audio effect (reverb, EQ, etc.) that can be applied to sounds,
//! collections, or buses to modify their audio characteristics for the Amplitude Audio SDK.

use super::generated::{EffectDefinition, RtpcCompatibleValue};
use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

// =============================================================================
// Effect Type Alias
// =============================================================================

/// Audio effects (reverb, EQ, etc.).
///
/// Type alias to the build-time generated `EffectDefinition` from SDK FlatBuffer schemas.
/// Represents an audio effect that can be applied to sounds, collections, or buses.
///
/// # Example
///
/// ```
/// use am::assets::Effect;
///
/// let effect = Effect::builder(12345, "reverb_large_hall")
///     .effect_type("reverb")
///     .build();
/// ```
pub type Effect = EffectDefinition;

impl Effect {
    /// Creates a builder for constructing an Effect with optional fields.
    pub fn builder(id: u64, name: impl Into<String>) -> EffectBuilder {
        EffectBuilder::new(id, name)
    }
}

// =============================================================================
// EffectBuilder
// =============================================================================

/// Builder for constructing Effect instances with optional fields.
#[derive(Debug, Clone)]
pub struct EffectBuilder {
    effect: Effect,
}

impl EffectBuilder {
    /// Creates a new EffectBuilder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            effect: Effect {
                id,
                name: Some(name.into()),
                effect: None,
                parameters: None,
            },
        }
    }

    /// Sets the effect type name (e.g., "reverb", "eq").
    pub fn effect_type(mut self, effect_type: impl Into<String>) -> Self {
        self.effect.effect = Some(effect_type.into());
        self
    }

    /// Sets all parameters at once.
    pub fn parameters(mut self, params: Vec<RtpcCompatibleValue>) -> Self {
        self.effect.parameters = Some(params);
        self
    }

    /// Adds a single parameter.
    pub fn add_parameter(mut self, param: RtpcCompatibleValue) -> Self {
        self.effect
            .parameters
            .get_or_insert_with(Vec::new)
            .push(param);
        self
    }

    /// Builds the Effect instance.
    pub fn build(self) -> Effect {
        self.effect
    }
}

// =============================================================================
// Asset Trait Implementation
// =============================================================================

impl Asset for Effect {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Effect
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Effect.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 6
        Ok(())
    }

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        // Validate name is not empty
        if self.name.as_deref().unwrap_or("").is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Effect asset has no name",
                "Effect assets must have a non-empty name",
            )
            .with_suggestion("Set the 'name' field to a valid identifier (e.g., \"reverb\")")
            .with_field("name"));
        }

        // Validate effect type: if present but empty, error
        if let Some(ref effect_type) = self.effect
            && effect_type.is_empty()
        {
            return Err(ValidationError::type_rule_violation(
                "Effect type is empty",
                "Effect type must be a non-empty string when specified",
            )
            .with_suggestion(
                "Set the 'effect' field to a valid type name (e.g., \"reverb\", \"eq\")",
            )
            .with_field("effect"));
        }

        // Validate parameter values are finite
        if let Some(ref params) = self.parameters {
            for (i, param) in params.iter().enumerate() {
                if let Some(value) = param.as_static()
                    && !value.is_finite()
                {
                    return Err(ValidationError::type_rule_violation(
                        format!("Parameter value is not finite at index {}", i),
                        "Parameter static values must be finite numbers",
                    )
                    .with_suggestion("Use a valid finite number for the parameter value")
                    .with_field("parameters"));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Builder Tests (P0)
    // =========================================================================

    #[test]
    fn test_effect_builder_basic() {
        let effect = Effect::builder(12345, "reverb_large_hall").build();
        assert_eq!(effect.id(), 12345);
        assert_eq!(effect.name(), "reverb_large_hall");
    }

    #[test]
    fn test_effect_builder_defaults() {
        let e = Effect::builder(1, "test").build();
        assert_eq!(e.id, 1);
        assert_eq!(e.name.as_deref(), Some("test"));
        assert!(e.effect.is_none());
        assert!(e.parameters.is_none());
    }

    #[test]
    fn test_effect_builder_all_fields() {
        let e = Effect::builder(12345, "reverb_large_hall")
            .effect_type("reverb")
            .parameters(vec![
                RtpcCompatibleValue::static_value(0.8),
                RtpcCompatibleValue::static_value(0.5),
            ])
            .build();

        assert_eq!(e.id, 12345);
        assert_eq!(e.name.as_deref(), Some("reverb_large_hall"));
        assert_eq!(e.effect.as_deref(), Some("reverb"));
        assert_eq!(e.parameters.as_ref().unwrap().len(), 2);
        assert_eq!(e.parameters.as_ref().unwrap()[0].as_static(), Some(0.8));
        assert_eq!(e.parameters.as_ref().unwrap()[1].as_static(), Some(0.5));
    }

    #[test]
    fn test_effect_builder_add_parameter() {
        let e = Effect::builder(1, "eq")
            .effect_type("eq")
            .add_parameter(RtpcCompatibleValue::static_value(1.0))
            .add_parameter(RtpcCompatibleValue::static_value(2.0))
            .build();

        assert_eq!(e.parameters.as_ref().unwrap().len(), 2);
    }

    // =========================================================================
    // Asset Trait Tests (P0)
    // =========================================================================

    #[test]
    fn test_effect_asset_type() {
        let e = Effect::builder(1, "test").build();
        assert_eq!(e.asset_type(), AssetType::Effect);
    }

    #[test]
    fn test_effect_file_extension() {
        let e = Effect::builder(1, "test").build();
        assert_eq!(e.file_extension(), ".json");
    }

    // =========================================================================
    // Serde Tests (P0)
    // =========================================================================

    #[test]
    fn test_effect_serde_roundtrip() {
        let e = Effect::builder(12345, "reverb_large_hall")
            .effect_type("reverb")
            .parameters(vec![
                RtpcCompatibleValue::static_value(0.8),
                RtpcCompatibleValue::static_value(0.5),
            ])
            .build();

        let json = serde_json::to_string(&e).unwrap();
        let parsed: Effect = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, e.id);
        assert_eq!(parsed.name, e.name);
        assert_eq!(parsed.effect, e.effect);
        assert_eq!(parsed.parameters, e.parameters);
    }

    #[test]
    fn test_effect_deserialize_sdk_json() {
        let sdk_json = r#"{
            "id": 54321,
            "name": "reverb_hall",
            "effect": "reverb",
            "parameters": [
                { "kind": "Static", "value": 0.8 },
                { "kind": "Static", "value": 0.5 }
            ]
        }"#;

        let e: Effect = serde_json::from_str(sdk_json).unwrap();
        assert_eq!(e.id, 54321);
        assert_eq!(e.name.as_deref(), Some("reverb_hall"));
        assert_eq!(e.effect.as_deref(), Some("reverb"));
        assert_eq!(e.parameters.as_ref().unwrap().len(), 2);
        assert_eq!(e.parameters.as_ref().unwrap()[0].as_static(), Some(0.8));
        assert_eq!(e.parameters.as_ref().unwrap()[1].as_static(), Some(0.5));
    }

    #[test]
    fn test_effect_serialize_sdk_format() {
        let e = Effect::builder(12345, "reverb_large_hall")
            .effect_type("reverb")
            .parameters(vec![RtpcCompatibleValue::static_value(0.8)])
            .build();

        let json = serde_json::to_string_pretty(&e).unwrap();

        assert!(json.contains("\"id\": 12345"));
        assert!(json.contains("\"name\": \"reverb_large_hall\""));
        assert!(json.contains("\"effect\": \"reverb\""));
        assert!(json.contains("\"kind\": \"Static\""));
        assert!(json.contains("\"value\": 0.8"));
    }

    // =========================================================================
    // Validation Tests (P1)
    // =========================================================================

    #[test]
    fn test_effect_validate_rules_passes_valid() {
        let context = ProjectContext::empty();
        let e = Effect::builder(1, "reverb").effect_type("reverb").build();
        assert!(e.validate_rules(&context).is_ok());
    }

    #[test]
    fn test_effect_validate_rules_fails_empty_name() {
        let context = ProjectContext::empty();
        let mut e = Effect::builder(1, "test").build();
        e.name = Some(String::new());
        let result = e.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
        assert_eq!(err.field, Some("name".to_string()));
    }

    #[test]
    fn test_effect_validate_rules_fails_none_name() {
        let context = ProjectContext::empty();
        let mut e = Effect::builder(1, "test").build();
        e.name = None;
        let result = e.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
    }

    #[test]
    fn test_effect_validate_rules_fails_empty_effect_type() {
        let context = ProjectContext::empty();
        let mut e = Effect::builder(1, "test").build();
        e.effect = Some(String::new());
        let result = e.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Effect type is empty"));
        assert_eq!(err.field, Some("effect".to_string()));
    }

    #[test]
    fn test_effect_validate_rules_fails_nan_parameter() {
        let context = ProjectContext::empty();
        let e = Effect::builder(1, "test")
            .effect_type("reverb")
            .add_parameter(RtpcCompatibleValue::static_value(f32::NAN))
            .build();
        let result = e.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("not finite"));
        assert_eq!(err.field, Some("parameters".to_string()));
    }

    #[test]
    fn test_effect_validate_rules_fails_infinite_parameter() {
        let context = ProjectContext::empty();
        let e = Effect::builder(1, "test")
            .effect_type("reverb")
            .add_parameter(RtpcCompatibleValue::static_value(f32::INFINITY))
            .build();
        let result = e.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("not finite"));
    }
}
