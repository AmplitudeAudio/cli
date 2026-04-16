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

//! Soundbank asset type.
//!
//! Soundbanks package multiple assets together for efficient runtime loading.
//! Each soundbank contains references to sounds, collections, events, switches,
//! switch containers, effects, attenuators, and RTPCs that should be loaded together.

use super::generated::SoundBankDefinition;
use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

// =============================================================================
// Soundbank Type Alias
// =============================================================================

/// Packaged audio assets for runtime loading.
///
/// Type alias to the build-time generated `SoundBankDefinition` from SDK FlatBuffer schemas.
/// Soundbanks group multiple assets together so the game can load/unload them
/// efficiently as a unit.
///
/// # Example
///
/// ```
/// use am::assets::Soundbank;
///
/// let soundbank = Soundbank::builder(12345, "main_bank")
///     .sound("sounds/explosion.sound.json")
///     .sound("sounds/footstep.sound.json")
///     .collection("collections/ambience.collection.json")
///     .build();
/// ```
pub type Soundbank = SoundBankDefinition;

impl Soundbank {
    /// Creates a builder for constructing a Soundbank with optional fields.
    pub fn builder(id: u64, name: impl Into<String>) -> SoundbankBuilder {
        SoundbankBuilder::new(id, name)
    }

    /// Returns the total count of all assets in this soundbank.
    pub fn asset_count(&self) -> usize {
        self.sounds.as_ref().map(|v| v.len()).unwrap_or(0)
            + self.collections.as_ref().map(|v| v.len()).unwrap_or(0)
            + self.events.as_ref().map(|v| v.len()).unwrap_or(0)
            + self.switches.as_ref().map(|v| v.len()).unwrap_or(0)
            + self
                .switch_containers
                .as_ref()
                .map(|v| v.len())
                .unwrap_or(0)
            + self.effects.as_ref().map(|v| v.len()).unwrap_or(0)
            + self.attenuators.as_ref().map(|v| v.len()).unwrap_or(0)
            + self.rtpc.as_ref().map(|v| v.len()).unwrap_or(0)
    }

    /// Returns true if this soundbank references the given asset ID.
    ///
    /// This is used to detect circular dependencies (a soundbank containing itself).
    /// Note: Soundbanks reference assets by path string, not ID, so this checks
    /// if the soundbank's own ID matches the given ID.
    pub fn contains_asset_id(&self, asset_id: u64) -> bool {
        self.id == asset_id
    }
}

// =============================================================================
// SoundbankBuilder
// =============================================================================

/// Builder for constructing Soundbank instances with optional fields.
#[derive(Debug, Clone)]
pub struct SoundbankBuilder {
    soundbank: Soundbank,
}

