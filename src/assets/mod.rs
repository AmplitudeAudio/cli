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

//! Asset trait and types for Amplitude Audio SDK assets.
//!
//! This module defines the core `Asset` trait that all asset types implement,
//! providing a unified interface for asset operations including validation,
//! serialization, and identification.
//!
//! # Asset Types
//!
//! The SDK supports seven asset types:
//! - Sound: Individual sound definitions
//! - Collection: Grouped sound variations
//! - Switch: Switch state definitions
//! - SwitchContainer: State-based sound switching
//! - Soundbank: Packaged audio assets for runtime
//! - Event: Triggerable audio events
//! - Effect: Audio effects (reverb, EQ, etc.)
//!
//! # Validation Architecture
//!
//! Assets are validated through four layers:
//! 1. Schema - JSON structure validation against SDK schemas
//! 2. ID Uniqueness - Global ID uniqueness across all assets
//! 3. Name Uniqueness - Per-type name uniqueness
//! 4. Type Rules - Type-specific business rules

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;

use crate::common::errors::CliError;

/// Auto-generated asset types from SDK FlatBuffer schemas.
///
/// Generated at build time by `build.rs` which reads `.bfbs` binary schema files
/// from `$AM_SDK_PATH/schemas/` and emits serde-compatible Rust types. These types
/// mirror the SDK's data definitions (enums, tables, structs) and are always in sync
/// with the installed SDK version.
///
/// **Unions:** FlatBuffer union types are skipped (noted with TODO comments in the
/// generated file) and will be handled in a future story.
#[allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]
pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/generated_assets.rs"));
}

// Submodules for each asset type
mod collection;
mod effect;
mod event;
/// FaderAlgorithm enum and convenience extension methods for generated SDK types.
pub mod extensions;
mod sound;
mod soundbank;
mod switch;
mod switch_container;
/// Cross-asset reference validation infrastructure.
pub mod validator;

// Re-export generated types used across asset modules.
#[allow(unused_imports)]
pub use generated::{
    CollectionPlayMode, CurveDefinition, CurvePartDefinition, CurvePointDefinition,
    EventActionDefinition, EventActionRunningMode, EventActionType, RtpcCompatibleValue,
    RtpcParameter, Scope, SoundLoopConfig, SoundSchedulerMode, SoundSchedulerSettings,
    Spatialization, ValueKind,
};
// Re-export hand-written types that have no generated equivalent.
#[allow(unused_imports)]
pub use extensions::{COLLECTION_PLAY_MODE_NAMES, FaderAlgorithm, SOUND_SCHEDULER_MODE_NAMES};

// Re-export all asset types.
// Note: Some types are currently unused but are part of the public API for future asset type
// implementations. These will be used when Collection, Effect, Event, etc. CRUD commands are added.
#[allow(unused_imports)]
pub use collection::{Collection, CollectionBuilder};
#[allow(unused_imports)]
pub use effect::{Effect, EffectBuilder};
pub use event::{Event, EventBuilder};
pub use sound::{Sound, SoundBuilder};
pub use soundbank::{Soundbank, SoundbankBuilder};
#[allow(unused_imports)]
pub use switch::{Switch, SwitchBuilder};
#[allow(unused_imports)]
pub use switch_container::SwitchContainer;
pub use validator::ProjectValidator;

use crate::common::utils::{
    ASSET_DIR_COLLECTIONS, ASSET_DIR_EFFECTS, ASSET_DIR_EVENTS, ASSET_DIR_SOUNDBANKS,
    ASSET_DIR_SOUNDS, ASSET_DIR_SWITCH_CONTAINERS, ASSET_DIR_SWITCHES,
};

// =============================================================================
// Asset Trait
// =============================================================================

