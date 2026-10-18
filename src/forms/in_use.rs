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

//! The "is this asset still referenced?" check every `delete` runs first.
//!
//! Deleting an asset that others point at would leave them with a dangling id (or a
//! soundbank listing a file that no longer builds). The check gathers both kinds of
//! referrer: assets that hold the id (see [`referrers_with_type`]) and soundbanks that
//! list the asset by name (see [`soundbank_referrers`]). `--force` never overrides it.

use crate::assets::AssetType;
use crate::assets::ProjectValidator;
use crate::common::errors::{CliError, codes};
use crate::forms::references::{RefTarget, referrers_with_type};
use crate::forms::soundbank_refs::soundbank_referrers;

/// Fails with `ERR_ASSET_IN_USE` (naming every referrer) when some asset or soundbank
/// still references the `asset_type` asset `name` whose id is `id`.
pub fn assert_not_in_use(
    validator: &ProjectValidator,
    asset_type: AssetType,
    id: u64,
    name: &str,
) -> Result<(), CliError> {
    let mut users = referrers_with_type(validator, RefTarget::Asset(asset_type), id);
    users.extend(
        soundbank_referrers(validator, asset_type, name)
            .into_iter()
            .map(|bank| (AssetType::Soundbank, bank)),
    );
    if users.is_empty() {
        return Ok(());
    }

    let names = users
        .iter()
        .map(|(kind, asset)| format!("{kind} '{}'", asset.name))
        .collect::<Vec<_>>()
        .join(", ");
    let fixes = users
        .iter()
        .map(|(kind, asset)| fix_hint(*kind, &asset.name, asset_type, name))
        .collect::<Vec<_>>()
        .join("; ");
    Err(CliError::new(
        codes::ERR_ASSET_IN_USE,
        format!("{asset_type} '{name}' is still referenced"),
        format!("It is used by: {names}"),
    )
    .with_suggestion(format!(
        "Remove the reference first (e.g. {fixes}), then delete the {}",
        asset_type.to_string().to_lowercase()
    )))
}

/// The `am asset` subcommand that manages `asset_type`.
fn command_name(asset_type: AssetType) -> &'static str {
    match asset_type {
        AssetType::Sound => "sound",
        AssetType::Collection => "collection",
        AssetType::Switch => "switch",
        AssetType::SwitchContainer => "switch-container",
        AssetType::Soundbank => "soundbank",
        AssetType::Event => "event",
        AssetType::Effect => "effect",
        AssetType::Attenuation => "attenuation",
        AssetType::Rtpc => "rtpc",
        AssetType::Pipeline => "pipeline",
    }
}

/// How to drop the reference `referrer` (of `referrer_type`) holds on `target`.
fn fix_hint(
    referrer_type: AssetType,
    referrer: &str,
    target_type: AssetType,
    target: &str,
) -> String {
    let cmd = command_name(referrer_type);
    match (referrer_type, target_type) {
        (AssetType::Soundbank, _) => {
            let flag = match target_type {
                AssetType::Attenuation => "attenuator",
                other => command_name(other),
            };
            format!("am asset soundbank update {referrer} --remove-{flag} {target}")
        }
        (AssetType::Event, _) => {
            format!("am asset event update {referrer} --remove-action <index>")
        }
        (_, AssetType::Attenuation) => {
            format!("am asset {cmd} update {referrer} --attenuation none")
        }
        (_, AssetType::Effect) => format!("am asset {cmd} update {referrer} --effect none"),
        (_, AssetType::Switch) => {
            format!("am asset {cmd} update {referrer} --switch-group <other switch>")
        }
        (_, AssetType::Rtpc) => {
            format!("am asset {cmd} update {referrer} with a static value instead of the RTPC")
        }
        (AssetType::Collection | AssetType::SwitchContainer, _) => {
            format!("am asset {cmd} update {referrer} --remove-entry {target}")
        }
        _ => format!("am asset {cmd} update {referrer} to stop using {target}"),
    }
}
