//! Feature tests for CLI exit code behavior.
//!
//! Tests cover:
//! - Exit code 0 for successful commands
//! - Exit code 1 for user errors (validation, not found, etc.)
//! - Exit code 2 for system errors (SDK not found, panics)
//! - JSON mode exit code consistency
//!
//! Priority levels:
//! - P0: Core exit code contract (success=0, user error=1, system error=2)
//! - P1: JSON mode consistency, specific error scenarios
//! - P2: Edge cases, panic handling

use std::process::Command;

// =============================================================================
// P0: Core Exit Code Contract Tests
// =============================================================================

#[test]
fn test_p0_exit_code_success_version() {
    // GIVEN: The CLI binary
    // WHEN: Running with --version flag
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .arg("--version")
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 0
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit code 0 for --version, got {:?}",
        output.status.code()
    );
}

#[test]
fn test_p0_exit_code_success_help() {
    // GIVEN: The CLI binary
    // WHEN: Running with --help flag
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .arg("--help")
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 0
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit code 0 for --help, got {:?}",
        output.status.code()
    );
}

#[test]
fn test_p0_exit_code_user_error_project_register_invalid_path() {
    // GIVEN: The CLI binary
    // WHEN: Running project register with non-existent path
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args([
            "--non-interactive",
            "project",
            "register",
            "/nonexistent_path_xyz_12345_test",
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error - project not initialized)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for project not initialized, got {:?}. stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_p0_exit_code_user_error_invalid_subcommand() {
    // GIVEN: The CLI binary
    // WHEN: Running with an invalid subcommand
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args(["invalid_subcommand_xyz"])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be non-zero (clap returns 2 for usage errors)
    assert_ne!(
        output.status.code(),
        Some(0),
        "Expected non-zero exit code for invalid subcommand"
    );
}

// =============================================================================
// P1: JSON Mode Exit Code Tests
// =============================================================================

#[test]
fn test_p1_exit_code_json_mode_success_help() {
    // GIVEN: The CLI binary with --json flag
    // WHEN: Running with --help
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args(["--json", "--help"])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 0
    // Note: --help may not output JSON, but should still succeed
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit code 0 for --json --help, got {:?}",
        output.status.code()
    );
}

#[test]
fn test_p1_exit_code_json_mode_user_error() {
    // GIVEN: The CLI binary with --json flag
    // WHEN: Running project register with non-existent path
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args([
            "--json",
            "project",
            "register",
            "/nonexistent_path_xyz_67890_test",
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for JSON mode user error, got {:?}",
        output.status.code()
    );

    // AND: stdout should contain JSON error response
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\"") && stdout.contains("false"),
        "Expected JSON error response in stdout, got: {}",
        stdout
    );
}

#[test]
fn test_p1_exit_code_json_mode_matches_interactive() {
    // GIVEN: The CLI binary
    // WHEN: Running same failing command in both modes
    let interactive = Command::new(env!("CARGO_BIN_EXE_am"))
        .args([
            "--non-interactive",
            "project",
            "register",
            "/nonexistent_abc_99999_test",
        ])
        .output()
        .expect("Failed to execute interactive command");

    let json_mode = Command::new(env!("CARGO_BIN_EXE_am"))
        .args([
            "--json",
            "project",
            "register",
            "/nonexistent_abc_99999_test",
        ])
        .output()
        .expect("Failed to execute json command");

    // THEN: Exit codes should be identical
    assert_eq!(
        interactive.status.code(),
        json_mode.status.code(),
        "Exit codes should match between interactive ({:?}) and JSON mode ({:?})",
        interactive.status.code(),
        json_mode.status.code()
    );
}

#[test]
fn test_p1_exit_code_json_mode_error_on_stdout() {
    // GIVEN: The CLI binary with --json flag
    // WHEN: Running a command that produces an error
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args([
            "--json",
            "project",
            "register",
            "/nonexistent_stdout_test_path",
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: JSON error response should be written to stdout (not stderr)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":") || stdout.contains("\"ok\": "),
        "Expected JSON envelope in stdout. stdout: {}, stderr: {}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );

    // AND: stderr should be empty in JSON mode (AC4)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "Expected empty stderr in JSON mode, got: {}",
        stderr
    );
}

// =============================================================================
// P2: System Error (Exit Code 2) Tests
// =============================================================================

#[test]
#[ignore] // Enable when SDK validation command is implemented
fn test_p2_exit_code_system_error_sdk_not_found() {
    // GIVEN: AM_SDK_PATH is not set or invalid
    // WHEN: Running a command that requires SDK (e.g., 'am sdk check')
    // THEN: Exit code should be 2 (system error)
    //
    // TODO: Implement when a command that validates SDK presence is added.
    // The determine_exit_code() function maps -28xxx errors to exit code 2,
    // which is tested in unit tests (src/common/errors.rs:437-446).
    panic!("Test not yet implemented - awaiting SDK validation command");
}

#[test]
#[ignore] // Panic testing requires special setup
fn test_p2_exit_code_system_error_panic() {
    // GIVEN: A scenario that causes an internal panic
    // WHEN: The CLI panics during execution
    // THEN: Exit code should be 2 (system error)
    //
    // The panic handling via catch_unwind is implemented in main.rs:22-78.
    // This test would require a way to trigger a panic through CLI input,
    // which is not currently possible without a debug/test command.
    panic!("Test not yet implemented - no panic-triggering CLI input available");
}

// =============================================================================
// P2: Edge Cases
// =============================================================================

#[test]
fn test_p2_exit_code_empty_args() {
    // GIVEN: The CLI binary
    // WHEN: Running with no arguments
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be non-zero (requires subcommand)
    // clap typically returns exit code 2 for missing required arguments
    assert_ne!(
        output.status.code(),
        Some(0),
        "Expected non-zero exit code for missing subcommand"
    );
}

#[test]
fn test_p2_exit_code_verbose_flag_success() {
    // GIVEN: The CLI binary with --verbose flag
    // WHEN: Running with --verbose and --help
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args(["--verbose", "--help"])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should still be 0
    assert_eq!(
        output.status.code(),
        Some(0),
        "Expected exit code 0 for --verbose --help, got {:?}",
        output.status.code()
    );
}

#[test]
fn test_p2_exit_code_non_interactive_flag_error() {
    // GIVEN: The CLI binary with --non-interactive flag
    // WHEN: Running project register with non-existent path
    let output = Command::new(env!("CARGO_BIN_EXE_am"))
        .args([
            "--non-interactive",
            "project",
            "register",
            "/nonexistent_ni_test_path",
        ])
        .output()
        .expect("Failed to execute command");

    // THEN: Exit code should be 1 (user error)
    assert_eq!(
        output.status.code(),
        Some(1),
        "Expected exit code 1 for non-interactive mode user error, got {:?}",
        output.status.code()
    );
}
