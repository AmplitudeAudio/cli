//! Effect asset type.
//!
//! Placeholder implementation for Story 2-1.
//! Full implementation in Epic 3.

use serde::{Deserialize, Serialize};

use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// Audio effects (reverb, EQ, etc.).
///
/// Represents an audio effect that can be applied to sounds,
/// collections, or buses to modify their audio characteristics.
/// Full implementation will be added in Epic 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    /// Unique identifier for this effect.
    pub id: u64,
    /// Name of the effect.
    pub name: String,
}

impl Effect {
    /// Creates a new Effect with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl Asset for Effect {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
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
        // Placeholder - full implementation in Epic 3
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_new() {
        let effect = Effect::new(12345, "reverb_large_hall");
        assert_eq!(effect.id(), 12345);
        assert_eq!(effect.name(), "reverb_large_hall");
    }

    #[test]
    fn test_effect_asset_type() {
        let effect = Effect::new(1, "test");
        assert_eq!(effect.asset_type(), AssetType::Effect);
    }

    #[test]
    fn test_effect_file_extension() {
        let effect = Effect::new(1, "test");
        assert_eq!(effect.file_extension(), ".json");
    }

    #[test]
    fn test_effect_serde_roundtrip() {
        let effect = Effect::new(12345, "reverb_large_hall");
        let json = serde_json::to_string(&effect).unwrap();
        let parsed: Effect = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), effect.id());
        assert_eq!(parsed.name(), effect.name());
    }
}
