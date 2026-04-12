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

//! Common utility functions for the Amplitude CLI.
//!
//! This module contains reusable utilities for project operations
//! that may be used across multiple commands.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::common::errors::{CliError, codes, project_not_initialized};
use crate::database::entities::ProjectConfiguration;

// =============================================================================
// String Truncation Utilities
// =============================================================================

/// Truncate a string to a maximum length, adding ellipsis if truncated.
///
/// Uses character-based (not byte-based) slicing to handle UTF-8 strings safely.
/// This prevents panics when truncating strings containing multi-byte characters.
///
/// # Arguments
/// * `s` - The string to truncate
/// * `max_len` - Maximum length in characters (including ellipsis if added)
///
/// # Returns
/// The original string if within limit, or truncated string with "..." appended.
///
/// # Example
/// ```ignore
/// assert_eq!(truncate_string("hello world", 8), "hello...");
/// assert_eq!(truncate_string("短い", 10), "短い");
/// ```
pub fn truncate_string(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Truncate a string at a word boundary for cleaner display.
///
/// Similar to `truncate_string` but tries to break at a space rather than
/// mid-word. Falls back to character truncation if no space is found.
///
/// # Arguments
/// * `s` - The string to truncate
/// * `max_len` - Maximum length in characters (including ellipsis if added)
///
/// # Returns
/// The original string if within limit, or truncated string with "..." appended.
pub fn truncate_string_at_word(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else {
        // Get the portion before the ellipsis
        let truncated: String = s.chars().take(max_len.saturating_sub(3)).collect();
        // Try to find a space to break at
        let break_point = truncated.rfind(' ').unwrap_or(truncated.len());
        let final_truncated: String = s.chars().take(break_point).collect();
        format!("{}...", final_truncated)
    }
}

// =============================================================================
// Name Validation Utilities
// =============================================================================

/// Validate a name for use as a project or template identifier.
///
/// Names must:
/// - Not be empty (after trimming whitespace)
/// - Only contain alphanumeric characters, hyphens, underscores, and optionally spaces
///
/// # Arguments
/// * `name` - The name to validate
/// * `allow_spaces` - Whether spaces are allowed (projects allow spaces, templates don't)
/// * `entity_type` - The type of entity being validated (for error messages)
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err(String)` with error message if invalid
pub fn validate_name(name: &str, allow_spaces: bool, entity_type: &str) -> Result<(), String> {
    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err(format!("{} name is required", entity_type));
    }

    let invalid_char = if allow_spaces {
        trimmed
            .chars()
            .any(|c| !c.is_alphanumeric() && c != '_' && c != '-' && c != ' ')
    } else {
        trimmed
            .chars()
            .any(|c| !c.is_alphanumeric() && c != '_' && c != '-')
    };

    if invalid_char {
        let allowed = if allow_spaces {
            "alphanumeric characters, underscores, hyphens, and spaces"
        } else {
            "alphanumeric characters, underscores, and hyphens"
        };
        return Err(format!(
            "The {} name must only contain {}.",
            entity_type, allowed
        ));
    }

    Ok(())
}

/// Validate a project name (allows spaces).
///
/// This is a convenience wrapper around `validate_name` for projects.
pub fn validate_project_name(name: &str) -> Result<(), String> {
    validate_name(name, true, "project")
}

/// Validate a template name (no spaces allowed).
///
/// This is a convenience wrapper around `validate_name` for templates.
pub fn validate_template_name(name: &str) -> Result<(), String> {
    validate_name(name, false, "template")
}

/// Asset type directory names within a project's sources folder.
pub const ASSET_DIR_ATTENUATORS: &str = "attenuators";
pub const ASSET_DIR_COLLECTIONS: &str = "collections";
pub const ASSET_DIR_EFFECTS: &str = "effects";
pub const ASSET_DIR_EVENTS: &str = "events";
pub const ASSET_DIR_PIPELINES: &str = "pipelines";
pub const ASSET_DIR_RTPC: &str = "rtpc";
pub const ASSET_DIR_SOUNDBANKS: &str = "soundbanks";
pub const ASSET_DIR_SOUNDS: &str = "sounds";
pub const ASSET_DIR_SWITCH_CONTAINERS: &str = "switch_containers";
pub const ASSET_DIR_SWITCHES: &str = "switches";

/// All asset type directories that can contain assets.
pub const ASSET_DIRECTORIES: &[&str] = &[
    ASSET_DIR_ATTENUATORS,
    ASSET_DIR_COLLECTIONS,
    ASSET_DIR_EFFECTS,
    ASSET_DIR_EVENTS,
    ASSET_DIR_PIPELINES,
    ASSET_DIR_RTPC,
    ASSET_DIR_SOUNDBANKS,
    ASSET_DIR_SOUNDS,
    ASSET_DIR_SWITCH_CONTAINERS,
    ASSET_DIR_SWITCHES,
];

/// Read and parse the `.amproject` file from the given directory.
///
/// # Arguments
/// * `path` - Path to the directory containing the `.amproject` file
///
/// # Returns
/// * `Ok(ProjectConfiguration)` - Successfully parsed project configuration
/// * `Err` - If file doesn't exist, is not readable, or contains invalid JSON
///
/// # Example
/// ```ignore
/// let config = read_amproject_file(Path::new("/path/to/project"))?;
/// println!("Project name: {}", config.name);
/// ```
pub fn read_amproject_file(path: &Path) -> anyhow::Result<ProjectConfiguration> {
    let amproject_path = path.join(".amproject");

    if !amproject_path.exists() {
        return Err(project_not_initialized(path.to_str().unwrap_or_default()).into());
    }

    let content = fs::read_to_string(&amproject_path).with_context(|| {
        format!(
            "Failed to read .amproject file at {}",
            amproject_path.display()
        )
    })?;

    let config: ProjectConfiguration = serde_json::from_str(&content).with_context(|| {
        format!(
            "Failed to parse .amproject file at {}",
            amproject_path.display()
        )
    })?;

    Ok(config)
}

/// Count assets by type in a project.
///
/// Scans the `sources/` directory for each asset type subdirectory and counts
/// the number of `.json` files in each.
///
/// # Arguments
/// * `project_path` - Path to the project root directory
///
/// # Returns
/// * `Ok(HashMap<String, usize>)` - Map of asset type names to counts
/// * `Err` - If there's an error reading the directories
///
/// # Example
/// ```ignore
/// let counts = count_assets_by_type(Path::new("/path/to/project"))?;
/// println!("Sounds: {}", counts.get("sounds").unwrap_or(&0));
/// ```
pub fn count_assets_by_type(project_path: &Path) -> anyhow::Result<HashMap<String, usize>> {
    let sources_dir = project_path.join("sources");
    let mut counts = HashMap::new();

    // Initialize all asset types with 0
    for &asset_type in ASSET_DIRECTORIES {
        counts.insert(asset_type.to_string(), 0);
    }

    // If sources directory doesn't exist, return empty counts
    if !sources_dir.exists() {
        return Ok(counts);
    }

    // Count .json files in each asset directory
    // Note: We follow symlinks (is_file() resolves symlinks) and only count regular files
    for &asset_type in ASSET_DIRECTORIES {
        let asset_dir = sources_dir.join(asset_type);
        if asset_dir.exists() && asset_dir.is_dir() {
            let count = fs::read_dir(&asset_dir)?
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    // Use file_type() to properly handle symlinks and special files
                    let file_type = match entry.file_type() {
                        Ok(ft) => ft,
                        Err(_) => return false,
                    };
                    // Only count regular files and symlinks pointing to files
                    // Skip directories, sockets, pipes, and other special files
                    let is_regular_file =
                        file_type.is_file() || (file_type.is_symlink() && entry.path().is_file());
                    is_regular_file && entry.path().extension().is_some_and(|ext| ext == "json")
                })
                .count();
            counts.insert(asset_type.to_string(), count);
        }
    }

    Ok(counts)
}