/// Core trait for all Amplitude Audio SDK assets.
///
/// All asset types implement this trait to provide a unified interface for:
/// - Identification (id, name, type)
/// - File operations (file extension)
/// - Validation (schema and business rules)
///
/// The trait requires `Serialize + DeserializeOwned` bounds to enable
/// generic CRUD operations on assets.
///
/// # Example
///
/// ```ignore
/// use am::assets::{Asset, AssetType, Schema, ProjectContext, ValidationError};
///
/// struct MySound {
///     id: u64,
///     name: String,
/// }
///
/// impl Asset for MySound {
///     fn id(&self) -> u64 { self.id }
///     fn name(&self) -> &str { &self.name }
///     fn asset_type(&self) -> AssetType { AssetType::Sound }
///     fn file_extension(&self) -> &'static str { ".json" }
///
///     fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
///         Ok(()) // Schema validation placeholder
///     }
///
///     fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
///         Ok(()) // Business rules validation
///     }
/// }
/// ```
// Note: Some trait methods (id, name, asset_type, file_extension, validate_schema) are currently
// unused but are part of the Asset contract. These will be used in Epic 6 for SDK schema
// integration and generic asset CRUD operations.
#[allow(dead_code)]
pub trait Asset: Serialize + DeserializeOwned + Send + Sync {
    /// Returns the unique identifier for this asset.
    ///
    /// Asset IDs must be unique across ALL assets in a project,
    /// not just within the same asset type.
    fn id(&self) -> u64;

    /// Returns the name of this asset.
    ///
    /// Asset names must be unique within their asset type.
    fn name(&self) -> &str;

    /// Returns the type of this asset.
    fn asset_type(&self) -> AssetType;

    /// Returns the file extension for this asset type.
    ///
    /// This is used when saving the asset to disk.
    /// Example: ".json" for all asset types (assets live in type-specific directories).
    fn file_extension(&self) -> &'static str;

    /// Validates the asset against SDK schema.
    ///
    /// This is a placeholder that will be fully implemented in Epic 6
    /// when SDK schema integration is added.
    fn validate_schema(&self, schema: &Schema) -> Result<(), ValidationError>;

    /// Validates type-specific business rules.
    ///
    /// This checks rules beyond schema validation, such as:
    /// - Reference integrity (referenced assets exist)
    /// - Value constraints (e.g., gain values in valid range)
    /// - Type-specific requirements
    fn validate_rules(&self, context: &ProjectContext) -> Result<(), ValidationError>;
}

// =============================================================================
// AssetType Enum
// =============================================================================

/// Enumeration of all asset types supported by the Amplitude SDK.
///
/// Each variant corresponds to a specific asset type with its own
/// file format, directory location, and validation rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetType {
    /// Individual sound definitions
    Sound,
    /// Grouped sound variations
    Collection,
    /// Switch state definitions
    Switch,
    /// State-based sound switching
    SwitchContainer,
    /// Packaged audio assets for runtime
    Soundbank,
    /// Triggerable audio events
    Event,
    /// Audio effects (reverb, EQ, etc.)
    Effect,
}

impl AssetType {
    /// Returns the directory name in the sources/ folder.
    ///
    /// This matches the SDK's expected directory structure.
    ///
    /// # Examples
    ///
    /// ```
    /// use am::assets::AssetType;
    ///
    /// assert_eq!(AssetType::Sound.directory_name(), "sounds");
    /// assert_eq!(AssetType::SwitchContainer.directory_name(), "switch_containers");
    /// ```
    pub fn directory_name(&self) -> &'static str {
        match self {
            Self::Sound => ASSET_DIR_SOUNDS,
            Self::Collection => ASSET_DIR_COLLECTIONS,
            Self::Switch => ASSET_DIR_SWITCHES,
            Self::SwitchContainer => ASSET_DIR_SWITCH_CONTAINERS,
            Self::Soundbank => ASSET_DIR_SOUNDBANKS,
            Self::Event => ASSET_DIR_EVENTS,
            Self::Effect => ASSET_DIR_EFFECTS,
        }
    }

    /// Returns the file extension for this asset type.
    ///
    /// # Examples
    ///
    /// ```
    /// use am::assets::AssetType;
    ///
    /// assert_eq!(AssetType::Sound.file_extension(), ".json");
    /// assert_eq!(AssetType::Event.file_extension(), ".json");
    /// ```
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::Sound => ".json",
            Self::Collection => ".json",
            Self::Switch => ".json",
            Self::SwitchContainer => ".json",
            Self::Soundbank => ".json",
            Self::Event => ".json",
            Self::Effect => ".json",
        }
    }
}

