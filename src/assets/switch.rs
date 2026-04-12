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

//! Switch asset type.
//!
//! Represents a set of states that can be used by switch containers
//! to control which sounds play based on game state (e.g., surface type,
//! weather, time of day) for the Amplitude Audio SDK.

use super::generated::{SwitchDefinition, SwitchStateDefinition};
use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

// =============================================================================
// Switch Type Alias
// =============================================================================

/// Switch state definitions.
///
/// Type alias to the build-time generated `SwitchDefinition` from SDK FlatBuffer schemas.
/// Represents a set of named states for dynamic audio selection at runtime.
///
/// # Example
///
/// ```
/// use am::assets::Switch;
///
/// let switch = Switch::builder(12345, "surface_type")
///     .state(1, "wood")
///     .state(2, "stone")
///     .state(3, "metal")
///     .build();
/// ```
pub type Switch = SwitchDefinition;

impl Switch {
    /// Creates a builder for constructing a Switch with optional fields.
    pub fn builder(id: u64, name: impl Into<String>) -> SwitchBuilder {
        SwitchBuilder::new(id, name)
    }
}

// =============================================================================
// SwitchBuilder
// =============================================================================

/// Builder for constructing Switch instances with optional fields.
#[derive(Debug, Clone)]
pub struct SwitchBuilder {
    switch: Switch,
}

impl SwitchBuilder {
    /// Creates a new SwitchBuilder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            switch: Switch {
                id,
                name: Some(name.into()),
                states: Some(Vec::new()),
            },
        }
    }

    /// Adds a state to the switch.
    pub fn state(mut self, id: u64, name: impl Into<String>) -> Self {
        self.switch
            .states
            .get_or_insert_with(Vec::new)
            .push(SwitchStateDefinition {
                id,
                name: Some(name.into()),
            });
        self
    }

    /// Sets all states at once.
    pub fn states(mut self, states: Vec<SwitchStateDefinition>) -> Self {
        self.switch.states = Some(states);
        self
    }

    /// Builds the Switch instance.
    pub fn build(self) -> Switch {
        self.switch
    }
}

// =============================================================================
// Asset Trait Implementation
// =============================================================================

