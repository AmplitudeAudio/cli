//! Cross-asset reference validation infrastructure.
//!
//! Provides `ProjectValidator` for verifying that cross-asset references
//! (e.g., a Collection referencing Sound IDs) point to valid, existing assets.
//!
//! The validator scans project directories to build a registry of all asset IDs
//! and names, then provides methods to check whether a given reference is valid.
//!
//! # Zero-ID Convention
//!
//! In the Amplitude SDK, a zero ID (`0`) means "no reference" or "unset".
//! All validation methods treat `id == 0` as a no-op and return `Ok(())`.
//!
//! # Usage
//!
//! ```ignore
//! use am::assets::validator::ProjectValidator;
//!
//! let validator = ProjectValidator::new(project_root)?;
//! validator.validate_sound_exists(42)?;       // Ok if sound with ID 42 exists
//! validator.validate_effect_exists(0)?;        // Ok (zero means no reference)
//! validator.validate_sound_exists(999)?;       // Err if no sound with ID 999
//! ```

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use log::warn;

use super::{AssetType, ValidationError, ValidationLayer};
use crate::common::errors::codes;

/// Validates cross-asset references within an Amplitude project.
///
/// Scans all asset directories under `sources/` to build a registry of known
/// asset IDs and names by type. Provides methods to verify that a referenced
/// asset actually exists.
///
/// # Design
///
/// `ProjectValidator` is intentionally separate from `ProjectContext` to keep
/// `ProjectContext` lightweight (it's used in the `Asset` trait signature).
/// `ProjectValidator` is the heavier "scan and validate" tool used when
/// cross-asset reference checking is needed.
pub struct ProjectValidator {
    /// Root path of the project (where .amproject lives).
    project_root: PathBuf,
    /// All known asset IDs grouped by type.
    asset_ids: HashMap<AssetType, HashSet<u64>>,
    /// All known asset names grouped by type.
    asset_names: HashMap<AssetType, HashSet<String>>,
}

// ValidationError is the project-wide error type for all validation methods.
// Its size is intentional and consistent with the Asset trait signature.
#[allow(clippy::result_large_err)]
impl ProjectValidator {
    /// Creates a new `ProjectValidator` by scanning the project's asset directories.
    ///
    /// Reads all JSON files in each `sources/<type>/` directory, extracting
    /// `id` and `name` fields to build the registry.
    ///
    /// Missing directories are silently skipped (a fresh project may not have
    /// all asset type directories). Malformed JSON files are logged as warnings
    /// and skipped.
    ///
    /// # Arguments
    ///
    /// * `project_root` - Path to the project root (where `.amproject` lives)
    ///
    /// # Errors
    ///
    /// Returns an error only for I/O failures that prevent scanning entirely
    /// (e.g., permission denied on the sources directory itself).
    pub fn new(project_root: PathBuf) -> anyhow::Result<Self> {
        let mut validator = Self {
            project_root,
            asset_ids: HashMap::new(),
            asset_names: HashMap::new(),
        };

        // Scan all asset types
        let asset_types = [
            AssetType::Sound,
            AssetType::Collection,
            AssetType::Effect,
            AssetType::Switch,
            AssetType::SwitchContainer,
            AssetType::Soundbank,
            AssetType::Event,
        ];

        for asset_type in &asset_types {
            validator.scan_assets_of_type(*asset_type)?;
        }

        Ok(validator)
    }

    /// Creates an empty `ProjectValidator` with no registered assets.
    ///
    /// Useful for tests and contexts where disk scanning is not needed.
    pub fn empty() -> Self {
        Self {
            project_root: PathBuf::new(),
            asset_ids: HashMap::new(),
            asset_names: HashMap::new(),
        }
    }

