//! Switch asset type.
//!
//! Placeholder implementation for Story 2-1.
//! Full implementation in Epic 4.

use serde::{Deserialize, Serialize};

use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// Switch state definitions.
///
/// Represents a set of states that can be used by switch containers
/// to control which sounds play based on game state.
/// Full implementation will be added in Epic 4.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Switch {
    /// Unique identifier for this switch.
    pub id: u64,
    /// Name of the switch.
    pub name: String,
}

impl Switch {
    /// Creates a new Switch with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl Asset for Switch {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
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

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 4
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_switch_new() {
        let switch = Switch::new(12345, "surface_type");
        assert_eq!(switch.id(), 12345);
        assert_eq!(switch.name(), "surface_type");
    }

    #[test]
    fn test_switch_asset_type() {
        let switch = Switch::new(1, "test");
        assert_eq!(switch.asset_type(), AssetType::Switch);
    }

    #[test]
    fn test_switch_file_extension() {
        let switch = Switch::new(1, "test");
        assert_eq!(switch.file_extension(), ".json");
    }

    #[test]
    fn test_switch_serde_roundtrip() {
        let switch = Switch::new(12345, "surface_type");
        let json = serde_json::to_string(&switch).unwrap();
        let parsed: Switch = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), switch.id());
        assert_eq!(parsed.name(), switch.name());
    }
}
