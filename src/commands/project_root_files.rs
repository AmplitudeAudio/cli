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

//! Validation of the bus (`*.buses.json`) and engine config (`*.config.json`)
//! files for `am project validate` and `am project build`.
//!
//! They are named files, not assets under `sources/`, so they are looked up
//! next to the project file. The `sources/` directory is scanned too: the
//! default project layout created by `am project init` keeps them there.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::assets::{
    ValidationError,
    bus::{scan_bus_files, validate_bus_file},
    engine_config::{scan_engine_configs, validate_engine_config},
};

/// Which kind of root-level file a finding is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RootFileKind {
    Buses,
    EngineConfig,
}

impl RootFileKind {
    /// The label printed as the finding's type.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Buses => "buses",
            Self::EngineConfig => "engine-config",
        }
    }
}

/// One problem found in a root-level file.
#[derive(Debug)]
pub(super) struct RootFileFinding {
    pub(super) file: String,
    pub(super) kind: RootFileKind,
    pub(super) error: String,
    pub(super) why: String,
    pub(super) suggestion: String,
    pub(super) field: Option<String>,
}

/// Findings plus the number of files that were checked.
#[derive(Debug, Default)]
pub(super) struct RootFileReport {
    pub(super) findings: Vec<RootFileFinding>,
    pub(super) bus_files: usize,
    pub(super) engine_configs: usize,
}

/// The directories that can hold bus and engine config files, without duplicates.
fn search_dirs(project_root: &Path, sources_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![project_root.to_path_buf()];
    if sources_dir != project_root && sources_dir.is_dir() {
        dirs.push(sources_dir.to_path_buf());
    }
    dirs
}

fn relative(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn finding(
    project_root: &Path,
    path: &Path,
    kind: RootFileKind,
    error: &ValidationError,
) -> RootFileFinding {
    RootFileFinding {
        file: relative(project_root, path),
        kind,
        error: error.what().to_string(),
        why: error.why().to_string(),
        suggestion: error.suggestion().to_string(),
        field: error.field.clone(),
    }
}

fn unreadable(
    project_root: &Path,
    dir: &Path,
    kind: RootFileKind,
    problem: &str,
) -> RootFileFinding {
    RootFileFinding {
        file: relative(project_root, dir),
        kind,
        error: problem.to_string(),
        why: "The file could not be read or is not valid JSON".to_string(),
        suggestion: "Fix the JSON syntax or restore the file".to_string(),
        field: None,
    }
}

/// Validates the bus and/or engine config files of the project.
pub(super) fn validate_root_files(
    project_root: &Path,
    sources_dir: &Path,
    buses: bool,
    engine_configs: bool,
) -> RootFileReport {
    let mut report = RootFileReport::default();
    for dir in search_dirs(project_root, sources_dir) {
        if buses {
            check_dir(
                &mut report,
                project_root,
                &dir,
                RootFileKind::Buses,
                validate_bus_file,
            );
        }
        if engine_configs {
            check_dir(
                &mut report,
                project_root,
                &dir,
                RootFileKind::EngineConfig,
                validate_engine_config,
            );
        }
    }
    report
}

fn check_dir(
    report: &mut RootFileReport,
    project_root: &Path,
    dir: &Path,
    kind: RootFileKind,
    rules: fn(&Value) -> Result<(), ValidationError>,
) {
    let (files, problems): (Vec<(PathBuf, Value)>, Vec<String>) = match kind {
        RootFileKind::Buses => match scan_bus_files(dir) {
            Ok(scan) => (
                scan.files.into_iter().map(|f| (f.path, f.value)).collect(),
                // Id collisions are advice for `am asset bus`, not file errors.
                scan.problems
                    .into_iter()
                    .filter(|p| p.starts_with("Invalid"))
                    .collect(),
            ),
            Err(e) => (Vec::new(), vec![e.to_string()]),
        },
        RootFileKind::EngineConfig => match scan_engine_configs(dir) {
            Ok(scan) => (
                scan.files.into_iter().map(|f| (f.path, f.value)).collect(),
                scan.problems,
            ),
            Err(e) => (Vec::new(), vec![e.to_string()]),
        },
    };

    for (path, value) in files {
        match kind {
            RootFileKind::Buses => report.bus_files += 1,
            RootFileKind::EngineConfig => report.engine_configs += 1,
        }
        if let Err(error) = rules(&value) {
            report
                .findings
                .push(finding(project_root, &path, kind, &error));
        }
    }
    for problem in problems {
        report
            .findings
            .push(unreadable(project_root, dir, kind, &problem));
    }
}
