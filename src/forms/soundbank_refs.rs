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

//! Which soundbanks include an asset.
//!
//! Nothing references a soundbank by id, but a soundbank names the assets it loads by
//! runtime file name (see [`SoundbankList`]), so `forms::references::referrers` (which
//! follows ids) cannot answer "which soundbanks contain this sound?". This is the
//! name-based counterpart: deleting or renaming a sound, collection, event, switch,
//! switch container, effect, attenuator or RTPC has to account for the banks that load it.

use crate::assets::validator::AssetRef;
use crate::assets::{AssetType, ProjectValidator, SoundbankList};
use crate::services::soundbank::Catalog;

/// Lists the soundbanks whose lists include the source asset named `name` of `asset_type`.
///
/// Every source file of that type and name counts, under the runtime name its list gives
/// it (`fx/boom.amsound`, and `.amfx`, and the legacy `.amenv`, for effects). Types a soundbank
/// cannot hold (soundbanks, pipelines) and unknown names have no referrers.
pub fn soundbank_referrers(
    validator: &ProjectValidator,
    asset_type: AssetType,
    name: &str,
) -> Vec<AssetRef> {
    let Some(list) = SoundbankList::for_asset_type(asset_type) else {
        return Vec::new();
    };
    let runtime_names: Vec<String> = validator
        .source_assets(asset_type)
        .iter()
        .filter(|asset| asset.name == name)
        .flat_map(|asset| Catalog::runtime_names(list, asset))
        .collect();
    if runtime_names.is_empty() {
        return Vec::new();
    }

    let dir = validator
        .sources_dir()
        .join(AssetType::Soundbank.directory_name());
    let mut found = Vec::new();
    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(value) = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        else {
            continue;
        };
        let includes = value
            .get(list.field())
            .and_then(|v| v.as_array())
            .is_some_and(|entries| {
                entries
                    .iter()
                    .filter_map(|e| e.as_str())
                    .any(|e| runtime_names.contains(&e.replace('\\', "/")))
            });
        let id = value.get("id").and_then(|v| v.as_u64());
        let bank_name = value.get("name").and_then(|v| v.as_str());
        if let (true, Some(id), Some(bank_name)) = (includes, id, bank_name) {
            found.push(AssetRef {
                id,
                name: bank_name.to_string(),
            });
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}
