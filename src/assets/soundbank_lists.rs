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

//! The eight asset lists of a soundbank and how their entries are named.
//!
//! A soundbank does not reference assets by id. Each entry is the *runtime file name* of
//! an asset: the path of its source file below the type directory, with `.json` replaced
//! by the binary extension of its type (`sounds/fx/boom.json` is listed as
//! `fx/boom.amsound` in `sounds`). `SoundBank::InitializeInternal` joins the list's
//! directory (`sounds`, `collections`, ...) with the entry and loads that binary file;
//! the sample project of the SDK lists its assets the same way.
//!
//! This table is the name-based counterpart of `forms::references::RefRule`, which can
//! only express id references.

use super::AssetType;
use super::soundbank::Soundbank;

/// One of the eight lists of a `SoundBankDefinition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SoundbankList {
    Sounds,
    Collections,
    Events,
    Switches,
    SwitchContainers,
    Effects,
    Attenuators,
    Rtpc,
}

impl SoundbankList {
    /// Every list, in the order the CLI shows them.
    pub const ALL: [SoundbankList; 8] = [
        SoundbankList::Sounds,
        SoundbankList::Collections,
        SoundbankList::Events,
        SoundbankList::Switches,
        SoundbankList::SwitchContainers,
        SoundbankList::Effects,
        SoundbankList::Attenuators,
        SoundbankList::Rtpc,
    ];

    /// The schema field (and JSON key) of the list.
    pub fn field(self) -> &'static str {
        match self {
            Self::Sounds => "sounds",
            Self::Collections => "collections",
            Self::Events => "events",
            Self::Switches => "switches",
            Self::SwitchContainers => "switch_containers",
            Self::Effects => "effects",
            Self::Attenuators => "attenuators",
            Self::Rtpc => "rtpc",
        }
    }

    /// The asset type the entries of the list name.
    pub fn asset_type(self) -> AssetType {
        match self {
            Self::Sounds => AssetType::Sound,
            Self::Collections => AssetType::Collection,
            Self::Events => AssetType::Event,
            Self::Switches => AssetType::Switch,
            Self::SwitchContainers => AssetType::SwitchContainer,
            Self::Effects => AssetType::Effect,
            Self::Attenuators => AssetType::Attenuation,
            Self::Rtpc => AssetType::Rtpc,
        }
    }

    /// The list that holds assets of `asset_type`, if a soundbank can hold them.
    pub fn for_asset_type(asset_type: AssetType) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.asset_type() == asset_type)
    }

    /// The singular key used by `--include sound:...` and in messages (`switch_container`).
    pub fn type_key(self) -> &'static str {
        match self {
            Self::Sounds => "sound",
            Self::Collections => "collection",
            Self::Events => "event",
            Self::Switches => "switch",
            Self::SwitchContainers => "switch_container",
            Self::Effects => "effect",
            Self::Attenuators => "attenuator",
            Self::Rtpc => "rtpc",
        }
    }

    /// The list for a `--include` type key.
    pub fn from_type_key(key: &str) -> Option<Self> {
        let key = key.trim().to_lowercase().replace('-', "_");
        Self::ALL.into_iter().find(|l| l.type_key() == key)
    }

    /// The runtime extension the compiled asset gets (`.amsound`).
    ///
    /// Effects are listed as `.amfx` (the extension of `effect_definition.fbs`, used by the
    /// SDK sample project and documented on `Engine::GetEffectHandleFromFile`).
    pub fn extension(self) -> &'static str {
        match self {
            Self::Sounds => ".amsound",
            Self::Collections => ".amcollection",
            Self::Events => ".amevent",
            Self::Switches => ".amswitch",
            Self::SwitchContainers => ".amswitchcontainer",
            Self::Effects => ".amfx",
            Self::Attenuators => ".amattenuation",
            Self::Rtpc => ".amrtpc",
        }
    }

    /// Extensions an existing entry may carry. `am build` writes compiled effects as `.amfx`;
    /// `.amenv` is what older CLI versions built, so banks that list it still validate.
    pub fn accepted_extensions(self) -> &'static [&'static str] {
        match self {
            Self::Effects => &[".amfx", ".amenv"],
            Self::Sounds => &[".amsound"],
            Self::Collections => &[".amcollection"],
            Self::Events => &[".amevent"],
            Self::Switches => &[".amswitch"],
            Self::SwitchContainers => &[".amswitchcontainer"],
            Self::Attenuators => &[".amattenuation"],
            Self::Rtpc => &[".amrtpc"],
        }
    }

    /// The runtime file name of the source file `relative` (a path below the type
    /// directory, `fx/boom.json`): `fx/boom.amsound`.
    pub fn runtime_name(self, relative: &str) -> String {
        let stem = relative.strip_suffix(".json").unwrap_or(relative);
        format!("{stem}{}", self.extension())
    }

    /// The source file (below the type directory) a runtime name stands for, or `None` when
    /// the entry does not end with an extension of this list.
    pub fn source_relative(self, entry: &str) -> Option<String> {
        let entry = entry.replace('\\', "/");
        self.accepted_extensions()
            .iter()
            .find_map(|ext| entry.strip_suffix(ext))
            .filter(|stem| !stem.is_empty())
            .map(|stem| format!("{stem}.json"))
    }
}

impl Soundbank {
    /// The entries of `list` (empty when the list is absent).
    pub fn list(&self, list: SoundbankList) -> &[String] {
        self.list_option(list).as_deref().unwrap_or(&[])
    }

    /// The raw optional list, to tell an absent list from an empty one.
    pub fn list_option(&self, list: SoundbankList) -> &Option<Vec<String>> {
        match list {
            SoundbankList::Sounds => &self.sounds,
            SoundbankList::Collections => &self.collections,
            SoundbankList::Events => &self.events,
            SoundbankList::Switches => &self.switches,
            SoundbankList::SwitchContainers => &self.switch_containers,
            SoundbankList::Effects => &self.effects,
            SoundbankList::Attenuators => &self.attenuators,
            SoundbankList::Rtpc => &self.rtpc,
        }
    }

    /// A copy of the soundbank whose `list` is `entries`.
    pub fn with_list(&self, list: SoundbankList, entries: Vec<String>) -> Soundbank {
        let mut copy = self.clone();
        let slot = match list {
            SoundbankList::Sounds => &mut copy.sounds,
            SoundbankList::Collections => &mut copy.collections,
            SoundbankList::Events => &mut copy.events,
            SoundbankList::Switches => &mut copy.switches,
            SoundbankList::SwitchContainers => &mut copy.switch_containers,
            SoundbankList::Effects => &mut copy.effects,
            SoundbankList::Attenuators => &mut copy.attenuators,
            SoundbankList::Rtpc => &mut copy.rtpc,
        };
        *slot = Some(entries);
        copy
    }
}
