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

use super::{AssetType, ValidationError, ValidationLayer};
use crate::common::errors::codes;
use crate::forms::references::RefTarget;
use crate::presentation::Output;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Runtime binary extensions mapped to their source equivalent.
/// Soundbank references use these runtime names; the validator needs to
/// convert them back to `.json` to find the source file on disk.
const RUNTIME_EXTENSIONS: &[&str] = &[
    ".amsound",
    ".amcollection",
    ".ambank",
    ".amevent",
    ".ampipeline",
    ".amattenuation",
    ".amswitch",
    ".amswitchcontainer",
    ".amrtpc",
    ".amenv",
    ".amfx",
    ".amconfig",
    ".ambus",
];

/// Convert a runtime binary asset path to its source `.json` path.
///
/// Example: `sounds/throw_01.wav.amsound` → `sounds/throw_01.wav.json`
fn runtime_path_to_source(path: &str) -> String {
    for ext in RUNTIME_EXTENSIONS {
        if path.ends_with(ext) {
            return format!("{}.json", &path[..path.len() - ext.len()]);
        }
    }
    path.to_string()
}

/// An (id, name) pair identifying a referenceable object (asset or bus).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRef {
    pub id: u64,
    pub name: String,
}

fn normalize_asset_path(path: &str) -> String {
    path.replace('\\', "/")
}

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
    /// Resolved sources directory (from .amproject `sources_dir` field).
    sources_dir: PathBuf,
    /// All known asset IDs grouped by type.
    pub(crate) asset_ids: HashMap<AssetType, HashSet<u64>>,
    /// All known asset names grouped by type.
    pub(crate) asset_names: HashMap<AssetType, HashSet<String>>,
    /// Maps each asset ID to the asset type directory it was found in.
    asset_locations: HashMap<u64, AssetType>,
    /// All known asset file paths relative to the project sources directory.
    /// Used for path-based reference validation (e.g., soundbank asset references).
    asset_paths: HashSet<String>,
    /// (id, name) pairs per asset type, for pick-lists and reference resolution.
    asset_refs: HashMap<AssetType, Vec<AssetRef>>,
    /// The states of each switch asset, by switch id, in file order.
    switch_states: HashMap<u64, Vec<AssetRef>>,
    /// Buses defined in the `*.buses.json` files of the project root.
    buses: Vec<AssetRef>,
    /// The buses of each bus file, by file name (without `.buses.json`).
    bus_files: Vec<(String, Vec<AssetRef>)>,
    /// The bus file all engine configs load, when they agree on one.
    active_bus_file: Option<String>,
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
    pub fn new(project_root: PathBuf, output: &dyn Output) -> anyhow::Result<Self> {
        // Read .amproject to get the configured sources directory
        let sources_dir = match crate::common::utils::read_amproject_file(&project_root) {
            Ok(config) => {
                if config.sources_dir.is_empty() {
                    project_root.clone()
                } else {
                    project_root.join(&config.sources_dir)
                }
            }
            Err(_) => project_root.join("sources"), // fallback for projects without .amproject
        };

        let mut validator = Self {
            project_root,
            sources_dir,
            asset_ids: HashMap::new(),
            asset_names: HashMap::new(),
            asset_locations: HashMap::new(),
            asset_paths: HashSet::new(),
            asset_refs: HashMap::new(),
            switch_states: HashMap::new(),
            buses: Vec::new(),
            bus_files: Vec::new(),
            active_bus_file: None,
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
            AssetType::Attenuation,
            AssetType::Rtpc,
            AssetType::Pipeline,
        ];

        for asset_type in &asset_types {
            validator.scan_assets_of_type(*asset_type, output)?;
        }

        validator.scan_buses(output);

        Ok(validator)
    }

    /// Creates an empty `ProjectValidator` with no registered assets.
    ///
    /// Useful for tests and contexts where disk scanning is not needed.
    pub fn empty() -> Self {
        Self {
            project_root: PathBuf::new(),
            sources_dir: PathBuf::new(),
            asset_ids: HashMap::new(),
            asset_names: HashMap::new(),
            asset_locations: HashMap::new(),
            asset_paths: HashSet::new(),
            asset_refs: HashMap::new(),
            switch_states: HashMap::new(),
            buses: Vec::new(),
            bus_files: Vec::new(),
            active_bus_file: None,
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
            // Check if the ID exists but in a different type directory
            if let Err(failure) = self.validate_asset_in_correct_directory(asset_type, id) {
                return Err(failure);
            }

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

    /// Validates that an asset ID is located in the correct type directory.
    ///
    /// Returns `Ok(())` if:
    /// - `id == 0` (zero means "no reference")
    /// - The ID is not found (validated separately by `validate_asset_exists`)
    /// - The ID is in the correct type directory
    ///
    /// Returns `Err` if the ID exists but in a different type directory.
    pub fn validate_asset_in_correct_directory(
        &self,
        asset_type: AssetType,
        id: u64,
    ) -> Result<(), ValidationError> {
        if id == 0 {
            return Ok(());
        }

        if let Some(actual_type) = self.asset_locations.get(&id) {
            if *actual_type != asset_type {
                return Err(ValidationError::new(
                    ValidationLayer::TypeRules,
                    codes::ERR_VALIDATION_REFERENCE,
                    format!(
                        "Asset ID {} exists but is a {}, not a {}",
                        id, actual_type, asset_type
                    ),
                    format!(
                        "The asset was found in sources/{} instead of sources/{}",
                        actual_type.directory_name(),
                        asset_type.directory_name()
                    ),
                )
                .with_suggestion(format!(
                    "Move the asset to sources/{} or use the correct asset type reference",
                    asset_type.directory_name()
                )));
            }
        }

        Ok(())
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

    /// Checks if an asset ID references a playable asset type.
    ///
    /// Playable assets are: Sound, Collection, and SwitchContainer.
    /// These are the asset types that can be targeted by Play and Resume event actions.
    ///
    /// Returns `true` if the ID exists and is one of the playable asset types.
    /// Zero ID returns `false` (not a playable asset).
    pub fn is_playable_asset(&self, id: u64) -> bool {
        if id == 0 {
            return false;
        }

        if let Some(asset_type) = self.asset_locations.get(&id) {
            matches!(
                asset_type,
                AssetType::Sound | AssetType::Collection | AssetType::SwitchContainer
            )
        } else {
            false
        }
    }

    /// Validates that `state_id` is a state of the switch `switch_id`.
    ///
    /// Returns `Ok(())` if `switch_id == 0` or `state_id == 0` (no reference). The switch itself must
    /// exist; a state of another switch is a reference error that lists the states
    /// the switch does have.
    pub fn validate_switch_state_exists(
        &self,
        switch_id: u64,
        state_id: u64,
    ) -> Result<(), ValidationError> {
        if switch_id == 0 {
            return Ok(());
        }
        self.validate_switch_exists(switch_id)?;
        if state_id == 0 {
            return Ok(());
        }
        let states = self.switch_states(switch_id).unwrap_or(&[]);
        if states.iter().any(|s| s.id == state_id) {
            return Ok(());
        }
        Err(ValidationError::new(
            ValidationLayer::TypeRules,
            codes::ERR_VALIDATION_REFERENCE,
            format!("Switch state with ID {state_id} not found in the switch"),
            format!("The switch has no state with ID {state_id}"),
        )
        .with_suggestion(self.describe_states(switch_id)))
    }

    /// The states of the switch `switch_id`, or `None` when no such switch exists.
    pub fn switch_states(&self, switch_id: u64) -> Option<&[AssetRef]> {
        self.switch_states.get(&switch_id).map(Vec::as_slice)
    }

    /// "Valid states of switch 'x': a, b" (or a hint when the switch has none), for error messages.
    pub fn describe_states(&self, switch_id: u64) -> String {
        let switch = self.resolve(RefTarget::Asset(AssetType::Switch), switch_id);
        let name = switch.map_or_else(|| format!("ID {switch_id}"), |s| format!("'{}'", s.name));
        match self.switch_states(switch_id) {
            Some(states) if !states.is_empty() => format!(
                "Valid states of switch {name}: {}",
                states
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => format!("Switch {name} has no states; add some with 'am asset switch update'"),
        }
    }

    /// Checks if an asset exists at the given path relative to the sources directory.
    ///
    /// Supports both source paths (`sounds/footstep.json`) and runtime binary paths
    /// (`sounds/footstep.amsound`). Runtime extensions are mapped back to `.json` for
    /// lookup since the validator scans source files on disk.
    pub fn asset_exists_by_path(&self, path: &str) -> bool {
        let path = normalize_asset_path(path);
        if self.asset_paths.contains(&path) {
            return true;
        }

        // Map runtime binary extensions back to .json for source file lookup
        let json_path = runtime_path_to_source(&path);
        if json_path != path {
            return self.asset_paths.contains(json_path.as_str());
        }

        false
    }

    /// Lists every object a reference to `target` can point at, sorted by name.
    pub fn list(&self, target: RefTarget) -> Vec<AssetRef> {
        let mut refs: Vec<AssetRef> = target
            .members()
            .into_iter()
            .flat_map(|member| match member {
                RefTarget::Asset(asset_type) => {
                    self.asset_refs.get(&asset_type).cloned().unwrap_or_default()
                }
                RefTarget::SwitchState => self.switch_states.values().flatten().cloned().collect(),
                _ => self.visible_buses().clone(),
            })
            .collect();
        refs.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        refs
    }

    /// The buses a reference should offer: those of the active bus file when every
    /// engine config loads the same known file, else those of all bus files.
    fn visible_buses(&self) -> &Vec<AssetRef> {
        self.active_bus_file
            .as_deref()
            .and_then(|active| self.bus_files.iter().find(|(name, _)| name == active))
            .map_or(&self.buses, |(_, buses)| buses)
    }

    /// The bus file all engine configs agree on, if any (see [`Self::list`]).
    pub fn active_bus_file(&self) -> Option<&str> {
        self.active_bus_file
            .as_deref()
            .filter(|active| self.bus_files.iter().any(|(name, _)| name == active))
    }

    /// Finds a bus in any bus file, by name or by ID, ignoring the active bus file.
    ///
    /// Used to warn (rather than fail) about buses outside the active file.
    pub fn find_bus_anywhere(&self, name: Option<&str>, id: Option<u64>) -> Option<AssetRef> {
        self.buses
            .iter()
            .find(|b| name.is_some_and(|n| b.name == n) || id.is_some_and(|i| b.id == i))
            .cloned()
    }

    /// Whether the bus `id` exists only outside the active bus file.
    pub fn bus_outside_active(&self, id: u64) -> bool {
        self.active_bus_file().is_some()
            && self.resolve(RefTarget::Bus, id).is_none()
            && self.find_bus_anywhere(None, Some(id)).is_some()
    }

    /// Finds the object `id` refers to. `id == 0` ("no reference") is never resolved.
    pub fn resolve(&self, target: RefTarget, id: u64) -> Option<AssetRef> {
        if id == 0 {
            return None;
        }
        target.members().into_iter().find_map(|member| {
            let source = match member {
                RefTarget::Asset(asset_type) => self.asset_refs.get(&asset_type)?,
                RefTarget::SwitchState => {
                    return self
                        .switch_states
                        .values()
                        .flatten()
                        .find(|r| r.id == id)
                        .cloned();
                }
                _ => self.visible_buses(),
            };
            source.iter().find(|r| r.id == id).cloned()
        })
    }

    /// Returns the project root path.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Returns the resolved sources directory path.
    pub fn sources_dir(&self) -> &Path {
        &self.sources_dir
    }

    /// Indexes the buses of every `*.buses.json` file in the project root
    /// (non-recursive). Unreadable or malformed files are skipped with a warning.
    fn scan_buses(&mut self, output: &dyn Output) {
        let Ok(entries) = fs::read_dir(&self.project_root) else {
            return;
        };

        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(".buses.json"))
            })
            .collect();
        files.sort();

        for path in files {
            let value = match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(v) => v,
                    Err(err) => {
                        output.warning(&format!(
                            "Malformed JSON in buses file {}: {}",
                            path.display(),
                            err
                        ));
                        continue;
                    }
                },
                Err(err) => {
                    output.warning(&format!(
                        "Failed to read buses file {}: {}",
                        path.display(),
                        err
                    ));
                    continue;
                }
            };

            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".buses.json"))
                .unwrap_or_default()
                .to_string();
            let mut file_buses = Vec::new();
            let buses = value.get("buses").and_then(|b| b.as_array());
            for bus in buses.into_iter().flatten() {
                if let (Some(id), Some(name)) = (
                    bus.get("id").and_then(|v| v.as_u64()),
                    bus.get("name").and_then(|v| v.as_str()),
                ) {
                    file_buses.push(AssetRef {
                        id,
                        name: name.to_string(),
                    });
                }
            }
            self.buses.extend(file_buses.iter().cloned());
            self.bus_files.push((file_name, file_buses));
        }

        // Engine configs name the bus file the engine loads.
        if let Ok(scan) = super::engine_config::scan_engine_configs(&self.project_root) {
            self.active_bus_file = super::engine_config::active_bus_file(&scan.files);
        }
    }

    /// Scans a single asset type directory and populates the registries.
    ///
    /// Reads all `.json` files from `sources/<type>/`, extracting `id` and
    /// `name` fields using `serde_json::Value` (avoids requiring full struct
    /// deserialization for asset types not yet implemented).
    ///
    /// Symlinks are followed when scanning. Symlinked JSON files are processed
    /// as regular files.
    fn scan_assets_of_type(
        &mut self,
        asset_type: AssetType,
        output: &dyn Output,
    ) -> anyhow::Result<()> {
        let dir = self.sources_dir.join(asset_type.directory_name());

        if !dir.exists() {
            return Ok(());
        }

        // Walk recursively to handle subdirectories (e.g., sounds/footsteps/grass/)
        for entry in walkdir::WalkDir::new(&dir).into_iter().flatten() {
            let path = entry.into_path();

            // Only process .json files
            if !path.is_file() || path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }

            // Read and parse the JSON file
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(err) => {
                    output.warning(&format!(
                        "Failed to read asset file {}: {}",
                        path.display(),
                        err
                    ));
                    continue;
                }
            };

            let value: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(err) => {
                    output.warning(&format!(
                        "Malformed JSON in asset file {}: {}",
                        path.display(),
                        err
                    ));
                    continue;
                }
            };

            // Extract id (u64) and name (String)
            if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                self.asset_ids.entry(asset_type).or_default().insert(id);
                self.asset_locations.insert(id, asset_type);
            }

            if let (Some(id), Some(name)) = (
                value.get("id").and_then(|v| v.as_u64()),
                value.get("name").and_then(|v| v.as_str()),
            ) {
                self.asset_refs.entry(asset_type).or_default().push(AssetRef {
                    id,
                    name: name.to_string(),
                });
            }

            if asset_type == AssetType::Switch
                && let Some(id) = value.get("id").and_then(|v| v.as_u64())
            {
                let states = value
                    .get("states")
                    .and_then(|s| s.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|s| {
                        Some(AssetRef {
                            id: s.get("id")?.as_u64()?,
                            name: s.get("name")?.as_str()?.to_string(),
                        })
                    })
                    .collect();
                self.switch_states.insert(id, states);
            }

            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                self.asset_names
                    .entry(asset_type)
                    .or_default()
                    .insert(name.to_string());
            }

            // Track the relative path from sources dir for path-based lookups
            if let Ok(relative) = path.strip_prefix(&self.sources_dir) {
                self.asset_paths
                    .insert(normalize_asset_path(&relative.to_string_lossy()));
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

    #[test]
    fn test_asset_exists_by_path_normalizes_path_separators() {
        let dir = tempdir().unwrap();
        let nested_sounds = dir.path().join("sources/sounds/nested");
        let nested_attenuators = dir.path().join("sources/attenuators/nested");
        fs::create_dir_all(&nested_sounds).unwrap();
        fs::create_dir_all(&nested_attenuators).unwrap();
        write_minimal_asset_json(&nested_sounds, "footstep.json", 42, "footstep");
        write_minimal_asset_json(&nested_attenuators, "room.json", 43, "room");

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();

        for separator in ["/", "\\"] {
            assert!(
                validator.asset_exists_by_path(&format!(
                    "sounds{separator}nested{separator}footstep.json"
                ))
            );
            assert!(validator.asset_exists_by_path(&format!(
                "sounds{separator}nested{separator}footstep.amsound"
            )));
            assert!(validator.asset_exists_by_path(&format!(
                "attenuators{separator}nested{separator}room.json"
            )));
            assert!(validator.asset_exists_by_path(&format!(
                "attenuators{separator}nested{separator}room.amattenuation"
            )));
        }
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

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();
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

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();
        assert!(validator.validate_sound_exists(42).is_ok());
    }

    #[test]
    fn test_p0_validate_sound_exists_err_for_missing_id() {
        let dir = tempdir().unwrap();
        let sounds_dir = dir.path().join("sources/sounds");
        fs::create_dir_all(&sounds_dir).unwrap();
        write_sound_json(&sounds_dir, "footstep.json", 42, "footstep");

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();
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

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();

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
        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();
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

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();
        // Valid sound should be found despite broken file
        assert!(validator.validate_sound_exists(42).is_ok());
    }

    #[test]
    fn test_p1_empty_project_no_sources_dir() {
        let dir = tempdir().unwrap();
        // Project root exists but no sources/ directory at all
        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();
        assert!(validator.validate_sound_exists(1).is_err());
        assert!(validator.validate_sound_exists(0).is_ok());
    }

    #[test]
    fn test_p1_validate_switch_state_exists() {
        let dir = tempdir().unwrap();
        let switches_dir = dir.path().join("sources/switches");
        fs::create_dir_all(&switches_dir).unwrap();
        fs::write(
            switches_dir.join("surface.json"),
            r#"{"id": 50, "name": "surface", "states": [{"id": 1, "name": "wood"}]}"#,
        )
        .unwrap();

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();

        // The state belongs to the switch; an unknown one is a reference error
        assert!(validator.validate_switch_state_exists(50, 1).is_ok());
        assert!(validator.validate_switch_state_exists(50, 999).is_err());

        // Switch doesn't exist - should fail
        assert!(validator.validate_switch_state_exists(999, 1).is_err());

        // Zero switch ID - always ok
        assert!(validator.validate_switch_state_exists(0, 1).is_ok());

        // Valid switch, zero state ID - ok (state_id=0 means "no state reference")
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

    // =========================================================================
    // P1: Type-directory mismatch tests
    // =========================================================================

    #[test]
    fn test_p1_type_directory_mismatch_detected() {
        let dir = tempdir().unwrap();

        // Place a sound JSON in the effects directory (wrong type)
        let effects_dir = dir.path().join("sources/effects");
        fs::create_dir_all(&effects_dir).unwrap();
        write_sound_json(&effects_dir, "misplaced.json", 100, "misplaced");

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();

        // ID 100 is in effects, so asking for it as a Sound should fail
        let result = validator.validate_sound_exists(100);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.what().contains("Effect"));
        assert!(err.what().contains("not a Sound"));
    }

    #[test]
    fn test_p1_type_directory_correct_passes() {
        let dir = tempdir().unwrap();

        let sounds_dir = dir.path().join("sources/sounds");
        fs::create_dir_all(&sounds_dir).unwrap();
        write_sound_json(&sounds_dir, "correct.json", 200, "correct");

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();

        // ID 200 is in sounds, so asking for it as a Sound should pass
        assert!(validator.validate_sound_exists(200).is_ok());
    }

    #[test]
    fn test_p1_validate_asset_in_correct_directory() {
        let dir = tempdir().unwrap();

        let sounds_dir = dir.path().join("sources/sounds");
        fs::create_dir_all(&sounds_dir).unwrap();
        write_sound_json(&sounds_dir, "beep.json", 10, "beep");

        let effects_dir = dir.path().join("sources/effects");
        fs::create_dir_all(&effects_dir).unwrap();
        write_minimal_asset_json(&effects_dir, "reverb.json", 20, "reverb");

        let validator = ProjectValidator::new(
            dir.path().to_path_buf(),
            &crate::presentation::JsonOutput::new(),
        )
        .unwrap();

        // Correct directory
        assert!(
            validator
                .validate_asset_in_correct_directory(AssetType::Sound, 10)
                .is_ok()
        );
        assert!(
            validator
                .validate_asset_in_correct_directory(AssetType::Effect, 20)
                .is_ok()
        );

        // Wrong directory
        let err = validator
            .validate_asset_in_correct_directory(AssetType::Effect, 10)
            .unwrap_err();
        assert!(err.what().contains("Sound"));
        assert!(err.what().contains("not a Effect"));

        // Zero ID always ok
        assert!(
            validator
                .validate_asset_in_correct_directory(AssetType::Sound, 0)
                .is_ok()
        );

        // Unknown ID is ok (not found = not a mismatch)
        assert!(
            validator
                .validate_asset_in_correct_directory(AssetType::Sound, 999)
                .is_ok()
        );
    }

    #[test]
    fn test_p2_error_uses_correct_error_code() {
        let validator = ProjectValidator::empty();

        let err = validator.validate_effect_exists(99).unwrap_err();
        assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
        assert_eq!(err.layer, ValidationLayer::TypeRules);
    }
}
