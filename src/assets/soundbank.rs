//! Soundbank asset type.
//!
//! Placeholder implementation for Story 2-1.
//! Full implementation in Epic 5.

use serde::{Deserialize, Serialize};

use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// Packaged audio assets for runtime.
///
/// Represents a collection of sounds and other assets that are
/// packaged together for efficient loading at runtime.
/// Full implementation will be added in Epic 5.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Soundbank {
    /// Unique identifier for this soundbank.
    pub id: u64,
    /// Name of the soundbank.
    pub name: String,
}

impl Soundbank {
    /// Creates a new Soundbank with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl Asset for Soundbank {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
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

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 5
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soundbank_new() {
        let soundbank = Soundbank::new(12345, "main_bank");
        assert_eq!(soundbank.id(), 12345);
        assert_eq!(soundbank.name(), "main_bank");
    }

    #[test]
    fn test_soundbank_asset_type() {
        let soundbank = Soundbank::new(1, "test");
        assert_eq!(soundbank.asset_type(), AssetType::Soundbank);
    }

    #[test]
    fn test_soundbank_file_extension() {
        let soundbank = Soundbank::new(1, "test");
        assert_eq!(soundbank.file_extension(), ".json");
    }

    #[test]
    fn test_soundbank_serde_roundtrip() {
        let soundbank = Soundbank::new(12345, "main_bank");
        let json = serde_json::to_string(&soundbank).unwrap();
        let parsed: Soundbank = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), soundbank.id());
        assert_eq!(parsed.name(), soundbank.name());
    }
}
