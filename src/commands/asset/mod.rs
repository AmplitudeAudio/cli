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

//! Asset management commands.
//!
//! This module provides commands for managing Amplitude Audio SDK assets.
//! Currently supports Sound assets, with more asset types planned.

mod collection;
mod effect;
mod event;
mod sound;
mod soundbank;
mod switch;
mod switch_container;

pub use collection::{CollectionCommands, handler as handle_collection_command};
pub use effect::{EffectCommands, handler as handle_effect_command};
pub use event::{EventCommands, handler as handle_event_command};
pub use sound::{SoundCommands, handler as handle_sound_command};
pub use soundbank::{SoundbankCommands, handler as handle_soundbank_command};
pub use switch::{SwitchCommands, handler as handle_switch_command};
pub use switch_container::{SwitchContainerCommands, handler as handle_switch_container_command};

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
    /// Switch container asset management
    SwitchContainer {
        #[command(subcommand)]
        command: SwitchContainerCommands,
    },
    /// Event asset management
    Event {
        #[command(subcommand)]
        command: EventCommands,
    },
    /// Soundbank asset management
    Soundbank {
        #[command(subcommand)]
        command: SoundbankCommands,
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
        AssetCommands::SwitchContainer { command } => {
            handle_switch_container_command(command, database, input, output).await
        }
        AssetCommands::Event { command } => {
            handle_event_command(command, database, input, output).await
        }
        AssetCommands::Soundbank { command } => {
            handle_soundbank_command(command, database, input, output).await
        }
    }
}
