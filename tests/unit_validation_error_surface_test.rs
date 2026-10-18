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

//! A `ValidationError` returned through `anyhow` must keep its code at process level
//! (exit class, JSON error envelope), exactly like a `CliError`.

mod common;

use am::assets::AssetType;
use am::common::errors::codes::{ERR_VALIDATION_EMPTY_REFERENCE, ERR_VALIDATION_REFERENCE};
use am::common::errors::{as_cli_error, determine_exit_code, exit_codes};
use am::forms::references::rule_for;
use am::forms::validate_refs::validate_reference;
use am::presentation::JsonOutput;
use common::fixtures::AssetTestFixture;
use std::process::Command;

fn bus_error(id: u64) -> anyhow::Error {
    let fixture = AssetTestFixture::new("surface").unwrap();
    let validator = fixture.create_project_validator().unwrap();
    let rule = rule_for(AssetType::Sound, "bus").unwrap();
    validate_reference(&rule, id, &validator)
        .unwrap_err()
        .into()
}

#[test]
fn test_validation_error_exit_code_is_user_error() {
    assert_eq!(determine_exit_code(&bus_error(0)), exit_codes::USER_ERROR);
    assert_eq!(determine_exit_code(&bus_error(999)), exit_codes::USER_ERROR);
}

#[test]
fn test_as_cli_error_unwraps_validation_error() {
    assert_eq!(
        as_cli_error(&bus_error(0)).map(|e| e.code),
        Some(ERR_VALIDATION_EMPTY_REFERENCE)
    );
    assert_eq!(
        as_cli_error(&bus_error(999)).map(|e| e.code),
        Some(ERR_VALIDATION_REFERENCE)
    );
}

#[test]
fn test_json_error_envelope_carries_validation_code() {
    let zero = JsonOutput::build_error_response(&bus_error(0), -1);
    let zero = zero.error.expect("error body");
    assert_eq!(zero.code, ERR_VALIDATION_EMPTY_REFERENCE);
    assert!(!zero.suggestion.is_empty());

    let dangling = JsonOutput::build_error_response(&bus_error(999), -1);
    assert_eq!(dangling.error.unwrap().code, ERR_VALIDATION_REFERENCE);
}

#[test]
fn test_binary_reports_validation_code_and_user_exit_code() {
    let fixture = AssetTestFixture::new("surface_bin").unwrap();
    fixture.create_data_file("a.wav").unwrap();
    let home = tempfile::tempdir().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .current_dir(fixture.project_root())
        .env("HOME", home.path())
        .args([
            "--json", "asset", "sound", "create", "boom", "--file", "a.wav", "--bus", "999",
        ])
        .output()
        .expect("run am");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "stdout: {stdout}");
    assert!(
        stdout.contains(&ERR_VALIDATION_REFERENCE.to_string()),
        "stdout: {stdout}"
    );
}
