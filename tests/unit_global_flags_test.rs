//! Unit tests for global CLI flags (--json, --non-interactive)
//!
//! Tests the CLI flag parsing and interaction with OutputMode and InputMode selection.

use clap::Parser;

/// Minimal App struct for testing flag parsing
/// Mirrors the structure in src/app.rs
#[derive(Parser, Debug)]
#[command(name = "am")]
struct TestApp {
    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(long, global = true)]
    json: bool,

    #[arg(long, global = true)]
    non_interactive: bool,

    #[command(subcommand)]
    command: Option<TestCommands>,
}

#[derive(clap::Subcommand, Debug)]
enum TestCommands {
    /// Test subcommand
    Test,
    /// Project subcommand for testing nested commands
    Project {
        #[command(subcommand)]
        command: TestProjectCommands,
    },
}

#[derive(clap::Subcommand, Debug)]
enum TestProjectCommands {
    /// List projects
    List,
}

// =============================================================================
// P0 Tests (Critical)
// =============================================================================

#[test]
fn test_json_flag_parsed_correctly() {
    let app = TestApp::try_parse_from(["am", "--json", "test"]).unwrap();
    assert!(app.json, "Expected --json flag to be true");
    assert!(
        !app.non_interactive,
        "Expected --non-interactive to be false"
    );
}

#[test]
fn test_no_json_flag_defaults_to_false() {
    let app = TestApp::try_parse_from(["am", "test"]).unwrap();
    assert!(!app.json, "Expected --json flag to default to false");
}

#[test]
fn test_json_flag_is_global_applies_to_subcommands() {
    // Test flag after "am" but before subcommand
    let app = TestApp::try_parse_from(["am", "--json", "project", "list"]).unwrap();
    assert!(
        app.json,
        "Expected --json flag to be true for nested subcommand"
    );

    // Test flag at the end (global flags can appear anywhere with clap)
    let app2 = TestApp::try_parse_from(["am", "project", "--json", "list"]).unwrap();
    assert!(
        app2.json,
        "Expected --json flag to work at subcommand level"
    );
}

// =============================================================================
// P1 Tests (Important)
// =============================================================================

#[test]
fn test_non_interactive_flag_parsed_correctly() {
    let app = TestApp::try_parse_from(["am", "--non-interactive", "test"]).unwrap();
    assert!(
        app.non_interactive,
        "Expected --non-interactive flag to be true"
    );
    assert!(!app.json, "Expected --json to be false");
}

#[test]
fn test_json_and_non_interactive_flags_together() {
    let app = TestApp::try_parse_from(["am", "--json", "--non-interactive", "test"]).unwrap();
    assert!(app.json, "Expected --json flag to be true");
    assert!(
        app.non_interactive,
        "Expected --non-interactive flag to be true"
    );
}

#[test]
fn test_verbose_flag_still_works_with_json() {
    let app = TestApp::try_parse_from(["am", "--verbose", "--json", "test"]).unwrap();
    assert!(app.verbose, "Expected --verbose flag to be true");
    assert!(app.json, "Expected --json flag to be true");
}

// =============================================================================
// P2 Tests (Nice to have)
// =============================================================================

#[test]
fn test_flags_order_independence() {
    // Test various orderings
    let orderings = [
        ["am", "--json", "--non-interactive", "--verbose", "test"],
        ["am", "--verbose", "--json", "--non-interactive", "test"],
        ["am", "--non-interactive", "--verbose", "--json", "test"],
    ];

    for args in orderings {
        let app = TestApp::try_parse_from(args).unwrap();
        assert!(app.json, "Expected --json to be true");
        assert!(app.non_interactive, "Expected --non-interactive to be true");
        assert!(app.verbose, "Expected --verbose to be true");
    }
}

#[test]
fn test_flags_work_with_nested_subcommands() {
    let app =
        TestApp::try_parse_from(["am", "--json", "--non-interactive", "project", "list"]).unwrap();
    assert!(app.json, "Expected --json flag for nested command");
    assert!(
        app.non_interactive,
        "Expected --non-interactive flag for nested command"
    );
}

// =============================================================================
// OutputMode Selection Tests
// =============================================================================

/// OutputMode enum mirroring src/presentation/mod.rs for testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Interactive,
    Json,
}

/// Determines output mode based on flags (mirrors logic from main.rs)
fn determine_output_mode(json_flag: bool) -> OutputMode {
    if json_flag {
        OutputMode::Json
    } else {
        OutputMode::Interactive
    }
}

#[test]
fn test_json_flag_selects_json_output_mode() {
    assert_eq!(
        determine_output_mode(true),
        OutputMode::Json,
        "Expected JsonOutput mode when --json flag is true"
    );
}

#[test]
fn test_no_json_flag_selects_interactive_output_mode() {
    assert_eq!(
        determine_output_mode(false),
        OutputMode::Interactive,
        "Expected InteractiveOutput mode when --json flag is false"
    );
}

// =============================================================================
// Non-Interactive Mode Tests (Unit tests for prompt blocking behavior)
// =============================================================================

#[cfg(test)]
mod non_interactive_tests {
    use am::input::{create_input, Input, InputMode};

    #[test]
    fn test_create_input_interactive_by_default() {
        // Default mode (no flags): InteractiveInput
        let input = create_input(InputMode::Interactive);
        let _: &dyn Input = input.as_ref();
    }

    #[test]
    fn test_create_input_non_interactive_when_flag_set() {
        // --non-interactive: NonInteractiveInput
        let input = create_input(InputMode::NonInteractive);
        let result = input.prompt_text("Test prompt", None, None, None);
        assert!(
            result.is_err(),
            "Expected prompt to fail in non-interactive mode"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("non-interactive mode"),
            "Error should mention non-interactive mode: {}",
            err_msg
        );
        assert!(
            err_msg.contains("command-line arguments"),
            "Error should suggest using command-line arguments: {}",
            err_msg
        );
    }

    #[test]
    fn test_json_implies_non_interactive_input_mode() {
        // This mirrors main.rs behavior:
        // --json implies NonInteractiveInput even if --non-interactive is not set.
        let json_flag = true;
        let non_interactive_flag = false;

        let mode = if json_flag || non_interactive_flag {
            InputMode::NonInteractive
        } else {
            InputMode::Interactive
        };

        let input = create_input(mode);
        let result = input.prompt_text("Test prompt", None, None, None);
        assert!(
            result.is_err(),
            "Expected prompt to fail when --json implies non-interactive input"
        );
    }
}

// =============================================================================
// Tests using TestApp mirror of actual App struct
// =============================================================================

// Tests for command groups are covered above in test_flags_work_with_nested_subcommands
// which tests both project-like and nested command structures.

// =============================================================================
// Output Factory Tests
// =============================================================================

#[cfg(test)]
mod output_factory_tests {
    use am::presentation::{create_output, Output, OutputMode};

    #[test]
    fn test_create_output_interactive_mode() {
        let output = create_output(OutputMode::Interactive);
        // Verify we got an output implementation
        let _: &dyn Output = output.as_ref();
    }

    #[test]
    fn test_create_output_json_mode() {
        let output = create_output(OutputMode::Json);
        // Verify we got an output implementation
        let _: &dyn Output = output.as_ref();
    }
}