    /// Validates that an asset with the given ID exists for the specified type.
    ///
    /// Returns `Ok(())` if:
    /// - `id == 0` (zero means "no reference" in the SDK)
    /// - The ID exists in the registry for the given asset type
    ///
    /// Returns `Err(ValidationError)` if the ID is non-zero and not found.
    pub fn validate_asset_exists(
        &self,
        asset_type: AssetType,
        id: u64,
    ) -> Result<(), ValidationError> {
        // Zero ID means "no reference" - always valid
        if id == 0 {
            return Ok(());
        }

        let exists = self
            .asset_ids
            .get(&asset_type)
            .is_some_and(|ids| ids.contains(&id));

        if exists {
            Ok(())
        } else {
            Err(ValidationError::new(
                ValidationLayer::TypeRules,
                codes::ERR_VALIDATION_REFERENCE,
                format!("Referenced {} with ID {} not found", asset_type, id),
                format!(
                    "The referenced {} does not exist in the project",
                    asset_type
                ),
            )
            .with_suggestion(format!(
                "Create the {} first with 'am asset {} create', or fix the reference",
                asset_type,
                asset_type.directory_name()
            )))
        }
    }

    /// Validates that a Sound with the given ID exists.
    ///
    /// Convenience wrapper around `validate_asset_exists(AssetType::Sound, id)`.
    pub fn validate_sound_exists(&self, id: u64) -> Result<(), ValidationError> {
        self.validate_asset_exists(AssetType::Sound, id)
    }

    /// Validates that a Collection with the given ID exists.
    ///
    /// Convenience wrapper around `validate_asset_exists(AssetType::Collection, id)`.
    pub fn validate_collection_exists(&self, id: u64) -> Result<(), ValidationError> {
        self.validate_asset_exists(AssetType::Collection, id)
    }

    /// Validates that an Effect with the given ID exists.
    ///
    /// Convenience wrapper around `validate_asset_exists(AssetType::Effect, id)`.
    pub fn validate_effect_exists(&self, id: u64) -> Result<(), ValidationError> {
        self.validate_asset_exists(AssetType::Effect, id)
    }

    /// Validates that a Switch with the given ID exists.
    ///
    /// Convenience wrapper around `validate_asset_exists(AssetType::Switch, id)`.
    pub fn validate_switch_exists(&self, id: u64) -> Result<(), ValidationError> {
        self.validate_asset_exists(AssetType::Switch, id)
    }

    /// Validates that a Switch state exists within a given Switch.
    ///
    /// Currently validates only that the parent Switch exists. Full state
    /// validation will be added in Epic 4 when the Switch struct is implemented.
    ///
    /// Returns `Ok(())` if `switch_id == 0` (no reference).
    pub fn validate_switch_state_exists(
        &self,
        switch_id: u64,
        _state_id: u64,
    ) -> Result<(), ValidationError> {
        // For now, just validate the switch itself exists.
        // State validation deferred to Epic 4 when Switch struct is implemented.
        self.validate_switch_exists(switch_id)
    }

