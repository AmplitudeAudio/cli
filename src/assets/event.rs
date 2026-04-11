//! Event asset type.
//!
//! Events are triggerable audio actions that can be called from game code.
//! Each event contains one or more actions that specify what to play, stop,
//! or modify when the event is triggered.

use super::generated::{
    EventActionDefinition, EventActionRunningMode, EventActionType, EventDefinition, Scope,
};
use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

// =============================================================================
// Event Type Alias
// =============================================================================

/// Triggerable audio event definition.
///
/// Type alias to the build-time generated `EventDefinition` from SDK FlatBuffer schemas.
/// Events are triggered by game code to execute audio actions like playing sounds,
/// stopping playback, or modifying parameters.
///
/// # Example
///
/// ```
/// use am::assets::{Event, EventActionType};
///
/// let event = Event::builder(12345, "play_explosion")
///     .action(EventActionType::Play, vec![1001])
///     .build();
/// ```
pub type Event = EventDefinition;

impl Event {
    /// Creates a builder for constructing an Event with optional fields.
    pub fn builder(id: u64, name: impl Into<String>) -> EventBuilder {
        EventBuilder::new(id, name)
    }
}

// =============================================================================
// EventBuilder
// =============================================================================

/// Builder for constructing Event instances with optional fields.
#[derive(Debug, Clone)]
pub struct EventBuilder {
    event: Event,
}

impl EventBuilder {
    /// Creates a new EventBuilder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            event: Event {
                id,
                name: Some(name.into()),
                run_mode: EventActionRunningMode::Parallel,
                actions: Some(vec![]),
            },
        }
    }

    /// Sets the run mode for actions.
    pub fn run_mode(mut self, mode: EventActionRunningMode) -> Self {
        self.event.run_mode = mode;
        self
    }

    /// Adds an action to the event.
    ///
    /// # Arguments
    ///
    /// * `action_type` - The type of action (Play, Stop, Pause, etc.)
    /// * `targets` - Asset IDs to target with this action
    /// * `scope` - The scope for this action (defaults to Entity)
    pub fn action(mut self, action_type: EventActionType, targets: Vec<u64>) -> Self {
        let action = EventActionDefinition {
            type_: action_type,
            active: true,
            scope: Scope::Entity,
            targets: if targets.is_empty() {
                None
            } else {
                Some(targets)
            },
        };
        if let Some(ref mut actions) = self.event.actions {
            actions.push(action);
        } else {
            self.event.actions = Some(vec![action]);
        }
        self
    }

    /// Adds a fully configured action to the event.
    pub fn action_def(mut self, action: EventActionDefinition) -> Self {
        if let Some(ref mut actions) = self.event.actions {
            actions.push(action);
        } else {
            self.event.actions = Some(vec![action]);
        }
        self
    }

    /// Builds the Event instance.
    pub fn build(self) -> Event {
        self.event
    }
}

impl Asset for Event {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
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

