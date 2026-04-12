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

//! SDK discovery for the Amplitude Audio SDK.
//!
//! Locates the SDK installation at runtime by checking:
//! 1. `AM_SDK_PATH` environment variable (preferred)
//! 2. Common installation paths (platform-specific fallbacks)
//!
//! The SDK path is primarily needed at runtime for:
//! - Loading schemas for runtime validation of user-edited JSON files
//! - Verifying SDK version compatibility
//! - Build/export operations that need SDK tooling
//!
//! Note: `AM_SDK_PATH` is also required at **build time** for code generation
//! (see `build.rs`). The runtime discovery reuses the same mechanism but adds
//! fallback paths for cases where the env var is set only during compilation.

use std::env;
use std::path::{Path, PathBuf};

use crate::common::errors::{CliError, codes};

/// Result of SDK discovery, containing the validated SDK path.
#[derive(Debug, Clone)]
pub struct SdkLocation {
    /// Root path of the SDK installation.
    root: PathBuf,
    /// Path to the schemas directory within the SDK.
    schemas_dir: PathBuf,
}

impl SdkLocation {
    /// Returns the root path of the SDK installation.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the path to the schemas directory (`<sdk_root>/schemas/`).
    pub fn schemas_dir(&self) -> &Path {
        &self.schemas_dir
    }

    /// Creates an SdkLocation for testing without validation.
    ///
    /// Assumes the schemas directory is at `<root>/schemas/`.
    #[doc(hidden)]
    pub fn new_for_test(root: PathBuf) -> Self {
        let schemas_dir = root.join("schemas");
        Self { root, schemas_dir }
    }
}

/// Discover the Amplitude SDK installation.
///
/// Checks the following locations in order:
/// 1. `AM_SDK_PATH` environment variable
/// 2. Platform-specific common installation paths
///
/// Returns `Ok(SdkLocation)` if the SDK is found with a valid schemas directory.
/// Returns `Err` with a helpful error message if the SDK is not found.
///
/// # Errors
///
/// Returns `CliError` with code `ERR_SDK_NOT_FOUND` when:
/// - No valid SDK installation is found at any checked location
/// - The SDK path exists but doesn't contain a `schemas/` directory
pub fn discover_sdk() -> Result<SdkLocation, CliError> {
    // 1. Check AM_SDK_PATH environment variable (preferred)
    if let Ok(sdk_path) = env::var("AM_SDK_PATH") {
        let path = PathBuf::from(&sdk_path);
        if let Some(location) = validate_sdk_path(&path) {
            return Ok(location);
        }
        // Path was set but invalid — report specific error
        return Err(CliError::new(
            codes::ERR_SDK_NOT_FOUND,
            format!(
                "AM_SDK_PATH is set to '{}' but no schemas directory was found",
                sdk_path
            ),
            "The SDK path must contain a 'schemas/' directory with .bfbs schema files",
        )
        .with_suggestion("Verify your AM_SDK_PATH points to a valid Amplitude SDK installation"));
    }

    // 2. Check common installation paths
    for candidate in common_sdk_paths() {
        if let Some(location) = validate_sdk_path(&candidate) {
            return Ok(location);
        }
    }

    // SDK not found anywhere
    Err(CliError::new(
        codes::ERR_SDK_NOT_FOUND,
        "Amplitude SDK installation not found",
        "The CLI checked AM_SDK_PATH and common installation directories but found no SDK",
    )
    .with_suggestion(
        "Set the AM_SDK_PATH environment variable to your SDK installation path.\n\
         See https://github.com/AmplitudeAudio/sdk for installation instructions.",
    ))
}

/// Validate that a path is a valid SDK installation.
///
/// A valid SDK path must:
/// - Exist as a directory
/// - Contain a `schemas/` subdirectory
fn validate_sdk_path(path: &Path) -> Option<SdkLocation> {
    if !path.is_dir() {
        return None;
    }

    let schemas_dir = path.join("schemas");
    if !schemas_dir.is_dir() {
        return None;
    }

    Some(SdkLocation {
        root: path.to_path_buf(),
        schemas_dir,
    })
}

