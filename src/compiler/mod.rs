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

//! Project compilation orchestration.
//!
//! Ties the FlatBuffers compiler to the Amplitude project structure,
//! handling file discovery, incremental rebuild checks, and output
//! path mapping for all SDK asset types.

pub mod flatc;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use walkdir::WalkDir;

use crate::config::sdk::SdkLocation;

/// Maps a source asset type to its schema and output format.
#[derive(Debug, Clone)]
pub struct ConversionEntry {
    /// Subdirectory under `sources/` (empty string for root-level files).
    pub subdir: String,
    /// Glob-like suffix pattern (e.g. `"*.config.json"` or `"**/*.json"`).
    pub pattern_suffix: String,
    /// Schema filename inside `<sdk>/schemas/` (e.g. `"sound_definition.bfbs"`).
    pub schema_file: String,
    /// Binary output extension (e.g. `".amsound"`).
    pub output_extension: String,
}

/// Summary of a project build run.
#[derive(Debug, Default)]
pub struct BuildSummary {
    /// Number of files successfully compiled.
    pub compiled: usize,
    /// Number of files skipped (already up to date).
    pub skipped: usize,
    /// Total bytes written across all compiled files.
    pub total_bytes: u64,
    /// Count of compiled files per asset type (keyed by subdir or pattern).
    pub type_counts: HashMap<String, usize>,
    /// Files that failed to compile: `(source_path, error_message)`.
    pub errors: Vec<(String, String)>,
}

/// Returns the full list of conversion entries for an Amplitude project.
///
/// Each entry describes how one category of JSON asset maps to a binary
/// output file via its FlatBuffers schema.
pub fn get_conversion_entries() -> Vec<ConversionEntry> {
    vec![
        ConversionEntry {
            subdir: String::new(),
            pattern_suffix: "*.config.json".into(),
            schema_file: "engine_config_definition.bfbs".into(),
            output_extension: ".amconfig".into(),
        },
        ConversionEntry {
            subdir: String::new(),
            pattern_suffix: "*.buses.json".into(),
            schema_file: "buses_definition.bfbs".into(),
            output_extension: ".ambus".into(),
        },
        ConversionEntry {
            subdir: "sounds".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "sound_definition.bfbs".into(),
            output_extension: ".amsound".into(),
        },
        ConversionEntry {
            subdir: "collections".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "collection_definition.bfbs".into(),
            output_extension: ".amcollection".into(),
        },
        ConversionEntry {
            subdir: "soundbanks".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "sound_bank_definition.bfbs".into(),
            output_extension: ".ambank".into(),
        },
        ConversionEntry {
            subdir: "events".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "event_definition.bfbs".into(),
            output_extension: ".amevent".into(),
        },
        ConversionEntry {
            subdir: "pipelines".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "pipeline_definition.bfbs".into(),
            output_extension: ".ampipeline".into(),
        },
        ConversionEntry {
            subdir: "attenuators".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "attenuation_definition.bfbs".into(),
            output_extension: ".amattenuation".into(),
        },
        ConversionEntry {
            subdir: "switches".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "switch_definition.bfbs".into(),
            output_extension: ".amswitch".into(),
        },
        ConversionEntry {
            subdir: "switch_containers".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "switch_container_definition.bfbs".into(),
            output_extension: ".amswitchcontainer".into(),
        },
        ConversionEntry {
            subdir: "rtpc".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "rtpc_definition.bfbs".into(),
            output_extension: ".amrtpc".into(),
        },
        ConversionEntry {
            subdir: "effects".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "effect_definition.bfbs".into(),
            output_extension: ".amenv".into(),
        },
    ]
}

/// Check whether a source file needs to be recompiled.
///
/// Returns `true` when:
/// - The target file does not exist, or
/// - The source file is newer than the target, or
/// - The schema file is newer than the target.
pub fn needs_rebuild(source: &Path, schema: &Path, target: &Path) -> bool {
    let target_mtime = match fs::metadata(target).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return true, // Target doesn't exist.
    };

    let is_newer = |path: &Path, reference: SystemTime| -> bool {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| t > reference)
            .unwrap_or(true)
    };

    is_newer(source, target_mtime) || is_newer(schema, target_mtime)
}

