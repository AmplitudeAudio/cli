use clap::{Parser, Subcommand};

use crate::commands::project::ProjectCommands;

#[derive(Parser)]
#[command(name = "ampm", version, about, long_about = None)]
#[command(after_help = "Example:
  ampm project init my_awesome_project --template=o3de

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

#[derive(Subcommand)]
pub enum Commands {
    /// Amplitude project-related tasks
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },
}
