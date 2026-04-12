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

//! Unit tests for switch asset commands.
//!
//! Tests command parsing and validation for switch CRUD operations.

use am::app::{App, Commands};
use am::commands::asset::{AssetCommands, SwitchCommands};
use clap::Parser;

// =============================================================================
// P0: Command Parsing Tests
// =============================================================================

#[test]
fn test_p0_switch_create_command_parses_with_name_only() {
    let args = ["am", "asset", "switch", "create", "surface_type"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::Create { name, states },
                },
        } => {
            assert_eq!(name, "surface_type");
            assert!(states.is_none());
        }
        _ => panic!("Expected Asset Switch Create command"),
    }
}

#[test]
fn test_p0_switch_create_command_parses_with_states_flag() {
    let args = [
        "am",
        "asset",
        "switch",
        "create",
        "surface_type",
        "--states",
        "wood,stone,metal",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::Create { name, states },
                },
        } => {
            assert_eq!(name, "surface_type");
            assert_eq!(
                states,
                Some(vec![
                    "wood".to_string(),
                    "stone".to_string(),
                    "metal".to_string()
                ])
            );
        }
        _ => panic!("Expected Asset Switch Create command"),
    }
}

#[test]
fn test_p0_switch_list_command_parses() {
    let args = ["am", "asset", "switch", "list"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::List {},
                },
        } => {
            // Command parsed successfully
        }
        _ => panic!("Expected Asset Switch List command"),
    }
}

#[test]
fn test_p0_switch_update_command_parses_with_name_only() {
    let args = ["am", "asset", "switch", "update", "surface_type"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::Update { name, states },
                },
        } => {
            assert_eq!(name, "surface_type");
            assert!(states.is_none());
        }
        _ => panic!("Expected Asset Switch Update command"),
    }
}

#[test]
fn test_p0_switch_update_command_parses_with_states_flag() {
    let args = [
        "am",
        "asset",
        "switch",
        "update",
        "surface_type",
        "--states",
        "wood,stone,grass",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::Update { name, states },
                },
        } => {
            assert_eq!(name, "surface_type");
            assert_eq!(
                states,
                Some(vec![
                    "wood".to_string(),
                    "stone".to_string(),
                    "grass".to_string()
                ])
            );
        }
        _ => panic!("Expected Asset Switch Update command"),
    }
}

#[test]
fn test_p0_switch_delete_command_parses() {
    let args = ["am", "asset", "switch", "delete", "surface_type"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::Delete { name, force },
                },
        } => {
            assert_eq!(name, "surface_type");
            assert!(!force);
        }
        _ => panic!("Expected Asset Switch Delete command"),
    }
}

#[test]
fn test_p0_switch_delete_command_parses_with_force() {
    let args = ["am", "asset", "switch", "delete", "surface_type", "--force"];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::Delete { name, force },
                },
        } => {
            assert_eq!(name, "surface_type");
            assert!(force);
        }
        _ => panic!("Expected Asset Switch Delete command with force"),
    }
}

// =============================================================================
// P1: Command Requirements Tests
// =============================================================================

#[test]
fn test_p1_switch_create_command_requires_name() {
    let args = ["am", "asset", "switch", "create"];
    let result = App::try_parse_from(args);
    assert!(result.is_err());
}

#[test]
fn test_p1_switch_update_command_requires_name() {
    let args = ["am", "asset", "switch", "update"];
    let result = App::try_parse_from(args);
    assert!(result.is_err());
}

#[test]
fn test_p1_switch_delete_command_requires_name() {
    let args = ["am", "asset", "switch", "delete"];
    let result = App::try_parse_from(args);
    assert!(result.is_err());
}

// =============================================================================
// P1: States Flag Parsing Tests
// =============================================================================

#[test]
fn test_p1_switch_create_single_state() {
    let args = [
        "am",
        "asset",
        "switch",
        "create",
        "surface_type",
        "--states",
        "wood",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::Create { name, states },
                },
        } => {
            assert_eq!(name, "surface_type");
            assert_eq!(states, Some(vec!["wood".to_string()]));
        }
        _ => panic!("Expected Asset Switch Create command"),
    }
}

#[test]
fn test_p1_switch_create_many_states() {
    let args = [
        "am",
        "asset",
        "switch",
        "create",
        "surface_type",
        "--states",
        "wood,stone,metal,grass,dirt,concrete",
    ];
    let app = App::try_parse_from(args).expect("Should parse");

    match app.command {
        Commands::Asset {
            command:
                AssetCommands::Switch {
                    command: SwitchCommands::Create { name, states },
                },
        } => {
            assert_eq!(name, "surface_type");
            assert_eq!(states.as_ref().unwrap().len(), 6);
        }
        _ => panic!("Expected Asset Switch Create command"),
    }
}
