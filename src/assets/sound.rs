//! Sound asset type.
//!
//! Placeholder implementation for Story 2-1.
//! Full implementation in Story 2-2.

use serde::{Deserialize, Serialize};

use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// Individual sound definition.
///
/// Represents a single audio source with playback configuration.
/// Full implementation will be added in Story 2-2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sound {
    /// Unique identifier for this sound.
    pub id: u64,
    /// Name of the sound.
    pub name: String,
}

impl Sound {
    /// Creates a new Sound with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl Asset for Sound {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Sound
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Sound.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Epic 6
        Ok(())
    }

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        // Placeholder - full implementation in Story 2-2
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_new() {
        let sound = Sound::new(12345, "explosion");
        assert_eq!(sound.id(), 12345);
        assert_eq!(sound.name(), "explosion");
    }

    #[test]
    fn test_sound_asset_type() {
        let sound = Sound::new(1, "test");
        assert_eq!(sound.asset_type(), AssetType::Sound);
    }

    #[test]
    fn test_sound_file_extension() {
        let sound = Sound::new(1, "test");
        assert_eq!(sound.file_extension(), ".json");
    }

    #[test]
    fn test_sound_serde_roundtrip() {
        let sound = Sound::new(12345, "explosion");
        let json = serde_json::to_string(&sound).unwrap();
        let parsed: Sound = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), sound.id());
        assert_eq!(parsed.name(), sound.name());
    }
}