// =============================================================================
// Template Validation Utilities
// =============================================================================

/// Template manifest structure parsed from `template.json`.
///
/// The manifest is optional - templates can be registered without one.
/// When present, it provides metadata for the template.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemplateManifest {
    /// Template name (used if not provided via CLI flag)
    #[serde(default)]
    pub name: Option<String>,

    /// Target game engine (e.g., "generic", "o3de", "unreal")
    #[serde(default)]
    pub engine: Option<String>,

    /// Human-readable description of the template
    #[serde(default)]
    pub description: Option<String>,
}

/// Result of template directory validation.
#[derive(Debug)]
pub struct TemplateValidationResult {
    /// Parsed manifest if `template.json` exists
    pub manifest: Option<TemplateManifest>,

    /// List of files found in the template
    pub files: Vec<String>,
}

/// Validate that a directory is a valid template structure.
///
/// A valid template must contain:
/// - `.amproject` file (project configuration)
/// - At least one `*.buses.json` file
/// - At least one `*.config.json` file
///
/// # Arguments
/// * `path` - Path to the template directory
///
/// # Returns
/// * `Ok(TemplateValidationResult)` - Template is valid with optional manifest
/// * `Err` - Template is invalid with structured error explaining what's missing
pub fn validate_template_directory(path: &Path) -> anyhow::Result<TemplateValidationResult> {
    // Check path exists and is a directory
    if !path.exists() {
        return Err(CliError::new(
            codes::ERR_INVALID_TEMPLATE_STRUCTURE,
            format!("Template path '{}' does not exist", path.display()),
            "The specified path does not exist on the filesystem",
        )
        .with_suggestion("Verify the path is correct and the directory exists")
        .into());
    }

    if !path.is_dir() {
        return Err(CliError::new(
            codes::ERR_INVALID_TEMPLATE_STRUCTURE,
            format!("'{}' is not a directory", path.display()),
            "Templates must be directories, not files",
        )
        .with_suggestion("Provide a path to a template directory")
        .into());
    }

    // Collect all files in the template (for reporting)
    let mut files = Vec::new();
    let mut has_amproject = false;
    let mut has_buses_json = false;
    let mut has_config_json = false;

    // Scan directory for required files
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        if entry.file_type()?.is_file() {
            files.push(file_name.clone());

            // Check for required files
            if file_name == ".amproject" {
                has_amproject = true;
                // Validate that .amproject is valid JSON
                let amproject_path = path.join(".amproject");
                let content = fs::read_to_string(&amproject_path).with_context(|| {
                    format!(
                        "Failed to read .amproject file at {}",
                        amproject_path.display()
                    )
                })?;
                // Try to parse as JSON to validate structure
                let _: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
                    CliError::new(
                        codes::ERR_INVALID_TEMPLATE_STRUCTURE,
                        format!(".amproject file contains invalid JSON: {}", e),
                        "The .amproject file must be valid JSON",
                    )
                    .with_suggestion("Fix the JSON syntax in the .amproject file")
                })?;
            } else if file_name.ends_with(".buses.json") {
                has_buses_json = true;
            } else if file_name.ends_with(".config.json") {
                has_config_json = true;
            }
        } else if entry.file_type()?.is_dir() {
            files.push(format!("{}/", file_name));
        }
    }

    files.sort();

    // Build list of missing requirements
    let mut missing = Vec::new();
    if !has_amproject {
        missing.push(".amproject");
    }
    if !has_buses_json {
        missing.push("*.buses.json");
    }
    if !has_config_json {
        missing.push("*.config.json");
    }

    if !missing.is_empty() {
        return Err(CliError::new(
            codes::ERR_INVALID_TEMPLATE_STRUCTURE,
            format!(
                "Template directory missing required file(s): {}",
                missing.join(", ")
            ),
            "A valid template must contain .amproject, *.buses.json, and *.config.json",
        )
        .with_suggestion("Ensure template directory contains all required files")
        .into());
    }

    // Parse optional manifest
    let manifest = parse_template_manifest(path)?;

    Ok(TemplateValidationResult { manifest, files })
}

/// Parse the optional `template.json` manifest from a template directory.
///
/// # Arguments
/// * `path` - Path to the template directory
///
/// # Returns
/// * `Ok(Some(TemplateManifest))` - Manifest found and parsed successfully
/// * `Ok(None)` - No manifest file present (this is valid)
/// * `Err` - Manifest exists but is invalid JSON
pub fn parse_template_manifest(path: &Path) -> anyhow::Result<Option<TemplateManifest>> {
    let manifest_path = path.join("template.json");

    if !manifest_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&manifest_path).with_context(|| {
        format!(
            "Failed to read template manifest at {}",
            manifest_path.display()
        )
    })?;

    let manifest: TemplateManifest = serde_json::from_str(&content).with_context(|| {
        format!(
            "Failed to parse template manifest at {}",
            manifest_path.display()
        )
    })?;

    Ok(Some(manifest))
}

/// Generate a unique ID for an asset.
///
/// Uses a combination of the asset name and current timestamp to generate
/// a unique u64 identifier.
pub fn generate_unique_id(name: &str) -> u64 {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    timestamp.hash(&mut hasher);
    hasher.finish()
}
