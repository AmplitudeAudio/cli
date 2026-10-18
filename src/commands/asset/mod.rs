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
//! Manages Amplitude Audio SDK assets: sounds, collections, effects, switches,
//! switch containers, events, and soundbanks.

mod attenuation;
mod attenuation_args;
mod attenuation_prompts;
mod bus;
mod bus_args;
mod bus_file;
mod bus_prompts;
mod bus_shared;
mod collection;
mod effect;
mod event;
mod rtpc;
mod rtpc_args;
mod rtpc_prompts;
mod sound;
mod sound_args;
mod sound_prompts;
mod soundbank;
mod switch;
mod switch_container;

pub use attenuation::{AttenuationCommands, handler as handle_attenuation_command};
pub use bus::{BusCommands, handler as handle_bus_command};
pub use bus_file::{BusFileCommands, handler as handle_bus_file_command};
pub use collection::{CollectionCommands, handler as handle_collection_command};
pub use effect::{EffectCommands, handler as handle_effect_command};
pub use event::{EventCommands, handler as handle_event_command};
pub use rtpc::{RtpcCommands, handler as handle_rtpc_command};
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

/// Recursively find all `.json` files under `dir`. Returns empty vec if `dir` doesn't exist.
pub(crate) fn find_json_files_recursive(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "json").unwrap_or(false) {
            files.push(path.to_path_buf());
        }
    }
    Ok(files)
}

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
    /// Attenuation asset management
    Attenuation {
        #[command(subcommand)]
        command: AttenuationCommands,
    },
    /// RTPC (real-time parameter control) asset management
    Rtpc {
        #[command(subcommand)]
        command: RtpcCommands,
    },
    /// Bus asset management (buses inside '*.buses.json' files)
    Bus {
        #[command(subcommand)]
        command: BusCommands,
    },
    /// Bus file management ('<name>.buses.json' files in the project root)
    BusFile {
        #[command(subcommand)]
        command: BusFileCommands,
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
        AssetCommands::Attenuation { command } => {
            handle_attenuation_command(command, database, input, output).await
        }
        AssetCommands::Rtpc { command } => {
            handle_rtpc_command(command, database, input, output).await
        }
        AssetCommands::Bus { command } => {
            handle_bus_command(command, database, input, output).await
        }
        AssetCommands::BusFile { command } => {
            handle_bus_file_command(command, database, input, output).await
        }
    }
}