/// Compute the output path for a compiled asset.
///
/// Preserves the relative directory structure from `sources_dir` into
/// `build_dir`, replacing only the `.json` extension with `output_extension`.
///
/// For example, `sources/sounds/fx/boom.json` with extension `.amsound`
/// becomes `build/sounds/fx/boom.amsound`. For root files like
/// `pc.config.json`, the output is `pc.config.amconfig`.
pub fn output_path_for(
    source: &Path,
    sources_dir: &Path,
    build_dir: &Path,
    output_extension: &str,
) -> PathBuf {
    let relative = source
        .strip_prefix(sources_dir)
        .unwrap_or(source.file_name().map(Path::new).unwrap_or(source));

    // Strip only the trailing `.json` extension, keep everything else.
    let stem = relative.to_string_lossy();
    let without_json = stem.strip_suffix(".json").unwrap_or(&stem);

    build_dir.join(format!("{}{}", without_json, output_extension))
}

/// Discover source files matching a conversion entry.
///
/// For root-level entries (empty `subdir`), matches files in `sources_dir`
/// by the pattern suffix. For subdirectory entries, recursively walks the
/// subdirectory collecting all `.json` files.
pub fn discover_files(sources_dir: &Path, entry: &ConversionEntry) -> Vec<PathBuf> {
    if entry.subdir.is_empty() {
        // Root-level pattern matching (e.g. "*.config.json").
        let suffix = entry
            .pattern_suffix
            .strip_prefix('*')
            .unwrap_or(&entry.pattern_suffix);

        match fs::read_dir(sources_dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| n.ends_with(suffix))
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    } else {
        let subdir_path = sources_dir.join(&entry.subdir);
        if !subdir_path.is_dir() {
            return Vec::new();
        }

        WalkDir::new(&subdir_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext == "json")
            })
            .map(|e| e.into_path())
            .collect()
    }
}