impl SoundbankBuilder {
    /// Creates a new SoundbankBuilder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            soundbank: Soundbank {
                id,
                name: Some(name.into()),
                sounds: None,
                collections: None,
                events: None,
                switches: None,
                switch_containers: None,
                effects: None,
                attenuators: None,
                rtpc: None,
            },
        }
    }

    /// Sets the name of the soundbank.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.soundbank.name = Some(name.into());
        self
    }

    /// Adds a sound reference to the soundbank.
    ///
    /// The reference should be a path string to the sound asset file.
    pub fn sound(mut self, sound_path: impl Into<String>) -> Self {
        let path = sound_path.into();
        if let Some(ref mut sounds) = self.soundbank.sounds {
            sounds.push(path);
        } else {
            self.soundbank.sounds = Some(vec![path]);
        }
        self
    }

    /// Adds multiple sound references to the soundbank.
    pub fn sounds(mut self, sound_paths: Vec<impl Into<String>>) -> Self {
        let paths: Vec<String> = sound_paths.into_iter().map(Into::into).collect();
        if let Some(ref mut sounds) = self.soundbank.sounds {
            sounds.extend(paths);
        } else {
            self.soundbank.sounds = Some(paths);
        }
        self
    }

    /// Adds a collection reference to the soundbank.
    pub fn collection(mut self, collection_path: impl Into<String>) -> Self {
        let path = collection_path.into();
        if let Some(ref mut collections) = self.soundbank.collections {
            collections.push(path);
        } else {
            self.soundbank.collections = Some(vec![path]);
        }
        self
    }

    /// Adds multiple collection references to the soundbank.
    pub fn collections(mut self, collection_paths: Vec<impl Into<String>>) -> Self {
        let paths: Vec<String> = collection_paths.into_iter().map(Into::into).collect();
        if let Some(ref mut collections) = self.soundbank.collections {
            collections.extend(paths);
        } else {
            self.soundbank.collections = Some(paths);
        }
        self
    }

    /// Adds an event reference to the soundbank.
    pub fn event(mut self, event_path: impl Into<String>) -> Self {
        let path = event_path.into();
        if let Some(ref mut events) = self.soundbank.events {
            events.push(path);
        } else {
            self.soundbank.events = Some(vec![path]);
        }
        self
    }

    /// Adds multiple event references to the soundbank.
    pub fn events(mut self, event_paths: Vec<impl Into<String>>) -> Self {
        let paths: Vec<String> = event_paths.into_iter().map(Into::into).collect();
        if let Some(ref mut events) = self.soundbank.events {
            events.extend(paths);
        } else {
            self.soundbank.events = Some(paths);
        }
        self
    }

    /// Adds a switch reference to the soundbank.
    pub fn switch(mut self, switch_path: impl Into<String>) -> Self {
        let path = switch_path.into();
        if let Some(ref mut switches) = self.soundbank.switches {
            switches.push(path);
        } else {
            self.soundbank.switches = Some(vec![path]);
        }
        self
    }

    /// Adds multiple switch references to the soundbank.
    pub fn switches(mut self, switch_paths: Vec<impl Into<String>>) -> Self {
        let paths: Vec<String> = switch_paths.into_iter().map(Into::into).collect();
        if let Some(ref mut switches) = self.soundbank.switches {
            switches.extend(paths);
        } else {
            self.soundbank.switches = Some(paths);
        }
        self
    }

    /// Adds a switch container reference to the soundbank.
    pub fn switch_container(mut self, container_path: impl Into<String>) -> Self {
        let path = container_path.into();
        if let Some(ref mut containers) = self.soundbank.switch_containers {
            containers.push(path);
        } else {
            self.soundbank.switch_containers = Some(vec![path]);
        }
        self
    }

    /// Adds multiple switch container references to the soundbank.
    pub fn switch_containers(mut self, container_paths: Vec<impl Into<String>>) -> Self {
        let paths: Vec<String> = container_paths.into_iter().map(Into::into).collect();
        if let Some(ref mut containers) = self.soundbank.switch_containers {
            containers.extend(paths);
        } else {
            self.soundbank.switch_containers = Some(paths);
        }
        self
    }

    /// Adds an effect reference to the soundbank.
    pub fn effect(mut self, effect_path: impl Into<String>) -> Self {
        let path = effect_path.into();
        if let Some(ref mut effects) = self.soundbank.effects {
            effects.push(path);
        } else {
            self.soundbank.effects = Some(vec![path]);
        }
        self
    }

    /// Adds multiple effect references to the soundbank.
    pub fn effects(mut self, effect_paths: Vec<impl Into<String>>) -> Self {
        let paths: Vec<String> = effect_paths.into_iter().map(Into::into).collect();
        if let Some(ref mut effects) = self.soundbank.effects {
            effects.extend(paths);
        } else {
            self.soundbank.effects = Some(paths);
        }
        self
    }

    /// Adds an attenuator reference to the soundbank.
    pub fn attenuator(mut self, attenuator_path: impl Into<String>) -> Self {
        let path = attenuator_path.into();
        if let Some(ref mut attenuators) = self.soundbank.attenuators {
            attenuators.push(path);
        } else {
            self.soundbank.attenuators = Some(vec![path]);
        }
        self
    }

    /// Adds multiple attenuator references to the soundbank.
    pub fn attenuators(mut self, attenuator_paths: Vec<impl Into<String>>) -> Self {
        let paths: Vec<String> = attenuator_paths.into_iter().map(Into::into).collect();
        if let Some(ref mut attenuators) = self.soundbank.attenuators {
            attenuators.extend(paths);
        } else {
            self.soundbank.attenuators = Some(paths);
        }
        self
    }

    /// Adds an RTPC reference to the soundbank.
    pub fn rtpc(mut self, rtpc_path: impl Into<String>) -> Self {
        let path = rtpc_path.into();
        if let Some(ref mut rtpc) = self.soundbank.rtpc {
            rtpc.push(path);
        } else {
            self.soundbank.rtpc = Some(vec![path]);
        }
        self
    }

    /// Adds multiple RTPC references to the soundbank.
    pub fn rtpcs(mut self, rtpc_paths: Vec<impl Into<String>>) -> Self {
        let paths: Vec<String> = rtpc_paths.into_iter().map(Into::into).collect();
        if let Some(ref mut rtpc) = self.soundbank.rtpc {
            rtpc.extend(paths);
        } else {
            self.soundbank.rtpc = Some(paths);
        }
        self
    }

    /// Builds the Soundbank instance.
    pub fn build(self) -> Soundbank {
        self.soundbank
    }
}

