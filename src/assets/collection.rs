//! Collection asset type.
//!
//! Placeholder implementation for Story 2-1.
//! Full implementation in Epic 3.

use serde::{Deserialize, Serialize};

use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// Grouped sound variations.
///
/// Represents a collection of related sounds that can be played
/// with variation (random selection, sequential, etc.).
/// Full implementation will be added in Epic 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    /// Unique identifier for this collection.
    pub id: u64,
    /// Name of the collection.
    pub name: String,
}

impl Collection {
    /// Creates a new Collection with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl Asset for Collection {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
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

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 3
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collection_new() {
        let collection = Collection::new(12345, "footsteps");
        assert_eq!(collection.id(), 12345);
        assert_eq!(collection.name(), "footsteps");
    }

    #[test]
    fn test_collection_asset_type() {
        let collection = Collection::new(1, "test");
        assert_eq!(collection.asset_type(), AssetType::Collection);
    }

    #[test]
    fn test_collection_file_extension() {
        let collection = Collection::new(1, "test");
        assert_eq!(collection.file_extension(), ".json");
    }

    #[test]
    fn test_collection_serde_roundtrip() {
        let collection = Collection::new(12345, "footsteps");
        let json = serde_json::to_string(&collection).unwrap();
        let parsed: Collection = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), collection.id());
        assert_eq!(parsed.name(), collection.name());
    }
}
