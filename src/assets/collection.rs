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

//! Collection asset type.
//!
//! Represents a group of related sounds that can be played with variation
//! (random selection, sequential, etc.) for the Amplitude Audio SDK.
//! Collections share many audio fields with Sound (gain, priority, fader, etc.)
//! but add collection-specific playback configuration (play_mode, scheduler).

use super::generated::{
    CollectionDefinition, CollectionPlayMode, RtpcCompatibleValue, Scope, SoundSchedulerMode,
    SoundSchedulerSettings, Spatialization,
};
use super::{Asset, AssetType, FaderAlgorithm, ProjectContext, Schema, ValidationError};

// =============================================================================
// Collection Type Alias
// =============================================================================

/// Grouped sound variations.
///
/// Type alias to the build-time generated `CollectionDefinition` from SDK FlatBuffer schemas.
/// Represents a collection of related sounds that can be played with variation.
///
/// # Example
///
/// ```
/// use am::assets::Collection;
///
/// let collection = Collection::builder(12345, "footsteps")
///     .play_mode(am::assets::generated::CollectionPlayMode::PlayOne)
///     .gain(0.9)
///     .build();
/// ```
pub type Collection = CollectionDefinition;

impl Collection {
    /// Creates a builder for constructing a Collection with optional fields.
    pub fn builder(id: u64, name: impl Into<String>) -> CollectionBuilder {
        CollectionBuilder::new(id, name)
    }
}

// =============================================================================
// CollectionBuilder
// =============================================================================

/// Default gain for a new collection (full volume).
const DEFAULT_GAIN: f32 = 1.0;
/// Default priority for a new collection (mid-range).
const DEFAULT_PRIORITY: f32 = 128.0;
/// ID value meaning "no reference" in the SDK.
const NO_REFERENCE: u64 = 0;

/// Builder for constructing Collection instances with optional fields.
#[derive(Debug, Clone)]
pub struct CollectionBuilder {
    collection: Collection,
}

impl CollectionBuilder {
    /// Creates a new CollectionBuilder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            collection: Collection {
                id,
                name: Some(name.into()),
                bus: NO_REFERENCE,
                effect: NO_REFERENCE,
                attenuation: NO_REFERENCE,
                gain: Some(RtpcCompatibleValue::static_value(DEFAULT_GAIN)),
                priority: Some(RtpcCompatibleValue::static_value(DEFAULT_PRIORITY)),
                pitch: None,
                fader: Some(FaderAlgorithm::Linear.to_string()),
                spatialization: Spatialization::None,
                scope: Scope::World,
                play_mode: CollectionPlayMode::PlayOne,
                scheduler: Some(SoundSchedulerSettings::default()),
            },
        }
    }

    /// Sets the bus ID.
    pub fn bus(mut self, bus_id: u64) -> Self {
        self.collection.bus = bus_id;
        self
    }

    /// Sets the gain as a static value.
    pub fn gain(mut self, value: f32) -> Self {
        self.collection.gain = Some(RtpcCompatibleValue::static_value(value));
        self
    }

    /// Sets the gain with full RtpcCompatibleValue control.
    pub fn gain_rtpc(mut self, value: RtpcCompatibleValue) -> Self {
        self.collection.gain = Some(value);
        self
    }

    /// Sets the priority as a static value.
    pub fn priority(mut self, value: u8) -> Self {
        self.collection.priority = Some(RtpcCompatibleValue::static_value(value as f32));
        self
    }

    /// Sets the priority with full RtpcCompatibleValue control.
    pub fn priority_rtpc(mut self, value: RtpcCompatibleValue) -> Self {
        self.collection.priority = Some(value);
        self
    }

    /// Sets the pitch with full RtpcCompatibleValue control.
    pub fn pitch(mut self, value: RtpcCompatibleValue) -> Self {
        self.collection.pitch = Some(value);
        self
    }

    /// Sets the fader algorithm.
    pub fn fader(mut self, fader: FaderAlgorithm) -> Self {
        self.collection.fader = Some(fader.to_string());
        self
    }

    /// Sets the spatialization mode.
    pub fn spatialization(mut self, mode: Spatialization) -> Self {
        self.collection.spatialization = mode;
        self
    }

    /// Sets the scope mode.
    pub fn scope(mut self, scope: Scope) -> Self {
        self.collection.scope = scope;
        self
    }

    /// Sets the attenuation model ID.
    pub fn attenuation(mut self, attenuation_id: u64) -> Self {
        self.collection.attenuation = attenuation_id;
        self
    }

    /// Sets the effect ID.
    pub fn effect(mut self, effect_id: u64) -> Self {
        self.collection.effect = effect_id;
        self
    }

    /// Sets the play mode.
    pub fn play_mode(mut self, mode: CollectionPlayMode) -> Self {
        self.collection.play_mode = mode;
        self
    }

    /// Sets the scheduler mode.
    pub fn scheduler_mode(mut self, mode: SoundSchedulerMode) -> Self {
        self.collection.scheduler = Some(SoundSchedulerSettings { mode });
        self
    }

    /// Builds the Collection instance.
    pub fn build(self) -> Collection {
        self.collection
    }
}