impl fmt::Display for AssetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sound => write!(f, "Sound"),
            Self::Collection => write!(f, "Collection"),
            Self::Switch => write!(f, "Switch"),
            Self::SwitchContainer => write!(f, "Switch Container"),
            Self::Soundbank => write!(f, "Soundbank"),
            Self::Event => write!(f, "Event"),
            Self::Effect => write!(f, "Effect"),
        }
    }
}

impl AsRef<str> for AssetType {
    /// Returns the directory name as a string slice.
    ///
    /// This is useful for path construction.
    fn as_ref(&self) -> &str {
        self.directory_name()
    }
}

// =============================================================================
// Validation Types
// =============================================================================

/// The layer at which validation occurred.
///
/// Validation is performed in this order:
/// 1. Schema - JSON structure validation
/// 2. IdUniqueness - Global ID uniqueness
/// 3. NameUniqueness - Per-type name uniqueness
/// 4. TypeRules - Type-specific business rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationLayer {
    /// JSON structure validation against SDK schemas
    Schema,
    /// Global ID uniqueness across all assets
    IdUniqueness,
    /// Per-type name uniqueness
    NameUniqueness,
    /// Type-specific business rules
    TypeRules,
}

impl fmt::Display for ValidationLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Schema => write!(f, "Schema"),
            Self::IdUniqueness => write!(f, "ID Uniqueness"),
            Self::NameUniqueness => write!(f, "Name Uniqueness"),
            Self::TypeRules => write!(f, "Type Rules"),
        }
    }
}

/// Error returned when asset validation fails.
///
/// Composes `CliError` to reuse the What/Why/Fix pattern, adding
/// validation-specific fields for layer and field information.
///
/// # Error Codes
///
/// Uses the -31xxx validation error range:
/// - -31001: Schema validation failed
/// - -31002: Field validation failed
/// - -31003: Format validation failed
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// The underlying CLI error with What/Why/Fix pattern
    pub inner: CliError,
    /// The validation layer that caught the error
    pub layer: ValidationLayer,
    /// Specific field that failed (if applicable)
    pub field: Option<String>,
}

impl ValidationError {
    /// Creates a new ValidationError.
    ///
    /// # Arguments
    ///
    /// * `layer` - The validation layer that caught the error
    /// * `code` - Error code from the -31xxx range
    /// * `what` - What operation failed
    /// * `why` - Why it failed
    pub fn new(
        layer: ValidationLayer,
        code: i32,
        what: impl Into<String>,
        why: impl Into<String>,
    ) -> Self {
        Self {
            inner: CliError::new(code, what, why),
            layer,
            field: None,
        }
    }

