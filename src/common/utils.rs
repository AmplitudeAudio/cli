//! Common utility functions for the Amplitude CLI.
//!
//! This module contains reusable utilities for project operations
//! that may be used across multiple commands.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Context;

use crate::common::errors::project_not_initialized;
use crate::database::entities::ProjectConfiguration;

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
