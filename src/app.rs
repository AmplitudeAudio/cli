use clap::{Parser, Subcommand};
use rust_embed::RustEmbed;

use crate::commands::{project::ProjectCommands, sudo::SudoCommands};

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
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
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
}
