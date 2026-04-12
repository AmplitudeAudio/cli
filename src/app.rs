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

use clap::{Parser, Subcommand};
use clap_complete::Shell;
use rust_embed::RustEmbed;

use crate::commands::{
    asset::AssetCommands, project::ProjectCommands, sudo::SudoCommands, template::TemplateCommands,
};

#[derive(RustEmbed)]
#[folder = "resources/"]
pub struct Resource;

#[derive(Parser)]
#[command(name = "am", version, about, long_about = None)]
#[command(after_help = "Example:
  am project init my_awesome_project --template=o3de

For more information, visit https://docs.amplitudeaudiosdk.com
")]
#[command(help_template = "\n
{about} ({name})
Copyright (c) 2025-present Sparky Studios. All rights reserved.
---------------------------------------------------------------

{usage-heading} {usage}

{all-args}
{after-help}
")]
pub struct App {
    /// Enable verbose logging (debug and trace messages)
    #[arg(short, long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Suppress informational output (errors are always shown)
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Output in JSON format for machine parsing
    #[arg(long, global = true)]
    pub json: bool,

    /// Disable interactive prompts (fail if input required)
    #[arg(long, global = true)]
    pub non_interactive: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage audio assets (sounds, collections, etc.)
    Asset {
        #[command(subcommand)]
        command: AssetCommands,
    },

    /// Amplitude project-related tasks
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },

    /// Administrative and destructive operations
    Sudo {
        #[command(subcommand)]
        command: SudoCommands,
    },

    /// SDK-related tasks
    Sdk {
        #[command(subcommand)]
        command: crate::commands::sdk::SdkCommands,
    },

    /// Manage project templates
    Template {
        #[command(subcommand)]
        command: TemplateCommands,
    },

    /// Generate shell completion scripts
    ///
    /// Outputs a completion script for the specified shell.
    /// Pipe to a file or source directly for tab-completion support.
    ///
    /// Installation:
    ///   bash:  am completions bash > ~/.local/share/bash-completion/completions/am
    ///   zsh:   am completions zsh > ~/.zfunc/_am
    ///   fish:  am completions fish > ~/.config/fish/completions/am.fish
    ///
    /// Note: Asset names and project names require manual typing;
    /// completions cover commands, subcommands, and flags.
    Completions {
        /// Shell to generate completions for (bash, zsh, fish)
        shell: Shell,
    },
}