// =============================================================================
// Asset Trait Implementation
// =============================================================================

impl Asset for Collection {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Collection
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Collection.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 6
        Ok(())
    }

    fn validate_rules(&self, context: &ProjectContext) -> Result<(), ValidationError> {
        // Validate name is not empty
        if self.name.as_deref().unwrap_or("").is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Collection asset has no name",
                "Collection assets must have a non-empty name",
            )
            .with_suggestion("Set the 'name' field to a valid identifier (e.g., \"footsteps\")")
            .with_field("name"));
        }

        // Check gain range (if static value)
        if let Some(gain_value) = self.gain.as_ref().and_then(|g| g.as_static())
            && (!gain_value.is_finite() || !(0.0..=1.0).contains(&gain_value))
        {
            return Err(ValidationError::type_rule_violation(
                format!("Invalid gain value: {}", gain_value),
                "Gain must be between 0.0 and 1.0",
            )
            .with_suggestion("Set gain to a value between 0.0 (silent) and 1.0 (full volume)")
            .with_field("gain"));
        }

        // Validate fader algorithm is a known value
        if let Some(fader_str) = &self.fader {
            if FaderAlgorithm::from_str(fader_str).is_err() {
                return Err(ValidationError::type_rule_violation(
                    format!("Unknown fader algorithm: '{}'", fader_str),
                    "Fader must be a valid algorithm name",
                )
                .with_suggestion(format!(
                    "Use one of: {}",
                    super::extensions::FADER_ALGORITHM_NAMES.join(", ")
                ))
                .with_field("fader"));
            }
        }

        // Validate scheduler is present (SDK requires a scheduler configuration)
        if self.scheduler.is_none() {
            return Err(ValidationError::type_rule_violation(
                "Collection asset has no scheduler",
                "Collection assets must have a scheduler configuration",
            )
            .with_suggestion("Set the 'scheduler' field with a mode (e.g., {\"mode\": \"Random\"})")
            .with_field("scheduler"));
        }

        // Cross-asset reference checks (only if validator is available)
        if let Some(validator) = &context.validator {
            // Validate effect reference (zero IDs handled internally as no-op)
            validator
                .validate_effect_exists(self.effect)
                .map_err(|e| e.with_field("effect"))?;

            // Attenuation validation deferred — Attenuator asset type not yet implemented.
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collection_builder_basic() {
        let collection = Collection::builder(12345, "footsteps").build();
        assert_eq!(collection.id(), 12345);
        assert_eq!(collection.name(), "footsteps");
    }

    #[test]
    fn test_collection_builder_defaults() {
        let c = Collection::builder(1, "test").build();
        assert_eq!(c.id, 1);
        assert_eq!(c.name.as_deref(), Some("test"));
        assert_eq!(c.bus, 0);
        assert_eq!(c.effect, 0);
        assert_eq!(c.attenuation, 0);
        assert_eq!(c.gain.as_ref().and_then(|g| g.as_static()), Some(1.0));
        assert_eq!(c.priority.as_ref().and_then(|p| p.as_static()), Some(128.0));
        assert!(c.pitch.is_none());
        assert_eq!(c.fader.as_deref(), Some("Linear"));
        assert_eq!(c.spatialization, Spatialization::None);
        assert_eq!(c.scope, Scope::World);
        assert_eq!(c.play_mode, CollectionPlayMode::PlayOne);
        assert_eq!(
            c.scheduler,
            Some(SoundSchedulerSettings {
                mode: SoundSchedulerMode::Random
            })
        );
    }

    #[test]
    fn test_collection_builder_all_fields() {
        let c = Collection::builder(12345, "footsteps")
            .bus(100)
            .gain(0.8)
            .priority(200)
            .pitch(RtpcCompatibleValue::static_value(1.5))
            .fader(FaderAlgorithm::SCurveSmooth)
            .spatialization(Spatialization::Position)
            .scope(Scope::Entity)
            .attenuation(50)
            .effect(25)
            .play_mode(CollectionPlayMode::PlayAll)
            .scheduler_mode(SoundSchedulerMode::Sequence)
            .build();

        assert_eq!(c.id, 12345);
        assert_eq!(c.name.as_deref(), Some("footsteps"));
        assert_eq!(c.bus, 100);
        assert_eq!(c.gain.as_ref().and_then(|g| g.as_static()), Some(0.8));
        assert_eq!(c.priority.as_ref().and_then(|p| p.as_static()), Some(200.0));
        assert_eq!(c.pitch.as_ref().and_then(|p| p.as_static()), Some(1.5));
        assert_eq!(c.fader.as_deref(), Some("SCurveSmooth"));
        assert_eq!(c.spatialization, Spatialization::Position);
        assert_eq!(c.scope, Scope::Entity);
        assert_eq!(c.attenuation, 50);
        assert_eq!(c.effect, 25);
        assert_eq!(c.play_mode, CollectionPlayMode::PlayAll);
        assert_eq!(
            c.scheduler,
            Some(SoundSchedulerSettings {
                mode: SoundSchedulerMode::Sequence
            })
        );
    }

    #[test]
    fn test_collection_asset_type() {
        let c = Collection::builder(1, "test").build();
        assert_eq!(c.asset_type(), AssetType::Collection);
    }

    #[test]
    fn test_collection_file_extension() {
        let c = Collection::builder(1, "test").build();
        assert_eq!(c.file_extension(), ".json");
    }

    #[test]
    fn test_collection_serde_roundtrip() {
        let c = Collection::builder(12345, "footsteps")
            .bus(100)
            .gain(0.8)
            .priority(200)
            .play_mode(CollectionPlayMode::PlayAll)
            .scheduler_mode(SoundSchedulerMode::Sequence)
            .spatialization(Spatialization::Position)
            .scope(Scope::Entity)
            .fader(FaderAlgorithm::EaseInOut)
            .attenuation(50)
            .effect(25)
            .build();

        let json = serde_json::to_string(&c).unwrap();
        let parsed: Collection = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, c.id);
        assert_eq!(parsed.name, c.name);
        assert_eq!(parsed.bus, c.bus);
        assert_eq!(parsed.gain, c.gain);
        assert_eq!(parsed.priority, c.priority);
        assert_eq!(parsed.fader, c.fader);
        assert_eq!(parsed.spatialization, c.spatialization);
        assert_eq!(parsed.scope, c.scope);
        assert_eq!(parsed.attenuation, c.attenuation);
        assert_eq!(parsed.effect, c.effect);
        assert_eq!(parsed.play_mode, c.play_mode);
        assert_eq!(parsed.scheduler, c.scheduler);
    }

    #[test]
    fn test_collection_serde_sdk_format() {
        let c = Collection::builder(12345, "footsteps")
            .bus(200)
            .gain(0.75)
            .priority(128)
            .play_mode(CollectionPlayMode::PlayOne)
            .scheduler_mode(SoundSchedulerMode::Random)
            .spatialization(Spatialization::HRTF)
            .scope(Scope::Entity)
            .fader(FaderAlgorithm::SCurveSmooth)
            .build();

        let json = serde_json::to_string_pretty(&c).unwrap();

        assert!(json.contains("\"id\": 12345"));
        assert!(json.contains("\"name\": \"footsteps\""));
        assert!(json.contains("\"bus\": 200"));
        assert!(json.contains("\"play_mode\": \"PlayOne\""));
        assert!(json.contains("\"spatialization\": \"HRTF\""));
        assert!(json.contains("\"scope\": \"Entity\""));
        assert!(json.contains("\"fader\": \"SCurveSmooth\""));
        assert!(json.contains("\"kind\": \"Static\""));
        assert!(json.contains("\"mode\": \"Random\""));
    }

    #[test]
    fn test_collection_deserialize_sdk_json() {
        let sdk_json = r#"{
            "id": 54321,
            "name": "explosions",
            "bus": 100,
            "attenuation": 0,
            "effect": 0,
            "gain": { "kind": "Static", "value": 0.8 },
            "priority": { "kind": "Static", "value": 128.0 },
            "fader": "Linear",
            "spatialization": "Position",
            "scope": "World",
            "play_mode": "PlayAll",
            "scheduler": { "mode": "Sequence" }
        }"#;

        let c: Collection = serde_json::from_str(sdk_json).unwrap();
        assert_eq!(c.id, 54321);
        assert_eq!(c.name.as_deref(), Some("explosions"));
        assert_eq!(c.bus, 100);
        assert_eq!(c.gain.as_ref().and_then(|g| g.as_static()), Some(0.8));
        assert_eq!(c.play_mode, CollectionPlayMode::PlayAll);
        assert_eq!(
            c.scheduler,
            Some(SoundSchedulerSettings {
                mode: SoundSchedulerMode::Sequence
            })
        );
        assert_eq!(c.spatialization, Spatialization::Position);
        assert_eq!(c.scope, Scope::World);
        assert_eq!(c.fader.as_deref(), Some("Linear"));
    }

    #[test]
    fn test_collection_validate_rules_passes_valid() {
        let context = ProjectContext::empty();
        let c = Collection::builder(1, "footsteps").gain(0.8).build();
        assert!(c.validate_rules(&context).is_ok());
    }

    #[test]
    fn test_collection_validate_rules_fails_empty_name() {
        let context = ProjectContext::empty();
        let mut c = Collection::builder(1, "test").build();
        c.name = Some(String::new());
        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
        assert_eq!(err.field, Some("name".to_string()));
    }

    #[test]
    fn test_collection_validate_rules_fails_none_name() {
        let context = ProjectContext::empty();
        let mut c = Collection::builder(1, "test").build();
        c.name = None;
        let result = c.validate_rules(&context);
        assert!(result.is_err());
    }

    #[test]
    fn test_collection_validate_rules_fails_invalid_gain() {
        let context = ProjectContext::empty();

        // gain > 1.0
        let c = Collection::builder(1, "test").gain(1.5).build();
        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Invalid gain value"));
        assert_eq!(err.field, Some("gain".to_string()));

        // gain < 0.0
        let c = Collection::builder(2, "test2").gain(-0.5).build();
        assert!(c.validate_rules(&context).is_err());
    }

    #[test]
    fn test_collection_validate_rules_fails_nan_gain() {
        let context = ProjectContext::empty();
        let c = Collection::builder(1, "test").gain(f32::NAN).build();
        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Invalid gain value"));
    }

    #[test]
    fn test_collection_validate_rules_fails_invalid_fader() {
        let context = ProjectContext::empty();
        let mut c = Collection::builder(1, "test").build();
        c.fader = Some("InvalidFader".to_string());
        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Unknown fader algorithm"));
        assert_eq!(err.field, Some("fader".to_string()));
    }

    #[test]
    fn test_collection_validate_rules_fails_none_scheduler() {
        let context = ProjectContext::empty();
        let mut c = Collection::builder(1, "test").build();
        c.scheduler = None;
        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no scheduler"));
        assert_eq!(err.field, Some("scheduler".to_string()));
    }
}
