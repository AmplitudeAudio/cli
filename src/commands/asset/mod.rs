//! Asset management commands.
//!
//! This module provides commands for managing Amplitude Audio SDK assets.
//! Currently supports Sound assets, with more asset types planned.

mod collection;
mod effect;
mod sound;
mod switch;

pub use collection::{CollectionCommands, handler as handle_collection_command};
pub use effect::{EffectCommands, handler as handle_effect_command};
pub use sound::{SoundCommands, handler as handle_sound_command};
pub use switch::{SwitchCommands, handler as handle_switch_command};

use anyhow::Result;
use clap::Subcommand;
use std::sync::Arc;

use crate::{
    assets::Spatialization,
    common::errors::{CliError, codes},
    database::Database,
    input::Input,
    presentation::Output,
};

/// Parse spatialization mode from string.
///
/// Shared utility used by multiple asset command modules (sound, collection, etc.).
pub fn parse_spatialization(s: &str) -> Result<Spatialization> {
    match s.to_lowercase().as_str() {
        "none" => Ok(Spatialization::None),
        "position" => Ok(Spatialization::Position),
        "position_orientation" | "positionorientation" => Ok(Spatialization::PositionOrientation),
        "hrtf" => Ok(Spatialization::HRTF),
        _ => Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid spatialization mode: '{}'", s),
            "Spatialization must be one of: none, position, position_orientation, hrtf",
        )
        .into()),
    }
}

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
    /// Collection asset management
    Collection {
        #[command(subcommand)]
        command: CollectionCommands,
    },
    /// Effect asset management
    Effect {
        #[command(subcommand)]
        command: EffectCommands,
    },
    /// Switch asset management
    Switch {
        #[command(subcommand)]
        command: SwitchCommands,
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
        AssetCommands::Collection { command } => {
            handle_collection_command(command, database, input, output).await
        }
        AssetCommands::Effect { command } => {
            handle_effect_command(command, database, input, output).await
        }
        AssetCommands::Switch { command } => {
            handle_switch_command(command, database, input, output).await
        }
    }
}
