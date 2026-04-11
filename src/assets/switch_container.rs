//! Switch Container asset type.
//!
//! Represents a container that maps switch states to sounds, enabling dynamic audio
//! that changes based on game state (e.g., different footstep sounds for different
//! surfaces like wood, stone, grass) for the Amplitude Audio SDK.

use super::generated::{
    FadeTransitionSettings, RtpcCompatibleValue, Scope, Spatialization, SwitchContainerDefinition,
    SwitchContainerEntry, SwitchContainerUpdateBehavior,
};
use super::{Asset, AssetType, ProjectContext, Schema, ValidationError, ValidationLayer};

// =============================================================================
// SwitchContainer Type Alias
// =============================================================================

/// State-based sound switching container.
///
/// Type alias to the build-time generated `SwitchContainerDefinition` from SDK FlatBuffer schemas.
/// Represents a container that maps switch states to audio assets (sounds or collections).
///
/// # Example
///
/// ```
/// use am::assets::SwitchContainer;
///
/// let container = SwitchContainer::builder(12345, "footstep_surface")
///     .switch_group(100)
///     .default_state(1)
///     .entry(SwitchContainerEntry {
///         object: 200, // Sound ID
///         switch_states: vec![1, 2], // Wood and Stone states
///         continue_between_states: false,
///         fade_in: None,
///         fade_out: None,
///         gain: None,
///         pitch: None,
///     })
///     .build();
/// ```
pub type SwitchContainer = SwitchContainerDefinition;

/// ID value meaning "no reference" in the SDK.
const NO_REFERENCE: u64 = 0;

impl SwitchContainer {
    /// Creates a builder for constructing a SwitchContainer with optional fields.
    pub fn builder(id: u64, name: impl Into<String>) -> SwitchContainerBuilder {
        SwitchContainerBuilder::new(id, name)
    }
}

// =============================================================================
// SwitchContainerBuilder
// =============================================================================

/// Default gain for a new switch container (full volume).
const DEFAULT_GAIN: f32 = 1.0;
/// Default priority for a new switch container (mid-range).
const DEFAULT_PRIORITY: f32 = 128.0;

/// Builder for constructing SwitchContainer instances with optional fields.
#[derive(Debug, Clone)]
pub struct SwitchContainerBuilder {
    container: SwitchContainer,
}

impl SwitchContainerBuilder {
    /// Creates a new SwitchContainerBuilder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            container: SwitchContainer {
                id,
                name: Some(name.into()),
                bus: NO_REFERENCE,
                effect: NO_REFERENCE,
                attenuation: NO_REFERENCE,
                gain: Some(RtpcCompatibleValue::static_value(DEFAULT_GAIN)),
                priority: Some(RtpcCompatibleValue::static_value(DEFAULT_PRIORITY)),
                pitch: None,
                fader: None,
                spatialization: Spatialization::None,
                scope: Scope::World,
                switch_group: NO_REFERENCE,
                default_switch_state: NO_REFERENCE,
                entries: Some(Vec::new()),
                update_behavior: SwitchContainerUpdateBehavior::UpdateOnPlay,
            },
        }
    }

    /// Sets the switch group (controlling switch) ID.
    pub fn switch_group(mut self, id: u64) -> Self {
        self.container.switch_group = id;
        self
    }

    /// Sets the default switch state ID.
    pub fn default_state(mut self, id: u64) -> Self {
        self.container.default_switch_state = id;
        self
    }

    /// Adds an entry to the container.
    pub fn entry(mut self, entry: SwitchContainerEntry) -> Self {
        self.container
            .entries
            .get_or_insert_with(Vec::new)
            .push(entry);
        self
    }

    /// Sets all entries at once.
    pub fn entries(mut self, entries: Vec<SwitchContainerEntry>) -> Self {
        self.container.entries = Some(entries);
        self
    }

    /// Sets the gain as a static value.
    pub fn gain(mut self, value: f32) -> Self {
        self.container.gain = Some(RtpcCompatibleValue::static_value(value));
        self
    }

    /// Sets the priority as a static value.
    pub fn priority(mut self, value: u8) -> Self {
        self.container.priority = Some(RtpcCompatibleValue::static_value(value as f32));
        self
    }

    /// Sets the bus ID.
    pub fn bus(mut self, bus_id: u64) -> Self {
        self.container.bus = bus_id;
        self
    }

    /// Sets the effect ID.
    pub fn effect(mut self, effect_id: u64) -> Self {
        self.container.effect = effect_id;
        self
    }

    /// Sets the attenuation ID.
    pub fn attenuation(mut self, attenuation_id: u64) -> Self {
        self.container.attenuation = attenuation_id;
        self
    }

    /// Sets the spatialization mode.
    pub fn spatialization(mut self, spatialization: Spatialization) -> Self {
        self.container.spatialization = spatialization;
        self
    }

    /// Sets the scope.
    pub fn scope(mut self, scope: Scope) -> Self {
        self.container.scope = scope;
        self
    }

    /// Sets the update behavior.
    pub fn update_behavior(mut self, behavior: SwitchContainerUpdateBehavior) -> Self {
        self.container.update_behavior = behavior;
        self
    }

    /// Builds the SwitchContainer instance.
    pub fn build(self) -> SwitchContainer {
        self.container
    }
}