impl Asset for Soundbank {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Soundbank
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Soundbank.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 6
        Ok(())
    }

    fn validate_rules(&self, context: &ProjectContext) -> Result<(), ValidationError> {
        // Validate name is not empty
        if self.name.as_deref().unwrap_or("").is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Soundbank asset has no name",
                "Soundbank assets must have a non-empty name",
            )
            .with_suggestion("Set the 'name' field to a valid identifier (e.g., \"main_bank\")")
            .with_field("name"));
        }

        // Validate at least one asset is included
        let asset_count = self.asset_count();
        if asset_count == 0 {
            return Err(ValidationError::type_rule_violation(
                "Soundbank has no assets",
                "Soundbanks must contain at least one asset",
            )
            .with_suggestion(
                "Add at least one asset (sound, collection, event, etc.) to the soundbank",
            )
            .with_field("assets"));
        }

        // Check for circular dependency (soundbank containing itself)
        // Since soundbanks reference assets by path strings, we need to check
        // if this soundbank's ID appears as a reference in an unusual way.
        // This is a safety check - normally the soundbank wouldn't reference itself.
        // Note: The SDK uses path strings for references, so we can't directly
        // check for ID matches. This check would need path-to-ID resolution.

        // Validate all referenced assets exist (if validator is available).
        // Soundbank references use runtime binary names (e.g., "throw.amsound")
        // without a type directory prefix. We prepend the type directory so the
        // validator can locate the source file on disk.
        if let Some(ref validator) = context.validator {
            let mut missing_assets: Vec<String> = Vec::new();

            let checks: &[(&str, &Option<Vec<String>>)] = &[
                ("sounds", &self.sounds),
                ("collections", &self.collections),
                ("events", &self.events),
                ("switches", &self.switches),
                ("switch_containers", &self.switch_containers),
                ("effects", &self.effects),
                ("attenuators", &self.attenuators),
                ("rtpc", &self.rtpc),
            ];

            for &(type_dir, ref field) in checks {
                if let Some(paths) = field {
                    for asset_path in paths {
                        let full_path = format!("{}/{}", type_dir, asset_path);
                        if !validator.asset_exists_by_path(&full_path) {
                            missing_assets.push(format!("{}: {}", type_dir, asset_path));
                        }
                    }
                }
            }

            if !missing_assets.is_empty() {
                return Err(ValidationError::type_rule_violation(
                    format!(
                        "Soundbank references {} non-existent assets",
                        missing_assets.len()
                    ),
                    "All assets referenced by a soundbank must exist in the project",
                )
                .with_suggestion(format!(
                    "Check that the following assets exist: {}",
                    missing_assets.join(", ")
                ))
                .with_field("assets"));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soundbank_builder_basic() {
        let soundbank = Soundbank::builder(12345, "main_bank")
            .sound("sounds/explosion.sound.json")
            .collection("collections/ambience.collection.json")
            .build();

        assert_eq!(soundbank.id(), 12345);
        assert_eq!(soundbank.name(), "main_bank");
        assert_eq!(soundbank.sounds.as_ref().unwrap().len(), 1);
        assert_eq!(soundbank.collections.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_soundbank_builder_multiple_assets() {
        let soundbank = Soundbank::builder(1, "complex_bank")
            .sound("sounds/s1.json")
            .sound("sounds/s2.json")
            .event("events/e1.json")
            .switch("switches/sw1.json")
            .switch_container("containers/sc1.json")
            .effect("effects/fx1.json")
            .attenuator("attenuators/at1.json")
            .rtpc("rtpcs/r1.json")
            .build();

        assert_eq!(soundbank.id(), 1);
        assert_eq!(soundbank.asset_count(), 8);
        assert_eq!(soundbank.sounds.as_ref().unwrap().len(), 2);
        assert_eq!(soundbank.events.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_soundbank_builder_batch_methods() {
        let soundbank = Soundbank::builder(1, "batch_bank")
            .sounds(vec!["s1.json", "s2.json", "s3.json"])
            .collections(vec!["c1.json", "c2.json"])
            .events(vec!["e1.json"])
            .build();

        assert_eq!(soundbank.asset_count(), 6);
        assert_eq!(soundbank.sounds.as_ref().unwrap().len(), 3);
        assert_eq!(soundbank.collections.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_soundbank_builder_empty() {
        let soundbank = Soundbank::builder(12345, "empty_bank").build();

        assert_eq!(soundbank.id(), 12345);
        assert_eq!(soundbank.name(), "empty_bank");
        assert_eq!(soundbank.asset_count(), 0);
    }

    #[test]
    fn test_soundbank_asset_type() {
        let soundbank = Soundbank::builder(1, "test").sound("s.json").build();
        assert_eq!(soundbank.asset_type(), AssetType::Soundbank);
    }

    #[test]
    fn test_soundbank_file_extension() {
        let soundbank = Soundbank::builder(1, "test").build();
        assert_eq!(soundbank.file_extension(), ".json");
    }

    #[test]
    fn test_soundbank_serde_roundtrip() {
        let soundbank = Soundbank::builder(12345, "main_bank")
            .sounds(vec!["sounds/s1.json", "sounds/s2.json"])
            .collections(vec!["collections/c1.json"])
            .build();

        let json = serde_json::to_string(&soundbank).unwrap();
        let parsed: Soundbank = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, soundbank.id);
        assert_eq!(parsed.name, soundbank.name);
        assert_eq!(parsed.sounds, soundbank.sounds);
        assert_eq!(parsed.collections, soundbank.collections);
    }

    #[test]
    fn test_soundbank_serde_sdk_format() {
        let soundbank = Soundbank::builder(12345, "main_bank")
            .sound("sounds/explosion.sound.json")
            .collection("collections/ambience.collection.json")
            .build();

        let json = serde_json::to_string_pretty(&soundbank).unwrap();

        // Verify key field names are in SDK format
        assert!(json.contains("\"id\": 12345"));
        assert!(json.contains("\"name\": \"main_bank\""));
        assert!(json.contains("\"sounds\": ["));
        assert!(json.contains("\"collections\": ["));
        assert!(json.contains("sounds/explosion.sound.json"));
        assert!(json.contains("collections/ambience.collection.json"));
    }

    #[test]
    fn test_soundbank_deserialize_sdk_json() {
        let sdk_json = r#"{
            "id": 54321,
            "name": "level1_bank",
            "sounds": [
                "sounds/footstep.json",
                "sounds/jump.json"
            ],
            "events": [
                "events/play_music.json"
            ],
            "effects": [
                "effects/reverb.json"
            ]
        }"#;

        let soundbank: Soundbank = serde_json::from_str(sdk_json).unwrap();
        assert_eq!(soundbank.id, 54321);
        assert_eq!(soundbank.name.as_deref(), Some("level1_bank"));

        let sounds = soundbank.sounds.as_ref().unwrap();
        assert_eq!(sounds.len(), 2);
        assert_eq!(sounds[0], "sounds/footstep.json");

        let events = soundbank.events.as_ref().unwrap();
        assert_eq!(events.len(), 1);

        let effects = soundbank.effects.as_ref().unwrap();
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn test_soundbank_validate_rules_passes_valid() {
        let context = ProjectContext::empty();
        let soundbank = Soundbank::builder(1, "test_bank").sound("s.json").build();

        assert!(soundbank.validate_rules(&context).is_ok());
    }

    #[test]
    fn test_soundbank_validate_rules_fails_empty_name() {
        let context = ProjectContext::empty();
        let soundbank = Soundbank {
            id: 1,
            name: Some("".to_string()),
            sounds: Some(vec!["s.json".to_string()]),
            collections: None,
            events: None,
            switches: None,
            switch_containers: None,
            effects: None,
            attenuators: None,
            rtpc: None,
        };

        let result = soundbank.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
        assert_eq!(err.field, Some("name".to_string()));
    }

    #[test]
    fn test_soundbank_validate_rules_fails_no_assets() {
        let context = ProjectContext::empty();
        let soundbank = Soundbank::builder(1, "empty_bank").build();

        let result = soundbank.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("no assets"));
        assert_eq!(err.field, Some("assets".to_string()));
    }

    #[test]
    fn test_soundbank_contains_asset_id() {
        let soundbank = Soundbank::builder(12345, "test").sound("s.json").build();
        assert!(soundbank.contains_asset_id(12345));
        assert!(!soundbank.contains_asset_id(99999));
    }

    #[test]
    fn test_soundbank_asset_count() {
        let soundbank = Soundbank::builder(1, "count_test")
            .sounds(vec!["s1.json", "s2.json"])
            .collections(vec!["c1.json"])
            .events(vec!["e1.json", "e2.json", "e3.json"])
            .build();

        assert_eq!(soundbank.asset_count(), 6);
    }
}