    /// Adds field information to the error.
    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    /// Adds context information to the underlying CliError.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.inner = self.inner.with_context(context);
        self
    }

    /// Overrides the suggestion on the underlying CliError.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.inner = self.inner.with_suggestion(suggestion);
        self
    }

    /// Creates a schema validation error.
    pub fn schema_error(what: impl Into<String>, why: impl Into<String>) -> Self {
        Self::new(
            ValidationLayer::Schema,
            crate::common::errors::codes::ERR_VALIDATION_SCHEMA,
            what,
            why,
        )
        .with_suggestion("Check that your JSON structure matches the expected schema")
    }

    /// Creates an ID uniqueness validation error.
    pub fn duplicate_id(id: u64, conflicting_asset: impl Into<String>) -> Self {
        Self::new(
            ValidationLayer::IdUniqueness,
            crate::common::errors::codes::ERR_VALIDATION_FIELD,
            format!("Duplicate asset ID: {}", id),
            "Asset IDs must be unique across all assets in the project",
        )
        .with_suggestion("Use a different ID or modify the existing asset")
        .with_field("id")
        .with_context(conflicting_asset)
    }

    /// Creates a name uniqueness validation error.
    pub fn duplicate_name(name: impl Into<String>, asset_type: AssetType) -> Self {
        let name = name.into();
        Self::new(
            ValidationLayer::NameUniqueness,
            crate::common::errors::codes::ERR_VALIDATION_FIELD,
            format!("Duplicate {} name: {}", asset_type, name),
            format!(
                "{} names must be unique within the {} type",
                asset_type, asset_type
            ),
        )
        .with_suggestion("Use a different name or modify the existing asset")
        .with_field("name")
    }

    /// Creates a type rules validation error.
    pub fn type_rule_violation(what: impl Into<String>, why: impl Into<String>) -> Self {
        Self::new(
            ValidationLayer::TypeRules,
            crate::common::errors::codes::ERR_VALIDATION_FIELD,
            what,
            why,
        )
        .with_suggestion("Check the asset requirements and correct the invalid values")
    }

    // Accessors to inner CliError fields for convenience

    /// Returns the error code.
    pub fn code(&self) -> i32 {
        self.inner.code
    }

    /// Returns what failed.
    pub fn what(&self) -> &str {
        &self.inner.what
    }

    /// Returns why it failed.
    pub fn why(&self) -> &str {
        &self.inner.why
    }

    /// Returns the suggestion.
    pub fn suggestion(&self) -> &str {
        &self.inner.suggestion
    }

    /// Returns the context if set.
    pub fn context(&self) -> Option<&str> {
        self.inner.context.as_deref()
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.layer, self.inner)?;
        if let Some(field) = &self.field {
            write!(f, " (field: {})", field)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.inner)
    }
}

impl From<ValidationError> for CliError {
    fn from(err: ValidationError) -> Self {
        err.inner
    }
}

// =============================================================================
// Project Context
// =============================================================================

/// Context for validating assets within a project.
///
/// Contains the project root path and registries for checking
/// ID and name uniqueness during validation.
///
/// # Example
///
/// ```
/// use std::path::PathBuf;
/// use am::assets::ProjectContext;
///
/// let ctx = ProjectContext::new(PathBuf::from("/path/to/project"));
/// ```
pub struct ProjectContext {
    /// Root path of the project (where .amproject lives)
    pub project_root: PathBuf,
    /// Resolved data directory path (audio files)
    pub data_dir: PathBuf,
    /// Registry of existing asset IDs for uniqueness checks (populated lazily)
    pub id_registry: HashSet<u64>,
    /// Registry of existing asset names by type (populated lazily)
    pub name_registry: HashMap<AssetType, HashSet<String>>,
    /// Optional cross-asset reference validator for checking inter-asset dependencies.
    pub validator: Option<validator::ProjectValidator>,
}

impl ProjectContext {
    /// Creates a new ProjectContext for the given project root.
    ///
    /// Reads `.amproject` to resolve the data directory path.
    /// Falls back to `<project_root>/data` if `.amproject` is unavailable.
    pub fn new(project_root: PathBuf) -> Self {
        let data_dir = match crate::common::utils::read_amproject_file(&project_root) {
            Ok(config) => {
                if config.data_dir.is_empty() {
                    project_root.join("data")
                } else {
                    project_root.join(&config.data_dir)
                }
            }
            Err(_) => project_root.join("data"),
        };

        Self {
            project_root,
            data_dir,
            id_registry: HashSet::new(),
            name_registry: HashMap::new(),
            validator: None,
        }
    }

    /// Creates an empty ProjectContext for testing.
    ///
    /// Uses current directory as root. Useful when only registry
    /// operations are needed without actual file system access.
    pub fn empty() -> Self {
        Self {
            project_root: PathBuf::new(),
            data_dir: PathBuf::from("data"),
            id_registry: HashSet::new(),
            name_registry: HashMap::new(),
            validator: None,
        }
    }