// =============================================================================
// Asset Trait Implementation
// =============================================================================

impl Asset for SwitchContainer {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn asset_type(&self) -> AssetType {
        AssetType::SwitchContainer
    }

    fn file_extension(&self) -> &'static str {
        AssetType::SwitchContainer.file_extension()
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
                "SwitchContainer asset has no name",
                "SwitchContainer assets must have a non-empty name",
            )
            .with_suggestion(
                "Set the 'name' field to a valid identifier (e.g., \"footstep_surface\")",
            )
            .with_field("name"));
        }

        // Validate container ID is not zero
        if self.id == 0 {
            return Err(ValidationError::type_rule_violation(
                "SwitchContainer ID cannot be zero",
                "SwitchContainer assets must have a non-zero ID",
            )
            .with_suggestion("Use a unique non-zero ID for the switch container")
            .with_field("id"));
        }

        // Validate switch_group is set (required reference)
        if self.switch_group == NO_REFERENCE {
            return Err(ValidationError::type_rule_violation(
                "SwitchContainer has no controlling switch",
                "SwitchContainer assets must reference a controlling switch",
            )
            .with_suggestion("Set the 'switch_group' field to a valid Switch ID")
            .with_field("switch_group"));
        }

        // If we have a validator, perform cross-asset reference validation
        if let Some(validator) = &context.validator {
            // Validate the controlling switch exists
            validator.validate_switch_exists(self.switch_group)?;

            // Validate default_switch_state is set
            if self.default_switch_state == NO_REFERENCE {
                return Err(ValidationError::type_rule_violation(
                    "SwitchContainer has no default state",
                    "SwitchContainer assets must specify a default switch state",
                )
                .with_suggestion(
                    "Set the 'default_switch_state' field to one of the switch's state IDs",
                )
                .with_field("default_switch_state"));
            }

            // Get entries list (treat None as empty)
            let entries = self.entries.as_deref().unwrap_or(&[]);

            // Collect all unique state IDs referenced by entries for validation
            let mut referenced_state_ids: Vec<u64> = Vec::new();

            for (idx, entry) in entries.iter().enumerate() {
                // Validate the entry's object (sound or collection) exists
                // Try as sound first, then collection
                let sound_result = validator.validate_sound_exists(entry.object);
                let collection_result = validator.validate_collection_exists(entry.object);

                if sound_result.is_err() && collection_result.is_err() {
                    return Err(ValidationError::type_rule_violation(
                        format!("Entry {} references non-existent audio asset with ID {}", idx, entry.object),
                        "SwitchContainer entries must reference existing Sound or Collection assets",
                    )
                    .with_suggestion(format!(
                        "Create a Sound or Collection with ID {} first, or use an existing valid ID",
                        entry.object
                    ))
                    .with_field(format!("entries[{}].object", idx)));
                }

                // Collect state IDs for this entry
                for state_id in &entry.switch_states {
                    referenced_state_ids.push(*state_id);
                }
            }

            // Validate that default_switch_state is referenced by at least one entry
            // (This ensures the default state has a sound mapped to it)
            if !referenced_state_ids.contains(&self.default_switch_state) {
                // This is a warning-level issue, not a hard error
                // The container will play nothing when in the default state
                // For now, we'll allow it but could add stricter validation later
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
    fn test_switch_container_builder_basic() {
        let container = SwitchContainer::builder(12345, "footstep_surface").build();
        assert_eq!(container.id(), 12345);
        assert_eq!(container.name(), "footstep_surface");
    }

    #[test]
    fn test_switch_container_builder_defaults() {
        let c = SwitchContainer::builder(1, "test").build();
        assert_eq!(c.id, 1);
        assert_eq!(c.name.as_deref(), Some("test"));
        assert_eq!(c.switch_group, NO_REFERENCE);
        assert_eq!(c.default_switch_state, NO_REFERENCE);
        assert_eq!(c.bus, NO_REFERENCE);
        assert_eq!(c.effect, NO_REFERENCE);
        assert_eq!(c.attenuation, NO_REFERENCE);
        assert_eq!(c.entries, Some(Vec::new()));
    }

    #[test]
    fn test_switch_container_builder_with_switch_group() {
        let c = SwitchContainer::builder(1, "test")
            .switch_group(100)
            .default_state(1)
            .build();

        assert_eq!(c.switch_group, 100);
        assert_eq!(c.default_switch_state, 1);
    }

    #[test]
    fn test_switch_container_builder_with_entry() {
        let entry = SwitchContainerEntry {
            object: 200,
            switch_states: vec![1, 2],
            continue_between_states: false,
            fade_in: None,
            fade_out: None,
            gain: None,
            pitch: None,
        };

        let c = SwitchContainer::builder(1, "test").entry(entry).build();

        let entries = c.entries.as_ref().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].object, 200);
        assert_eq!(entries[0].switch_states, vec![1, 2]);
    }

    #[test]
    fn test_switch_container_builder_set_entries() {
        let entries = vec![
            SwitchContainerEntry {
                object: 201,
                switch_states: vec![1],
                continue_between_states: false,
                fade_in: None,
                fade_out: None,
                gain: None,
                pitch: None,
            },
            SwitchContainerEntry {
                object: 202,
                switch_states: vec![2],
                continue_between_states: false,
                fade_in: None,
                fade_out: None,
                gain: None,
                pitch: None,
            },
        ];

        let c = SwitchContainer::builder(1, "test").entries(entries).build();
        let result_entries = c.entries.as_ref().unwrap();
        assert_eq!(result_entries.len(), 2);
        assert_eq!(result_entries[0].object, 201);
        assert_eq!(result_entries[1].object, 202);
    }

    // =========================================================================
    // Asset Trait Tests (P0)
    // =========================================================================

    #[test]
    fn test_switch_container_asset_type() {
        let c = SwitchContainer::builder(1, "test").build();
        assert_eq!(c.asset_type(), AssetType::SwitchContainer);
    }

    #[test]
    fn test_switch_container_file_extension() {
        let c = SwitchContainer::builder(1, "test").build();
        assert_eq!(c.file_extension(), ".json");
    }

    // =========================================================================
    // Serde Tests (P0)
    // =========================================================================

    #[test]
    fn test_switch_container_serde_roundtrip() {
        let c = SwitchContainer::builder(12345, "footstep_surface")
            .switch_group(100)
            .default_state(1)
            .entry(SwitchContainerEntry {
                object: 200,
                switch_states: vec![1, 2],
                continue_between_states: false,
                fade_in: None,
                fade_out: None,
                gain: None,
                pitch: None,
            })
            .build();

        let json = serde_json::to_string(&c).unwrap();
        let parsed: SwitchContainer = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, c.id);
        assert_eq!(parsed.name, c.name);
        assert_eq!(parsed.switch_group, c.switch_group);
        assert_eq!(parsed.default_switch_state, c.default_switch_state);
        assert_eq!(parsed.entries, c.entries);
    }

    #[test]
    fn test_switch_container_deserialize_sdk_json() {
        let sdk_json = r#"{
            "id": 54321,
            "name": "footsteps_by_surface",
            "switch_group": 100,
            "default_switch_state": 1,
            "entries": [
                {
                    "object": 200,
                    "switch_states": [1],
                    "continue_between_states": false
                },
                {
                    "object": 201,
                    "switch_states": [2],
                    "continue_between_states": false
                }
            ],
            "bus": 0,
            "effect": 0,
            "attenuation": 0,
            "scope": "World",
            "spatialization": "None",
            "update_behavior": "UpdateOnPlay"
        }"#;

        let c: SwitchContainer = serde_json::from_str(sdk_json).unwrap();
        assert_eq!(c.id, 54321);
        assert_eq!(c.name.as_deref(), Some("footsteps_by_surface"));
        assert_eq!(c.switch_group, 100);
        assert_eq!(c.default_switch_state, 1);

        let entries = c.entries.as_ref().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].object, 200);
        assert_eq!(entries[0].switch_states, vec![1]);
        assert_eq!(entries[1].object, 201);
        assert_eq!(entries[1].switch_states, vec![2]);
    }

    #[test]
    fn test_switch_container_serialize_sdk_format() {
        let c = SwitchContainer::builder(12345, "footstep_surface")
            .switch_group(100)
            .default_state(1)
            .build();

        let json = serde_json::to_string_pretty(&c).unwrap();

        assert!(json.contains("\"id\": 12345"));
        assert!(json.contains("\"name\": \"footstep_surface\""));
        assert!(json.contains("\"switch_group\": 100"));
        assert!(json.contains("\"default_switch_state\": 1"));
    }

    // =========================================================================
    // Validation Tests (P1)
    // =========================================================================

    #[test]
    fn test_switch_container_validate_rules_passes_valid() {
        let context = ProjectContext::empty();
        let c = SwitchContainer::builder(1, "footstep_surface")
            .switch_group(100)
            .default_state(1)
            .entry(SwitchContainerEntry {
                object: 200,
                switch_states: vec![1],
                continue_between_states: false,
                fade_in: None,
                fade_out: None,
                gain: None,
                pitch: None,
            })
            .build();

        // Without validator, only basic validation runs (name, id, switch_group presence)
        // The reference validation is skipped
        assert!(c.validate_rules(&context).is_ok());
    }

    #[test]
    fn test_switch_container_validate_rules_fails_empty_name() {
        let context = ProjectContext::empty();
        let mut c = SwitchContainer::builder(1, "test")
            .switch_group(100)
            .default_state(1)
            .build();
        c.name = Some(String::new());

        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
        assert_eq!(err.field, Some("name".to_string()));
    }

    #[test]
    fn test_switch_container_validate_rules_fails_none_name() {
        let context = ProjectContext::empty();
        let mut c = SwitchContainer::builder(1, "test")
            .switch_group(100)
            .default_state(1)
            .build();
        c.name = None;

        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
    }

    #[test]
    fn test_switch_container_validate_rules_fails_zero_id() {
        let context = ProjectContext::empty();
        let c = SwitchContainer::builder(0, "test")
            .switch_group(100)
            .default_state(1)
            .build();

        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("ID cannot be zero"));
        assert_eq!(err.field, Some("id".to_string()));
    }

    #[test]
    fn test_switch_container_validate_rules_fails_no_switch_group() {
        let context = ProjectContext::empty();
        let c = SwitchContainer::builder(1, "test").default_state(1).build();

        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no controlling switch"));
        assert_eq!(err.field, Some("switch_group".to_string()));
    }

    #[test]
    fn test_switch_container_validate_rules_fails_whitespace_only_name() {
        let context = ProjectContext::empty();
        let mut c = SwitchContainer::builder(1, "test")
            .switch_group(100)
            .default_state(1)
            .build();
        c.name = Some("   ".to_string());

        let result = c.validate_rules(&context);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
    }
}
