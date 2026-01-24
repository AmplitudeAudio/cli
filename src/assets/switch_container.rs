//! Switch Container asset type.
//!
//! Placeholder implementation for Story 2-1.
//! Full implementation in Epic 4.

use serde::{Deserialize, Serialize};

use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// State-based sound switching.
///
/// Represents a container that switches between different sounds
/// based on the current value of a switch.
/// Full implementation will be added in Epic 4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchContainer {
    /// Unique identifier for this switch container.
    pub id: u64,
    /// Name of the switch container.
    pub name: String,
}

impl SwitchContainer {
    /// Creates a new SwitchContainer with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl Asset for SwitchContainer {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
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

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 4
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_switch_container_new() {
        let container = SwitchContainer::new(12345, "footstep_surface");
        assert_eq!(container.id(), 12345);
        assert_eq!(container.name(), "footstep_surface");
    }

    #[test]
    fn test_switch_container_asset_type() {
        let container = SwitchContainer::new(1, "test");
        assert_eq!(container.asset_type(), AssetType::SwitchContainer);
    }

    #[test]
    fn test_switch_container_file_extension() {
        let container = SwitchContainer::new(1, "test");
        assert_eq!(container.file_extension(), ".json");
    }

    #[test]
    fn test_switch_container_serde_roundtrip() {
        let container = SwitchContainer::new(12345, "footstep_surface");
        let json = serde_json::to_string(&container).unwrap();
        let parsed: SwitchContainer = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), container.id());
        assert_eq!(parsed.name(), container.name());
    }
}