impl Asset for Switch {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Switch
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Switch.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 6
        Ok(())
    }

    fn validate_rules(&self, context: &ProjectContext) -> Result<(), ValidationError> {
        // Validate name is not empty or whitespace-only
        let name = self.name.as_deref().unwrap_or("").trim();
        if name.is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Switch asset has no name",
                "Switch assets must have a non-empty name",
            )
            .with_suggestion("Set the 'name' field to a valid identifier (e.g., \"surface_type\")")
            .with_field("name"));
        }

        // Validate switch ID is not zero
        if self.id == 0 {
            return Err(ValidationError::type_rule_violation(
                "Switch ID cannot be zero",
                "Switch assets must have a non-zero ID",
            )
            .with_suggestion("Use a unique non-zero ID for the switch")
            .with_field("id"));
        }

        // Get states list (treat None as empty)
        let states = self.states.as_deref().unwrap_or(&[]);

        // Validate at least one state is defined
        if states.is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Switch must have at least one state",
                "Switch assets require at least one state definition",
            )
            .with_suggestion("Add at least one state to the switch (e.g., 'wood', 'stone')")
            .with_field("states"));
        }

        // Validate state names and IDs
        let mut seen_names = std::collections::HashSet::new();
        let mut seen_ids = std::collections::HashSet::new();
        for state in states {
            // Validate state ID is not zero
            if state.id == 0 {
                return Err(ValidationError::type_rule_violation(
                    "State ID cannot be zero",
                    "All switch states must have a non-zero ID",
                )
                .with_suggestion("Use a unique non-zero ID for each state")
                .with_field("states"));
            }

            // Validate state name is not empty or whitespace-only
            let state_name = state.name.as_deref().unwrap_or("").trim();
            if state_name.is_empty() {
                return Err(ValidationError::type_rule_violation(
                    "Switch state has no name",
                    "All switch states must have a non-empty name",
                )
                .with_suggestion("Set the 'name' field on each state (e.g., 'wood', 'stone')")
                .with_field("states"));
            }

            // Validate state name uniqueness
            if !seen_names.insert(state_name) {
                return Err(ValidationError::type_rule_violation(
                    format!("Duplicate state name: '{}'", state_name),
                    "Each state name must be unique within the switch",
                )
                .with_suggestion("Rename one of the duplicate states to a unique name")
                .with_field("states"));
            }

            // Validate state ID uniqueness
            if !seen_ids.insert(state.id) {
                return Err(ValidationError::type_rule_violation(
                    format!("Duplicate state ID: {}", state.id),
                    "Each state ID must be unique within the switch",
                )
                .with_suggestion("Use a unique ID for each state")
                .with_field("states"));
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
    fn test_switch_builder_basic() {
        let switch = Switch::builder(12345, "surface_type").build();
        assert_eq!(switch.id(), 12345);
        assert_eq!(switch.name(), "surface_type");
    }

    #[test]
    fn test_switch_builder_defaults() {
        let s = Switch::builder(1, "test").build();
        assert_eq!(s.id, 1);
        assert_eq!(s.name.as_deref(), Some("test"));
        assert_eq!(s.states, Some(Vec::new()));
    }

    #[test]
    fn test_switch_builder_with_states() {
        let s = Switch::builder(12345, "surface_type")
            .state(1, "wood")
            .state(2, "stone")
            .state(3, "metal")
            .build();

        assert_eq!(s.id, 12345);
        assert_eq!(s.name.as_deref(), Some("surface_type"));
        let states = s.states.as_ref().unwrap();
        assert_eq!(states.len(), 3);
        assert_eq!(states[0].id, 1);
        assert_eq!(states[0].name.as_deref(), Some("wood"));
        assert_eq!(states[1].id, 2);
        assert_eq!(states[1].name.as_deref(), Some("stone"));
        assert_eq!(states[2].id, 3);
        assert_eq!(states[2].name.as_deref(), Some("metal"));
    }

    #[test]
    fn test_switch_builder_set_states() {
        let states = vec![
            SwitchStateDefinition {
                id: 10,
                name: Some("day".to_string()),
            },
            SwitchStateDefinition {
                id: 20,
                name: Some("night".to_string()),
            },
        ];
        let s = Switch::builder(1, "time_of_day").states(states).build();
        let result_states = s.states.as_ref().unwrap();
        assert_eq!(result_states.len(), 2);
        assert_eq!(result_states[0].name.as_deref(), Some("day"));
        assert_eq!(result_states[1].name.as_deref(), Some("night"));
    }

    // =========================================================================
    // Asset Trait Tests (P0)
    // =========================================================================

    #[test]
    fn test_switch_asset_type() {
        let s = Switch::builder(1, "test").build();
        assert_eq!(s.asset_type(), AssetType::Switch);
    }

    #[test]
    fn test_switch_file_extension() {
        let s = Switch::builder(1, "test").build();
        assert_eq!(s.file_extension(), ".json");
    }

    // =========================================================================
    // Serde Tests (P0)
    // =========================================================================

    #[test]
    fn test_switch_serde_roundtrip() {
        let s = Switch::builder(12345, "surface_type")
            .state(1, "wood")
            .state(2, "stone")
            .state(3, "metal")
            .build();

        let json = serde_json::to_string(&s).unwrap();
        let parsed: Switch = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, s.id);
        assert_eq!(parsed.name, s.name);
        assert_eq!(parsed.states, s.states);
    }

    #[test]
    fn test_switch_deserialize_sdk_json() {
        let sdk_json = r#"{
            "id": 54321,
            "name": "weather",
            "states": [
                { "id": 1, "name": "sunny" },
                { "id": 2, "name": "rainy" },
                { "id": 3, "name": "stormy" }
            ]
        }"#;

        let s: Switch = serde_json::from_str(sdk_json).unwrap();
        assert_eq!(s.id, 54321);
        assert_eq!(s.name.as_deref(), Some("weather"));
        let states = s.states.as_ref().unwrap();
        assert_eq!(states.len(), 3);
        assert_eq!(states[0].id, 1);
        assert_eq!(states[0].name.as_deref(), Some("sunny"));
        assert_eq!(states[1].id, 2);
        assert_eq!(states[1].name.as_deref(), Some("rainy"));
        assert_eq!(states[2].id, 3);
        assert_eq!(states[2].name.as_deref(), Some("stormy"));
    }

    #[test]
    fn test_switch_serialize_sdk_format() {
        let s = Switch::builder(12345, "surface_type")
            .state(1, "wood")
            .state(2, "stone")
            .build();

        let json = serde_json::to_string_pretty(&s).unwrap();

        assert!(json.contains("\"id\": 12345"));
        assert!(json.contains("\"name\": \"surface_type\""));
        assert!(json.contains("\"name\": \"wood\""));
        assert!(json.contains("\"name\": \"stone\""));
        assert!(json.contains("\"states\""));
    }

    // =========================================================================
    // Validation Tests (P1)
    // =========================================================================

    #[test]
    fn test_switch_validate_rules_passes_valid() {
        let context = ProjectContext::empty();
        let s = Switch::builder(1, "surface_type")
            .state(1, "wood")
            .state(2, "stone")
            .build();
        assert!(s.validate_rules(&context).is_ok());
    }

    #[test]
    fn test_switch_validate_rules_fails_empty_name() {
        let context = ProjectContext::empty();
        let mut s = Switch::builder(1, "test").state(1, "wood").build();
        s.name = Some(String::new());
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
        assert_eq!(err.field, Some("name".to_string()));
    }

    #[test]
    fn test_switch_validate_rules_fails_none_name() {
        let context = ProjectContext::empty();
        let mut s = Switch::builder(1, "test").state(1, "wood").build();
        s.name = None;
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
    }

    #[test]
    fn test_switch_validate_rules_fails_empty_states() {
        let context = ProjectContext::empty();
        let s = Switch::builder(1, "surface_type").build();
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("at least one state"));
        assert_eq!(err.field, Some("states".to_string()));
    }

    #[test]
    fn test_switch_validate_rules_fails_none_states() {
        let context = ProjectContext::empty();
        let mut s = Switch::builder(1, "surface_type").state(1, "wood").build();
        s.states = None;
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("at least one state"));
    }

    #[test]
    fn test_switch_validate_rules_fails_duplicate_state_names() {
        let context = ProjectContext::empty();
        let s = Switch::builder(1, "surface_type")
            .state(1, "wood")
            .state(2, "wood")
            .build();
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Duplicate state name: 'wood'"));
        assert_eq!(err.field, Some("states".to_string()));
    }

    #[test]
    fn test_switch_validate_rules_fails_duplicate_state_ids() {
        let context = ProjectContext::empty();
        let s = Switch::builder(1, "surface_type")
            .state(1, "wood")
            .state(1, "stone")
            .build();
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Duplicate state ID: 1"));
        assert_eq!(err.field, Some("states".to_string()));
    }

    #[test]
    fn test_switch_validate_rules_fails_empty_state_name() {
        let context = ProjectContext::empty();
        let s = Switch::builder(1, "surface_type").state(1, "").build();
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("state has no name"));
        assert_eq!(err.field, Some("states".to_string()));
    }

    #[test]
    fn test_switch_validate_rules_fails_none_state_name() {
        let context = ProjectContext::empty();
        let mut s = Switch::builder(1, "surface_type").state(1, "wood").build();
        s.states.as_mut().unwrap()[0].name = None;
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("state has no name"));
    }

    #[test]
    fn test_switch_validate_rules_fails_whitespace_only_name() {
        let context = ProjectContext::empty();
        let mut s = Switch::builder(1, "test").state(1, "wood").build();
        s.name = Some("   ".to_string());
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
    }

    #[test]
    fn test_switch_validate_rules_fails_whitespace_only_state_name() {
        let context = ProjectContext::empty();
        let s = Switch::builder(1, "surface_type")
            .state(1, "  \t  ")
            .build();
        let result = s.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("state has no name"));
    }
}