    /// Attaches a `ProjectValidator` for cross-asset reference checking.
    ///
    /// When set, `validate_rules()` implementations can use it to verify
    /// that referenced asset IDs actually exist.
    ///
    /// Also populates `id_registry` and `name_registry` from the validator's
    /// scanned data so that `has_id()` and `has_name()` checks work.
    pub fn with_validator(mut self, validator: validator::ProjectValidator) -> Self {
        // Populate id_registry from the validator's scanned asset IDs
        for ids in validator.asset_ids.values() {
            for &id in ids {
                self.id_registry.insert(id);
            }
        }

        // Populate name_registry from the validator's scanned asset names
        for (asset_type, names) in &validator.asset_names {
            let entry = self.name_registry.entry(*asset_type).or_default();
            for name in names {
                entry.insert(name.clone());
            }
        }

        self.validator = Some(validator);
        self
    }

    /// Checks if an ID is already registered.
    pub fn has_id(&self, id: u64) -> bool {
        self.id_registry.contains(&id)
    }

    /// Registers an ID in the registry.
    ///
    /// Returns `true` if the ID was newly inserted, `false` if it already existed.
    pub fn register_id(&mut self, id: u64) -> bool {
        self.id_registry.insert(id)
    }

    /// Checks if a name is already registered for the given asset type.
    pub fn has_name(&self, asset_type: AssetType, name: &str) -> bool {
        self.name_registry
            .get(&asset_type)
            .map(|names| names.contains(name))
            .unwrap_or(false)
    }

    /// Registers a name for the given asset type.
    ///
    /// Returns `true` if the name was newly inserted, `false` if it already existed.
    pub fn register_name(&mut self, asset_type: AssetType, name: String) -> bool {
        self.name_registry
            .entry(asset_type)
            .or_default()
            .insert(name)
    }
}

// =============================================================================
// Schema Placeholder
// =============================================================================

/// Placeholder for SDK schema validation.
///
/// This will be fully implemented in Epic 6 when SDK schema integration
/// is added. For now, it provides a no-op validator.
pub struct Schema {
    /// Zero-sized private field to prevent external construction.
    /// This allows adding fields later without breaking changes.
    _private: (),
}

