//! Asset management commands.
//!
//! This module provides commands for managing Amplitude Audio SDK assets.
//! Currently supports Sound assets, with more asset types planned.

mod sound;

pub use sound::{SoundCommands, handler as handle_sound_command};

use anyhow::Result;
use clap::Subcommand;
use std::sync::Arc;

use crate::{
    database::Database,
    input::Input,
    presentation::Output,
};

/// Asset management commands.
///
/// These commands manage audio assets within an Amplitude project.
#[derive(Subcommand, Debug)]
pub enum AssetCommands {
    /// Sound asset management
    Sound {
        #[command(subcommand)]
        command: SoundCommands,
    },
}

/// Handle asset commands by routing to the appropriate subcommand handler.
pub async fn handler(
    command: &AssetCommands,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        AssetCommands::Sound { command } => {
            handle_sound_command(command, database, input, output).await
        }
    }
}
