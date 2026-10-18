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

//! Checks that a written JSON file is accepted by `flatc --binary`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn flatc() -> Option<PathBuf> {
    ["flatc", "/opt/homebrew/bin/flatc"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| Command::new(p).arg("--version").output().is_ok())
}

/// Runs `flatc --binary` on `file` with `buses_definition.fbs`; skips (visibly)
/// when flatc or the SDK schemas are not available.
pub fn assert_flatc_accepts_buses(file: &Path) {
    assert_flatc_accepts_schema("buses_definition.fbs", file);
}

/// Runs `flatc --binary` on `file` with the named SDK schema; skips (visibly)
/// when flatc or the SDK schemas are not available.
pub fn assert_flatc_accepts_schema(schema_name: &str, file: &Path) {
    let Some(flatc) = flatc() else {
        eprintln!("skipped flatc check: flatc is not installed");
        return;
    };
    let Ok(sdk) = std::env::var("AM_SDK_PATH") else {
        eprintln!("skipped flatc check: AM_SDK_PATH is not set");
        return;
    };
    let sdk = PathBuf::from(sdk);
    let Some(schema) = [sdk.join("schemas"), sdk.join("../schemas")]
        .into_iter()
        .map(|d| d.join(schema_name))
        .find(|p| p.exists())
    else {
        eprintln!("skipped flatc check: {schema_name} not found");
        return;
    };
    let out_dir = tempfile::tempdir().unwrap();
    let out = Command::new(flatc)
        .args(["-b", "--strict-json", "-o"])
        .arg(out_dir.path())
        .arg("-I")
        .arg(schema.parent().unwrap())
        .arg(&schema)
        .arg(file)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "flatc rejected {}: {}",
        file.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}