    fn validate_rules(&self, context: &ProjectContext) -> Result<(), ValidationError> {
        // Validate name is not empty
        if self.name.as_deref().unwrap_or("").is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Event asset has no name",
                "Event assets must have a non-empty name",
            )
            .with_suggestion(
                "Set the 'name' field to a valid identifier (e.g., \"play_explosion\")",
            )
            .with_field("name"));
        }

        // Validate at least one action is defined
        let actions = self.actions.as_deref().unwrap_or(&[]);
        if actions.is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Event has no actions",
                "Events must have at least one action defined",
            )
            .with_suggestion(
                "Add at least one action using the builder or by setting the 'actions' field",
            )
            .with_field("actions"));
        }

        // Validate each action
        for (idx, action) in actions.iter().enumerate() {
            // Validate action has targets for action types that need them
            match action.type_ {
                EventActionType::Play
                | EventActionType::Stop
                | EventActionType::Pause
                | EventActionType::Resume
                | EventActionType::Seek => {
                    let targets = action.targets.as_deref().unwrap_or(&[]);
                    if targets.is_empty() {
                        return Err(ValidationError::type_rule_violation(
                            format!("Action {} has no targets", idx + 1),
                            format!(
                                "{:?} actions must have at least one target asset",
                                action.type_
                            ),
                        )
                        .with_suggestion("Add target asset IDs to the 'targets' field")
                        .with_field(format!("actions[{}].targets", idx)));
                    }

                    // Validate all target IDs reference existing assets
                    if let Some(ref validator) = context.validator {
                        for (target_idx, &target_id) in targets.iter().enumerate() {
                            // For Play/Resume actions, targets should be playable assets
                            // (sounds, collections, switch containers)
                            if action.type_ == EventActionType::Play
                                || action.type_ == EventActionType::Resume
                            {
                                if !validator.is_playable_asset(target_id) {
                                    return Err(ValidationError::type_rule_violation(
                                        format!(
                                            "Action {} target {} references non-playable asset {}",
                                            idx + 1,
                                            target_idx + 1,
                                            target_id
                                        ),
                                        "Play and Resume actions must target sounds, collections, or switch containers",
                                    )
                                    .with_suggestion(
                                        "Ensure the target ID references a valid playable asset",
                                    )
                                    .with_field(format!(
                                        "actions[{}].targets[{}]",
                                        idx, target_idx
                                    )));
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Other action types may not need targets
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_builder_basic() {
        let event = Event::builder(12345, "play_explosion")
            .action(EventActionType::Play, vec![1001])
            .build();
        assert_eq!(event.id(), 12345);
        assert_eq!(event.name(), "play_explosion");
        assert_eq!(event.actions.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_event_builder_defaults() {
        let event = Event::builder(1, "test_event")
            .action(EventActionType::Play, vec![100])
            .build();
        assert_eq!(event.id, 1);
        assert_eq!(event.name.as_deref(), Some("test_event"));
        assert_eq!(event.run_mode, EventActionRunningMode::Parallel);
        assert_eq!(event.actions.as_ref().unwrap().len(), 1);

        let action = &event.actions.unwrap()[0];
        assert_eq!(action.type_, EventActionType::Play);
        assert!(action.active);
        assert_eq!(action.scope, Scope::Entity);
        assert_eq!(action.targets.as_ref().unwrap(), &vec![100]);
    }

    #[test]
    fn test_event_builder_multiple_actions() {
        let event = Event::builder(12345, "complex_event")
            .action(EventActionType::Play, vec![1001])
            .action(EventActionType::Pause, vec![1002])
            .action(EventActionType::Stop, vec![1003])
            .build();

        assert_eq!(event.id, 12345);
        assert_eq!(event.name.as_deref(), Some("complex_event"));
        assert_eq!(event.actions.as_ref().unwrap().len(), 3);

        let actions = event.actions.unwrap();
        assert_eq!(actions[0].type_, EventActionType::Play);
        assert_eq!(actions[1].type_, EventActionType::Pause);
        assert_eq!(actions[2].type_, EventActionType::Stop);
    }

    #[test]
    fn test_event_builder_run_mode() {
        let event = Event::builder(1, "sequential_event")
            .run_mode(EventActionRunningMode::Sequential)
            .action(EventActionType::Play, vec![100])
            .build();

        assert_eq!(event.run_mode, EventActionRunningMode::Sequential);
    }

    #[test]
    fn test_event_builder_action_def() {
        let custom_action = EventActionDefinition {
            type_: EventActionType::Seek,
            active: false,
            scope: Scope::World,
            targets: Some(vec![200, 201]),
        };

        let event = Event::builder(1, "custom_event")
            .action_def(custom_action)
            .build();

        let actions = event.actions.unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].type_, EventActionType::Seek);
        assert!(!actions[0].active);
        assert_eq!(actions[0].scope, Scope::World);
    }

    #[test]
    fn test_event_asset_type() {
        let event = Event::builder(1, "test")
            .action(EventActionType::Play, vec![100])
            .build();
        assert_eq!(event.asset_type(), AssetType::Event);
    }

    #[test]
    fn test_event_file_extension() {
        let event = Event::builder(1, "test")
            .action(EventActionType::Play, vec![100])
            .build();
        assert_eq!(event.file_extension(), ".json");
    }

    #[test]
    fn test_event_serde_roundtrip() {
        let event = Event::builder(12345, "play_explosion")
            .run_mode(EventActionRunningMode::Sequential)
            .action(EventActionType::Play, vec![1001, 1002])
            .action(EventActionType::Stop, vec![2001])
            .build();

        let json = serde_json::to_string(&event).unwrap();
        let parsed: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, event.id);
        assert_eq!(parsed.name, event.name);
        assert_eq!(parsed.run_mode, event.run_mode);
        assert_eq!(parsed.actions.as_ref().unwrap().len(), 2);

        let parsed_actions = parsed.actions.unwrap();
        let original_actions = event.actions.unwrap();
        assert_eq!(parsed_actions[0].type_, original_actions[0].type_);
        assert_eq!(parsed_actions[0].targets, original_actions[0].targets);
        assert_eq!(parsed_actions[1].type_, original_actions[1].type_);
    }

    #[test]
    fn test_event_serde_sdk_format() {
        let event = Event::builder(12345, "play_ambience")
            .run_mode(EventActionRunningMode::Parallel)
            .action(EventActionType::Play, vec![5001])
            .build();

        let json = serde_json::to_string_pretty(&event).unwrap();

        // Verify key field names are in SDK format
        assert!(json.contains("\"id\": 12345"));
        assert!(json.contains("\"name\": \"play_ambience\""));
        assert!(json.contains("\"run_mode\": \"Parallel\""));
        assert!(json.contains("\"type\": \"Play\""));
        assert!(json.contains("\"active\": true"));
        assert!(json.contains("\"scope\": \"Entity\""));
        assert!(json.contains("\"targets\": ["));
        assert!(json.contains("5001"));
    }

    #[test]
    fn test_event_deserialize_sdk_json() {
        let sdk_json = r#"{
            "id": 54321,
            "name": "stop_all",
            "run_mode": "Sequential",
            "actions": [
                {
                    "type": "Stop",
                    "active": true,
                    "scope": "World",
                    "targets": [100, 101, 102]
                }
            ]
        }"#;

        let event: Event = serde_json::from_str(sdk_json).unwrap();
        assert_eq!(event.id, 54321);
        assert_eq!(event.name.as_deref(), Some("stop_all"));
        assert_eq!(event.run_mode, EventActionRunningMode::Sequential);

        let actions = event.actions.as_ref().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].type_, EventActionType::Stop);
        assert!(actions[0].active);
        assert_eq!(actions[0].scope, Scope::World);
        assert_eq!(actions[0].targets.as_ref().unwrap(), &vec![100, 101, 102]);
    }

    #[test]
    fn test_event_validate_rules_passes_valid() {
        let context = ProjectContext::empty();
        let event = Event::builder(1, "test_event")
            .action(EventActionType::Play, vec![100])
            .build();

        assert!(event.validate_rules(&context).is_ok());
    }

    #[test]
    fn test_event_validate_rules_fails_empty_name() {
        let context = ProjectContext::empty();
        // Need to construct manually to bypass builder validation
        let event = Event {
            id: 1,
            name: Some("".to_string()),
            run_mode: EventActionRunningMode::Parallel,
            actions: Some(vec![EventActionDefinition {
                type_: EventActionType::Play,
                active: true,
                scope: Scope::Entity,
                targets: Some(vec![100]),
            }]),
        };

        let result = event.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("no name"));
        assert_eq!(err.field, Some("name".to_string()));
    }

    #[test]
    fn test_event_validate_rules_fails_no_actions() {
        let context = ProjectContext::empty();
        let event = Event {
            id: 1,
            name: Some("test_event".to_string()),
            run_mode: EventActionRunningMode::Parallel,
            actions: Some(vec![]),
        };

        let result = event.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("no actions"));
        assert_eq!(err.field, Some("actions".to_string()));
    }

    #[test]
    fn test_event_validate_rules_fails_play_action_no_targets() {
        let context = ProjectContext::empty();
        let event = Event {
            id: 1,
            name: Some("test_event".to_string()),
            run_mode: EventActionRunningMode::Parallel,
            actions: Some(vec![EventActionDefinition {
                type_: EventActionType::Play,
                active: true,
                scope: Scope::Entity,
                targets: None,
            }]),
        };

        let result = event.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("no targets"));
        assert!(err.why().contains("Play"));
    }

    #[test]
    fn test_event_validate_rules_fails_stop_action_no_targets() {
        let context = ProjectContext::empty();
        let event = Event {
            id: 1,
            name: Some("test_event".to_string()),
            run_mode: EventActionRunningMode::Parallel,
            actions: Some(vec![EventActionDefinition {
                type_: EventActionType::Stop,
                active: true,
                scope: Scope::Entity,
                targets: Some(vec![]),
            }]),
        };

        let result = event.validate_rules(&context);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.what().contains("no targets"));
        assert!(err.why().contains("Stop"));
    }
}