/// Compile all assets in a project from JSON to FlatBuffers binary.
///
/// Iterates every conversion entry, discovers matching source files,
/// checks for incremental rebuild, compiles changed files, and writes
/// the binary output to `build_dir`.
///
/// When `fail_fast` is `true`, the first compilation error aborts the
/// entire build. Otherwise errors are collected in `BuildSummary.errors`.
pub fn compile_project(
    sources_dir: &Path,
    build_dir: &Path,
    sdk: &SdkLocation,
    fail_fast: bool,
    output: &dyn crate::presentation::Output,
) -> Result<BuildSummary> {
    let entries = get_conversion_entries();
    let mut summary = BuildSummary::default();

    for entry in &entries {
        let schema_path = sdk.schemas_dir().join(&entry.schema_file);
        let schema_bytes = match fs::read(&schema_path) {
            Ok(b) => b,
            Err(e) => {
                let msg = format!("failed to read schema '{}': {}", schema_path.display(), e);
                output.warning(&msg);
                // Skip this entry entirely — schema not available.
                continue;
            }
        };

        let files = discover_files(sources_dir, entry);
        let type_key = if entry.subdir.is_empty() {
            entry.pattern_suffix.clone()
        } else {
            entry.subdir.clone()
        };

        for source in &files {
            let target = output_path_for(source, sources_dir, build_dir, &entry.output_extension);

            if !needs_rebuild(source, &schema_path, &target) {
                summary.skipped += 1;
                continue;
            }

            let json_str = match fs::read_to_string(source) {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!("failed to read '{}': {}", source.display(), e);
                    if fail_fast {
                        return Err(e).context(msg);
                    }
                    summary.errors.push((source.display().to_string(), msg));
                    continue;
                }
            };

            match flatc::compile_json_to_binary(&schema_bytes, &json_str, output) {
                Ok(binary) => {
                    // Ensure output directory exists.
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("failed to create output directory '{}'", parent.display())
                        })?;
                    }

                    let bytes_written = binary.len() as u64;
                    fs::write(&target, &binary)
                        .with_context(|| format!("failed to write '{}'", target.display()))?;

                    summary.compiled += 1;
                    summary.total_bytes += bytes_written;
                    *summary.type_counts.entry(type_key.clone()).or_insert(0) += 1;

                    log::debug!(
                        "compiled {} -> {} ({} bytes)",
                        source.display(),
                        target.display(),
                        bytes_written
                    );
                }
                Err(e) => {
                    let msg = format!("failed to compile '{}': {}", source.display(), e);
                    if fail_fast {
                        anyhow::bail!("{}", msg);
                    }
                    log::error!("{}", msg);
                    summary.errors.push((source.display().to_string(), msg));
                }
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_get_conversion_entries_count() {
        let entries = get_conversion_entries();
        assert_eq!(entries.len(), 12);
    }

    #[test]
    fn test_get_conversion_entries_first_and_last() {
        let entries = get_conversion_entries();

        assert!(entries[0].subdir.is_empty());
        assert_eq!(entries[0].pattern_suffix, "*.config.json");
        assert_eq!(entries[0].schema_file, "engine_config_definition.bfbs");
        assert_eq!(entries[0].output_extension, ".amconfig");

        assert_eq!(entries[11].subdir, "effects");
        assert_eq!(entries[11].schema_file, "effect_definition.bfbs");
        assert_eq!(entries[11].output_extension, ".amenv");
    }

    #[test]
    fn test_needs_rebuild_missing_target() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("test.json");
        let schema = dir.path().join("test.bfbs");
        let target = dir.path().join("test.bin");

        fs::write(&source, "{}").unwrap();
        fs::write(&schema, &[0u8]).unwrap();
        // target does not exist
        assert!(needs_rebuild(&source, &schema, &target));
    }

    #[test]
    fn test_needs_rebuild_up_to_date() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("test.json");
        let schema = dir.path().join("test.bfbs");
        let target = dir.path().join("test.bin");

        fs::write(&source, "{}").unwrap();
        fs::write(&schema, &[0u8]).unwrap();
        // Write target after source and schema.
        std::thread::sleep(std::time::Duration::from_millis(50));
        fs::write(&target, &[0u8]).unwrap();

        assert!(!needs_rebuild(&source, &schema, &target));
    }

    #[test]
    fn test_output_path_for_subdir() {
        let sources = Path::new("/project/sources");
        let build = Path::new("/project/build");
        let source = Path::new("/project/sources/sounds/fx/boom.json");

        let result = output_path_for(source, sources, build, ".amsound");
        assert_eq!(
            result,
            PathBuf::from("/project/build/sounds/fx/boom.amsound")
        );
    }

    #[test]
    fn test_output_path_for_root_config() {
        let sources = Path::new("/project/sources");
        let build = Path::new("/project/build");
        let source = Path::new("/project/sources/pc.config.json");

        let result = output_path_for(source, sources, build, ".amconfig");
        assert_eq!(result, PathBuf::from("/project/build/pc.config.amconfig"));
    }

    #[test]
    fn test_discover_files_root_pattern() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("pc.config.json"), "{}").unwrap();
        fs::write(dir.path().join("other.json"), "{}").unwrap();
        fs::write(dir.path().join("readme.txt"), "hi").unwrap();

        let entry = ConversionEntry {
            subdir: String::new(),
            pattern_suffix: "*.config.json".into(),
            schema_file: "test.bfbs".into(),
            output_extension: ".amconfig".into(),
        };

        let files = discover_files(dir.path(), &entry);
        assert_eq!(files.len(), 1);
        assert!(
            files[0]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(".config.json")
        );
    }

    #[test]
    fn test_discover_files_subdir() {
        let dir = tempdir().unwrap();
        let sounds = dir.path().join("sounds");
        let nested = sounds.join("fx");
        fs::create_dir_all(&nested).unwrap();
        fs::write(sounds.join("music.json"), "{}").unwrap();
        fs::write(nested.join("boom.json"), "{}").unwrap();
        fs::write(nested.join("readme.txt"), "nope").unwrap();

        let entry = ConversionEntry {
            subdir: "sounds".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "test.bfbs".into(),
            output_extension: ".amsound".into(),
        };

        let files = discover_files(dir.path(), &entry);
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_discover_files_missing_subdir() {
        let dir = tempdir().unwrap();
        let entry = ConversionEntry {
            subdir: "nonexistent".into(),
            pattern_suffix: "**/*.json".into(),
            schema_file: "test.bfbs".into(),
            output_extension: ".amsound".into(),
        };

        let files = discover_files(dir.path(), &entry);
        assert!(files.is_empty());
    }
}
