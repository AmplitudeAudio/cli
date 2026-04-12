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

use crate::config::sdk::discover_sdk;
use crate::database::Database;
use crate::input::Input;
use crate::presentation::Output;
use clap::Subcommand;
use serde_json::json;
use std::sync::Arc;

#[derive(Subcommand, Debug)]
pub enum SdkCommands {
    /// Check if the Amplitude SDK is properly configured
    Check,
}

pub async fn handler(
    command: &SdkCommands,
    _database: Option<Arc<Database>>,
    _input: &dyn Input,
    output: &dyn Output,
) -> anyhow::Result<()> {
    match command {
        SdkCommands::Check => {
            let location = discover_sdk()?;

            output.success(
                json!({
                    "message": "SDK is properly configured",
                    "path": location.root().to_string_lossy(),
                    "schemas_dir": location.schemas_dir().to_string_lossy(),
                }),
                None,
            );
            Ok(())
        }
    }
}