    /// Returns the project root path.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Scans a single asset type directory and populates the registries.
    ///
    /// Reads all `.json` files from `sources/<type>/`, extracting `id` and
    /// `name` fields using `serde_json::Value` (avoids requiring full struct
    /// deserialization for asset types not yet implemented).
    ///
    /// Symlinks are followed when scanning. Symlinked JSON files are processed
    /// as regular files.
    fn scan_assets_of_type(&mut self, asset_type: AssetType) -> anyhow::Result<()> {
        let dir = self
            .project_root
            .join("sources")
            .join(asset_type.directory_name());

        // Attempt to read the directory directly. Missing or non-directory paths
        // return an error which we treat as "no assets of this type" (not a failure).
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    warn!(
                        "Failed to read directory entry in {}: {}",
                        dir.display(),
                        err
                    );
                    continue;
                }
            };

            let path = entry.path();

            // Only process .json files (skip directories, non-JSON files, etc.)
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }

            // Read and parse the JSON file
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(err) => {
                    warn!("Failed to read asset file {}: {}", path.display(), err);
                    continue;
                }
            };

            let value: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(err) => {
                    warn!("Malformed JSON in asset file {}: {}", path.display(), err);
                    continue;
                }
            };

            // Extract id (u64) and name (String)
            if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                self.asset_ids.entry(asset_type).or_default().insert(id);
            }

            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                self.asset_names
                    .entry(asset_type)
                    .or_default()
                    .insert(name.to_string());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    /// Helper: create a sound JSON file in the given directory.
    fn write_sound_json(sounds_dir: &std::path::Path, filename: &str, id: u64, name: &str) {
        let sound_json = json!({
            "id": id,
            "name": name,
            "path": format!("data/{}.wav", name),
            "bus": 0,
            "gain": { "kind": "Static", "value": 1.0 },
            "priority": { "kind": "Static", "value": 128.0 },
            "stream": false,
            "loop": { "enabled": false, "loop_count": 0 },
            "spatialization": "None",
            "attenuation": 0,
            "scope": "World",
            "fader": "Linear",
            "effect": 0
        });
        fs::write(
            sounds_dir.join(filename),
            serde_json::to_string_pretty(&sound_json).unwrap(),
        )
        .unwrap();
    }

    /// Helper: create a minimal asset JSON file (just id and name).
    fn write_minimal_asset_json(dir: &std::path::Path, filename: &str, id: u64, name: &str) {
        let json = json!({ "id": id, "name": name });
        fs::write(
            dir.join(filename),
            serde_json::to_string_pretty(&json).unwrap(),
        )
        .unwrap();
    }

    // =========================================================================
    // P0: Core validation tests
    // =========================================================================

    #[test]
    fn test_p0_project_validator_new_with_populated_directory() {
        let dir = tempdir().unwrap();
        let sounds_dir = dir.path().join("sources/sounds");
        fs::create_dir_all(&sounds_dir).unwrap();

        write_sound_json(&sounds_dir, "footstep.json", 42, "footstep");
        write_sound_json(&sounds_dir, "explosion.json", 100, "explosion");

        let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();
        assert!(validator.validate_sound_exists(42).is_ok());
        assert!(validator.validate_sound_exists(100).is_ok());
    }

    #[test]
    fn test_p0_project_validator_empty_has_no_assets() {
        let validator = ProjectValidator::empty();
        // No assets registered, so any non-zero ID should fail
        assert!(validator.validate_sound_exists(1).is_err());
        // Zero ID should still succeed (no-op)
        assert!(validator.validate_sound_exists(0).is_ok());
    }

    #[test]
    fn test_p0_validate_sound_exists_ok_for_existing_id() {
        let dir = tempdir().unwrap();
        let sounds_dir = dir.path().join("sources/sounds");
        fs::create_dir_all(&sounds_dir).unwrap();
        write_sound_json(&sounds_dir, "footstep.json", 42, "footstep");

        let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();
        assert!(validator.validate_sound_exists(42).is_ok());
    }

    #[test]
    fn test_p0_validate_sound_exists_err_for_missing_id() {
        let dir = tempdir().unwrap();
        let sounds_dir = dir.path().join("sources/sounds");
        fs::create_dir_all(&sounds_dir).unwrap();
        write_sound_json(&sounds_dir, "footstep.json", 42, "footstep");

        let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();
        let result = validator.validate_sound_exists(999);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
        assert!(err.what().contains("Sound"));
        assert!(err.what().contains("999"));
        assert!(err.why().contains("does not exist"));
        assert!(err.suggestion().contains("Create"));
    }

    #[test]
    fn test_p0_validate_asset_exists_multiple_types() {
        let dir = tempdir().unwrap();

        // Create sounds
        let sounds_dir = dir.path().join("sources/sounds");
        fs::create_dir_all(&sounds_dir).unwrap();
        write_sound_json(&sounds_dir, "beep.json", 10, "beep");

        // Create effects
        let effects_dir = dir.path().join("sources/effects");
        fs::create_dir_all(&effects_dir).unwrap();
        write_minimal_asset_json(&effects_dir, "reverb.json", 20, "reverb");

        // Create collections
        let collections_dir = dir.path().join("sources/collections");
        fs::create_dir_all(&collections_dir).unwrap();
        write_minimal_asset_json(&collections_dir, "footsteps.json", 30, "footsteps");

        let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();

        // Each type validates independently
        assert!(validator.validate_sound_exists(10).is_ok());
        assert!(validator.validate_effect_exists(20).is_ok());
        assert!(validator.validate_collection_exists(30).is_ok());

        // Cross-type: sound ID doesn't satisfy effect check
        assert!(validator.validate_effect_exists(10).is_err());
        assert!(validator.validate_sound_exists(20).is_err());
    }

    // =========================================================================
    // P1: Edge case tests
    // =========================================================================

    #[test]
    fn test_p1_zero_id_is_always_ok() {
        let validator = ProjectValidator::empty();
        assert!(validator.validate_sound_exists(0).is_ok());
        assert!(validator.validate_collection_exists(0).is_ok());
        assert!(validator.validate_effect_exists(0).is_ok());
        assert!(validator.validate_switch_exists(0).is_ok());
        assert!(
            validator
                .validate_asset_exists(AssetType::Soundbank, 0)
                .is_ok()
        );
        assert!(validator.validate_asset_exists(AssetType::Event, 0).is_ok());
    }

    #[test]
    fn test_p1_missing_directories_not_an_error() {
        let dir = tempdir().unwrap();
        // Don't create any sources/ directories at all
        let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();
        // Should succeed (no crash), but no assets registered
        assert!(validator.validate_sound_exists(1).is_err());
    }

    #[test]
    fn test_p1_malformed_json_files_skipped() {
        let dir = tempdir().unwrap();
        let sounds_dir = dir.path().join("sources/sounds");
        fs::create_dir_all(&sounds_dir).unwrap();

        // Valid sound
        write_sound_json(&sounds_dir, "valid.json", 42, "valid_sound");

        // Malformed JSON
        fs::write(sounds_dir.join("broken.json"), "{ this is not valid json }").unwrap();

        // File without .json extension (should be ignored)
        fs::write(sounds_dir.join("readme.txt"), "not an asset").unwrap();

        let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();
        // Valid sound should be found despite broken file
        assert!(validator.validate_sound_exists(42).is_ok());
    }

    #[test]
    fn test_p1_empty_project_no_sources_dir() {
        let dir = tempdir().unwrap();
        // Project root exists but no sources/ directory at all
        let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();
        assert!(validator.validate_sound_exists(1).is_err());
        assert!(validator.validate_sound_exists(0).is_ok());
    }

    #[test]
    fn test_p1_validate_switch_state_exists_stub() {
        let dir = tempdir().unwrap();
        let switches_dir = dir.path().join("sources/switches");
        fs::create_dir_all(&switches_dir).unwrap();
        write_minimal_asset_json(&switches_dir, "surface.json", 50, "surface");

        let validator = ProjectValidator::new(dir.path().to_path_buf()).unwrap();

        // Switch exists - state validation is a no-op (stub)
        assert!(validator.validate_switch_state_exists(50, 1).is_ok());
        assert!(validator.validate_switch_state_exists(50, 999).is_ok());

        // Switch doesn't exist - should fail
        assert!(validator.validate_switch_state_exists(999, 1).is_err());

        // Zero switch ID - always ok
        assert!(validator.validate_switch_state_exists(0, 1).is_ok());

        // Valid switch, zero state ID - ok (stub; state_id=0 means "no state reference")
        assert!(validator.validate_switch_state_exists(50, 0).is_ok());
    }

    // =========================================================================
    // P2: Error message quality tests
    // =========================================================================

    #[test]
    fn test_p2_error_message_contains_what_why_suggestion() {
        let validator = ProjectValidator::empty();
        let err = validator.validate_sound_exists(42).unwrap_err();

        // What: describes the failed check
        assert!(!err.what().is_empty());
        assert!(err.what().contains("42"));

        // Why: explains the reason
        assert!(!err.why().is_empty());
        assert!(err.why().contains("does not exist"));

        // Suggestion: actionable fix
        assert!(!err.suggestion().is_empty());
        assert!(err.suggestion().contains("Create"));
    }

    #[test]
    fn test_p2_error_uses_correct_error_code() {
        let validator = ProjectValidator::empty();

        let err = validator.validate_effect_exists(99).unwrap_err();
        assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
        assert_eq!(err.layer, ValidationLayer::TypeRules);
    }
}