/// Returns platform-specific common SDK installation paths.
///
/// These are fallback locations checked when `AM_SDK_PATH` is not set.
fn common_sdk_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // User-local paths
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join("amplitude-sdk"));
        paths.push(home.join("AmplitudeAudio").join("sdk"));
        paths.push(home.join(".amplitude").join("sdk"));
    }

    // Platform-specific paths
    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/local/share/amplitude-sdk"));
        paths.push(PathBuf::from("/opt/amplitude-sdk"));
    }

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/usr/local/share/amplitude-sdk"));
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join("Library").join("AmplitudeAudio").join("sdk"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(program_files) = env::var("ProgramFiles") {
            paths.push(
                PathBuf::from(&program_files)
                    .join("AmplitudeAudio")
                    .join("sdk"),
            );
        }
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            paths.push(
                PathBuf::from(&local_app_data)
                    .join("AmplitudeAudio")
                    .join("sdk"),
            );
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_validate_sdk_path_valid() {
        let dir = tempdir().unwrap();
        let schemas_dir = dir.path().join("schemas");
        fs::create_dir_all(&schemas_dir).unwrap();

        let result = validate_sdk_path(dir.path());
        assert!(result.is_some());

        let location = result.unwrap();
        assert_eq!(location.root(), dir.path());
        assert_eq!(location.schemas_dir(), schemas_dir);
    }

    #[test]
    fn test_validate_sdk_path_no_schemas_dir() {
        let dir = tempdir().unwrap();
        // No schemas/ directory
        assert!(validate_sdk_path(dir.path()).is_none());
    }

    #[test]
    fn test_validate_sdk_path_nonexistent() {
        let path = PathBuf::from("/nonexistent/path/to/sdk");
        assert!(validate_sdk_path(&path).is_none());
    }

    #[test]
    fn test_discover_sdk_from_env() {
        let dir = tempdir().unwrap();
        let schemas_dir = dir.path().join("schemas");
        fs::create_dir_all(&schemas_dir).unwrap();

        // Temporarily set AM_SDK_PATH
        // SAFETY: Test runs single-threaded; no concurrent env access.
        let original = env::var("AM_SDK_PATH").ok();
        unsafe { env::set_var("AM_SDK_PATH", dir.path()) };

        let result = discover_sdk();

        // Restore original value
        match original {
            Some(val) => unsafe { env::set_var("AM_SDK_PATH", val) },
            None => unsafe { env::remove_var("AM_SDK_PATH") },
        }

        assert!(result.is_ok());
        let location = result.unwrap();
        assert_eq!(location.root(), dir.path());
    }

    #[test]
    fn test_discover_sdk_invalid_env_path() {
        // SAFETY: Test runs single-threaded; no concurrent env access.
        let original = env::var("AM_SDK_PATH").ok();
        unsafe { env::set_var("AM_SDK_PATH", "/nonexistent/sdk/path") };

        let result = discover_sdk();

        match original {
            Some(val) => unsafe { env::set_var("AM_SDK_PATH", val) },
            None => unsafe { env::remove_var("AM_SDK_PATH") },
        }

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, codes::ERR_SDK_NOT_FOUND);
        assert!(err.what.contains("AM_SDK_PATH"));
    }

    #[test]
    fn test_common_sdk_paths_not_empty() {
        let paths = common_sdk_paths();
        // Should always have at least some paths (home-based ones)
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_sdk_location_accessors() {
        let location = SdkLocation {
            root: PathBuf::from("/sdk"),
            schemas_dir: PathBuf::from("/sdk/schemas"),
        };
        assert_eq!(location.root(), Path::new("/sdk"));
        assert_eq!(location.schemas_dir(), Path::new("/sdk/schemas"));
    }
}