impl Schema {
    /// Creates a no-op schema validator (placeholder).
    ///
    /// Use this until Epic 6 implements actual schema validation.
    pub fn noop() -> Self {
        Self { _private: () }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_type_directory_name() {
        assert_eq!(AssetType::Sound.directory_name(), "sounds");
        assert_eq!(AssetType::Collection.directory_name(), "collections");
        assert_eq!(AssetType::Switch.directory_name(), "switches");
        assert_eq!(
            AssetType::SwitchContainer.directory_name(),
            "switch_containers"
        );
        assert_eq!(AssetType::Soundbank.directory_name(), "soundbanks");
        assert_eq!(AssetType::Event.directory_name(), "events");
        assert_eq!(AssetType::Effect.directory_name(), "effects");
    }

    #[test]
    fn test_asset_type_file_extension() {
        // All assets in their type folders use .json extension
        // Config files (.config.json) and buses (.buses.json) are in project root, not assets
        assert_eq!(AssetType::Sound.file_extension(), ".json");
        assert_eq!(AssetType::Collection.file_extension(), ".json");
        assert_eq!(AssetType::Switch.file_extension(), ".json");
        assert_eq!(AssetType::SwitchContainer.file_extension(), ".json");
        assert_eq!(AssetType::Soundbank.file_extension(), ".json");
        assert_eq!(AssetType::Event.file_extension(), ".json");
        assert_eq!(AssetType::Effect.file_extension(), ".json");
    }

    #[test]
    fn test_asset_type_display() {
        assert_eq!(format!("{}", AssetType::Sound), "Sound");
        assert_eq!(format!("{}", AssetType::Collection), "Collection");
        assert_eq!(format!("{}", AssetType::Switch), "Switch");
        assert_eq!(
            format!("{}", AssetType::SwitchContainer),
            "Switch Container"
        );
        assert_eq!(format!("{}", AssetType::Soundbank), "Soundbank");
        assert_eq!(format!("{}", AssetType::Event), "Event");
        assert_eq!(format!("{}", AssetType::Effect), "Effect");
    }

    #[test]
    fn test_asset_type_as_ref() {
        let asset_type: &str = AssetType::Sound.as_ref();
        assert_eq!(asset_type, "sounds");

        let asset_type: &str = AssetType::SwitchContainer.as_ref();
        assert_eq!(asset_type, "switch_containers");
    }

    #[test]
    fn test_asset_type_serde_roundtrip() {
        let original = AssetType::SwitchContainer;
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "\"switch_container\"");

        let parsed: AssetType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_validation_layer_display() {
        assert_eq!(format!("{}", ValidationLayer::Schema), "Schema");
        assert_eq!(
            format!("{}", ValidationLayer::IdUniqueness),
            "ID Uniqueness"
        );
        assert_eq!(
            format!("{}", ValidationLayer::NameUniqueness),
            "Name Uniqueness"
        );
        assert_eq!(format!("{}", ValidationLayer::TypeRules), "Type Rules");
    }

    #[test]
    fn test_validation_error_formatting() {
        let err = ValidationError::new(
            ValidationLayer::Schema,
            -31001,
            "Invalid JSON structure",
            "Missing required field 'name'",
        )
        .with_suggestion("Add the 'name' field to your asset definition")
        .with_field("name");

        let formatted = format!("{}", err);
        assert!(formatted.contains("[Schema]"));
        assert!(formatted.contains("Invalid JSON structure"));
        assert!(formatted.contains("Missing required field 'name'"));
        assert!(formatted.contains("(field: name)"));
    }

    #[test]
    fn test_validation_error_duplicate_id() {
        let err = ValidationError::duplicate_id(12345, "sounds/explosion.json");

        assert_eq!(err.layer, ValidationLayer::IdUniqueness);
        assert!(err.what().contains("12345"));
        assert_eq!(err.field, Some("id".to_string()));
        assert_eq!(err.context(), Some("sounds/explosion.json"));
    }

    #[test]
    fn test_validation_error_duplicate_name() {
        let err = ValidationError::duplicate_name("explosion", AssetType::Sound);

        assert_eq!(err.layer, ValidationLayer::NameUniqueness);
        assert!(err.what().contains("explosion"));
        assert!(err.what().contains("Sound"));
        assert_eq!(err.field, Some("name".to_string()));
    }

    #[test]
    fn test_project_context_new() {
        let ctx = ProjectContext::new(PathBuf::from("/test/project"));
        assert_eq!(ctx.project_root, PathBuf::from("/test/project"));
        assert!(ctx.id_registry.is_empty());
        assert!(ctx.name_registry.is_empty());
    }

    #[test]
    fn test_project_context_empty() {
        let ctx = ProjectContext::empty();
        assert_eq!(ctx.project_root, PathBuf::new());
        assert!(ctx.id_registry.is_empty());
        assert!(ctx.name_registry.is_empty());
    }

    #[test]
    fn test_project_context_id_registry() {
        let mut ctx = ProjectContext::new(PathBuf::from("/test"));

        // Initially empty
        assert!(!ctx.has_id(12345));

        // Register an ID
        assert!(ctx.register_id(12345)); // Returns true for new ID
        assert!(ctx.has_id(12345));

        // Try to register same ID again
        assert!(!ctx.register_id(12345)); // Returns false for duplicate
    }

    #[test]
    fn test_project_context_name_registry() {
        let mut ctx = ProjectContext::new(PathBuf::from("/test"));

        // Initially empty
        assert!(!ctx.has_name(AssetType::Sound, "explosion"));

        // Register a name
        assert!(ctx.register_name(AssetType::Sound, "explosion".to_string()));
        assert!(ctx.has_name(AssetType::Sound, "explosion"));

        // Same name for different type is OK
        assert!(!ctx.has_name(AssetType::Effect, "explosion"));
        assert!(ctx.register_name(AssetType::Effect, "explosion".to_string()));
        assert!(ctx.has_name(AssetType::Effect, "explosion"));

        // Try to register same name for same type again
        assert!(!ctx.register_name(AssetType::Sound, "explosion".to_string()));
    }

    #[test]
    fn test_schema_noop() {
        let _schema = Schema::noop();
        // Just verify it can be created
    }
}
