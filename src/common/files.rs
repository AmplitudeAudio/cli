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

//! File operation utilities.
//!
//! Provides safe file operations including atomic writes to prevent
//! data corruption from interrupted operations.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Atomically write content to a file.
///
/// Writes to a temporary file first, then renames. This prevents
/// partial writes if the process crashes or is interrupted.
///
/// On POSIX systems, `rename()` is atomic for files on the same filesystem,
/// guaranteeing that either the old file or the new file exists, never a partial state.
///
/// # Arguments
///
/// * `path` - The destination file path
/// * `content` - The content to write
///
/// # Errors
///
/// Returns an error if:
/// - The parent directory cannot be created
/// - The temporary file cannot be written
/// - The rename operation fails
///
/// # Example
///
/// ```ignore
/// use std::path::Path;
/// use am::common::files::atomic_write;
///
/// atomic_write(Path::new("config.json"), b"{\"key\": \"value\"}")?;
/// ```
pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let tmp_path = path.with_extension("tmp");

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Write to temp file
    fs::write(&tmp_path, content)
        .with_context(|| format!("Failed to write temp file: {}", tmp_path.display()))?;

    // Atomic rename (POSIX guarantees atomicity for same-filesystem renames)
    fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_atomic_write_creates_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");

        atomic_write(&file_path, b"test content").unwrap();

        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_atomic_write_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("nested").join("dirs").join("test.json");

        atomic_write(&file_path, b"nested content").unwrap();

        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "nested content");
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");

        // Write initial content
        fs::write(&file_path, "old content").unwrap();

        // Overwrite with atomic write
        atomic_write(&file_path, b"new content").unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content");
    }

    #[test]
    fn test_atomic_write_no_tmp_file_remains() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.json");
        let tmp_path = file_path.with_extension("tmp");

        atomic_write(&file_path, b"content").unwrap();

        // Temp file should not exist after successful write
        assert!(!tmp_path.exists());
    }
}
