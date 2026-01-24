//! Event asset type.
//!
//! Placeholder implementation for Story 2-1.
//! Full implementation in Epic 5.

use serde::{Deserialize, Serialize};

use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// Triggerable audio events.
///
/// Represents an event that can be triggered by game code to play
/// sounds, collections, or other audio assets.
/// Full implementation will be added in Epic 5.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique identifier for this event.
    pub id: u64,
    /// Name of the event.
    pub name: String,
}

impl Event {
    /// Creates a new Event with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl Asset for Event {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Event
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Event.file_extension()
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
    fn test_event_new() {
        let event = Event::new(12345, "play_explosion");
        assert_eq!(event.id(), 12345);
        assert_eq!(event.name(), "play_explosion");
    }

    #[test]
    fn test_event_asset_type() {
        let event = Event::new(1, "test");
        assert_eq!(event.asset_type(), AssetType::Event);
    }

    #[test]
    fn test_event_file_extension() {
        let event = Event::new(1, "test");
        assert_eq!(event.file_extension(), ".json");
    }

    #[test]
    fn test_event_serde_roundtrip() {
        let event = Event::new(12345, "play_explosion");
        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id(), event.id());
        assert_eq!(parsed.name(), event.name());
    }
}
