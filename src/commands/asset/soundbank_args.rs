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

//! Command-line flags of `soundbank create` / `soundbank update` and their resolution
//! into a [`SoundbankPatch`]. Names are resolved against the project later, by the
//! service; this module only checks syntax and files.

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use serde_json::Value;

use crate::assets::SoundbankList;
use crate::common::errors::{CliError, codes};
use crate::services::soundbank::{ListPatch, SoundbankPatch};

/// Keys of a `--from-json` file besides the eight lists (an SDK soundbank is accepted).
const IGNORED_JSON_KEYS: &[&str] = &["id", "name"];

/// One repeatable, comma-separated flag per list; each replaces its whole list.
#[derive(Args, Debug, Default, Clone)]
pub struct SoundbankListArgs {
    /// Sounds to include, by name (comma-separated or repeated)
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub sound: Vec<String>,

    /// Collections to include, by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub collection: Vec<String>,

    /// Events to include, by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub event: Vec<String>,

    /// Switches to include, by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub switch: Vec<String>,

    /// Switch containers to include, by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub switch_container: Vec<String>,

    /// Effects to include, by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub effect: Vec<String>,

    /// Attenuators to include, by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub attenuator: Vec<String>,

    /// RTPCs to include, by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub rtpc: Vec<String>,
}

impl SoundbankListArgs {
    fn get(&self, list: SoundbankList) -> &[String] {
        match list {
            SoundbankList::Sounds => &self.sound,
            SoundbankList::Collections => &self.collection,
            SoundbankList::Events => &self.event,
            SoundbankList::Switches => &self.switch,
            SoundbankList::SwitchContainers => &self.switch_container,
            SoundbankList::Effects => &self.effect,
            SoundbankList::Attenuators => &self.attenuator,
            SoundbankList::Rtpc => &self.rtpc,
        }
    }
}

/// Incremental edits `update` offers per list.
#[derive(Args, Debug, Default, Clone)]
pub struct SoundbankEditArgs {
    /// Add sounds by name (comma-separated or repeated)
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub add_sound: Vec<String>,
    /// Add collections by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub add_collection: Vec<String>,
    /// Add events by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub add_event: Vec<String>,
    /// Add switches by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub add_switch: Vec<String>,
    /// Add switch containers by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub add_switch_container: Vec<String>,
    /// Add effects by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub add_effect: Vec<String>,
    /// Add attenuators by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub add_attenuator: Vec<String>,
    /// Add RTPCs by name
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub add_rtpc: Vec<String>,

    /// Remove sounds by name or by entry as listed in the file
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub remove_sound: Vec<String>,
    /// Remove collections
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub remove_collection: Vec<String>,
    /// Remove events
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub remove_event: Vec<String>,
    /// Remove switches
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub remove_switch: Vec<String>,
    /// Remove switch containers
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub remove_switch_container: Vec<String>,
    /// Remove effects
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub remove_effect: Vec<String>,
    /// Remove attenuators
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub remove_attenuator: Vec<String>,
    /// Remove RTPCs
    #[arg(long, value_name = "NAME", value_delimiter = ',')]
    pub remove_rtpc: Vec<String>,

    /// Remove every sound
    #[arg(long)]
    pub clear_sounds: bool,
    /// Remove every collection
    #[arg(long)]
    pub clear_collections: bool,
    /// Remove every event
    #[arg(long)]
    pub clear_events: bool,
    /// Remove every switch
    #[arg(long)]
    pub clear_switches: bool,
    /// Remove every switch container
    #[arg(long)]
    pub clear_switch_containers: bool,
    /// Remove every effect
    #[arg(long)]
    pub clear_effects: bool,
    /// Remove every attenuator
    #[arg(long)]
    pub clear_attenuators: bool,
    /// Remove every RTPC
    #[arg(long)]
    pub clear_rtpc: bool,
}

impl SoundbankEditArgs {
    fn added(&self, list: SoundbankList) -> &[String] {
        match list {
            SoundbankList::Sounds => &self.add_sound,
            SoundbankList::Collections => &self.add_collection,
            SoundbankList::Events => &self.add_event,
            SoundbankList::Switches => &self.add_switch,
            SoundbankList::SwitchContainers => &self.add_switch_container,
            SoundbankList::Effects => &self.add_effect,
            SoundbankList::Attenuators => &self.add_attenuator,
            SoundbankList::Rtpc => &self.add_rtpc,
        }
    }

    fn removed(&self, list: SoundbankList) -> &[String] {
        match list {
            SoundbankList::Sounds => &self.remove_sound,
            SoundbankList::Collections => &self.remove_collection,
            SoundbankList::Events => &self.remove_event,
            SoundbankList::Switches => &self.remove_switch,
            SoundbankList::SwitchContainers => &self.remove_switch_container,
            SoundbankList::Effects => &self.remove_effect,
            SoundbankList::Attenuators => &self.remove_attenuator,
            SoundbankList::Rtpc => &self.remove_rtpc,
        }
    }

    fn cleared(&self, list: SoundbankList) -> bool {
        match list {
            SoundbankList::Sounds => self.clear_sounds,
            SoundbankList::Collections => self.clear_collections,
            SoundbankList::Events => self.clear_events,
            SoundbankList::Switches => self.clear_switches,
            SoundbankList::SwitchContainers => self.clear_switch_containers,
            SoundbankList::Effects => self.clear_effects,
            SoundbankList::Attenuators => self.clear_attenuators,
            SoundbankList::Rtpc => self.clear_rtpc,
        }
    }
}

/// Every flag of `soundbank create` / `soundbank update`, in one place.
#[derive(Debug, Default, Clone)]
pub struct SoundbankFlags {
    pub lists: SoundbankListArgs,
    pub edit: SoundbankEditArgs,
    /// `--include type:name[,name]` (additive).
    pub include: Vec<String>,
    /// `--add type:name[,name]` (additive).
    pub add: Vec<String>,
    /// `--remove type:name[,name]`.
    pub remove: Vec<String>,
    pub from_json: Option<PathBuf>,
}

fn flag_name(list: SoundbankList) -> String {
    format!("--{}", list.type_key().replace('_', "-"))
}

fn field_error(what: impl Into<String>, why: impl Into<String>) -> CliError {
    CliError::new(codes::ERR_VALIDATION_FIELD, what, why).with_suggestion(
        "Example: --sound explosion,footstep --collection ambience, or --include sound:explosion",
    )
}

/// Parses one `type:name[,name...]` spec into its list and names.
pub fn parse_spec(spec: &str) -> Result<(SoundbankList, Vec<String>)> {
    let (key, names) = spec.split_once(':').ok_or_else(|| {
        field_error(
            format!("Invalid include format: '{spec}'"),
            "Include must be in format 'type:name' or 'type:name1,name2'",
        )
    })?;
    let list = SoundbankList::from_type_key(key).ok_or_else(|| {
        field_error(
            format!("Invalid asset type: '{}'", key.trim()),
            format!(
                "Asset type must be one of: {}",
                SoundbankList::ALL.map(SoundbankList::type_key).join(", ")
            ),
        )
    })?;
    let names = names
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect();
    Ok((list, names))
}

fn json_error(path: &Path, why: impl Into<String>) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("Cannot use soundbank file {}", path.display()),
        why,
    )
    .with_suggestion(
        "The file is an object whose keys are lists of asset names, for example {\"sounds\": [\"explosion\"], \"events\": [\"play_explosion\"]}",
    )
}

/// Reads the lists of a JSON file: an object keyed by list field (`sounds`, `events`, ...),
/// which may also be an SDK soundbank (its `id` and `name` are ignored).
pub fn lists_from_json(path: &Path) -> Result<Vec<(SoundbankList, Vec<String>)>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| json_error(path, format!("the file cannot be read: {e}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| json_error(path, format!("the file is not valid JSON: {e}")))?;
    let Value::Object(map) = value else {
        return Err(json_error(path, "expected an object of lists").into());
    };
    if let Some(unknown) = map.keys().find(|k| {
        !IGNORED_JSON_KEYS.contains(&k.as_str())
            && !SoundbankList::ALL.iter().any(|l| l.field() == k.as_str())
    }) {
        let valid = SoundbankList::ALL.map(SoundbankList::field).join(", ");
        return Err(json_error(path, format!("unknown key '{unknown}' (valid: {valid})")).into());
    }
    let mut lists = Vec::new();
    for list in SoundbankList::ALL {
        let Some(items) = map.get(list.field()) else {
            continue;
        };
        let names = items
            .as_array()
            .ok_or_else(|| json_error(path, format!("'{}' must be a list of names", list.field())))?
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .ok_or_else(|| {
                        json_error(
                            path,
                            format!("'{}' holds a value that is not a name", list.field()),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        lists.push((list, names));
    }
    Ok(lists)
}

fn specs(specs: &[String]) -> Result<Vec<(SoundbankList, Vec<String>)>> {
    specs.iter().map(|s| parse_spec(s)).collect()
}

fn names_for(parsed: &[(SoundbankList, Vec<String>)], list: SoundbankList) -> Vec<String> {
    parsed
        .iter()
        .filter(|(l, _)| *l == list)
        .flat_map(|(_, names)| names.clone())
        .collect()
}

/// The names that replace `list`, when exactly one source sets them.
fn replacement(
    flags: &SoundbankFlags,
    json: &[(SoundbankList, Vec<String>)],
    list: SoundbankList,
) -> Result<Option<Vec<String>>> {
    let from_flag = flags.lists.get(list);
    let from_json = json
        .iter()
        .find(|(l, _)| *l == list)
        .map(|(_, n)| n.clone());
    let cleared = flags.edit.cleared(list);
    let sources = [!from_flag.is_empty(), from_json.is_some(), cleared];
    if sources.iter().filter(|set| **set).count() > 1 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Conflicting flags for the {} list", list.field()),
            format!(
                "{}, --from-json and --clear-{} each replace the whole list",
                flag_name(list),
                list.field().replace('_', "-")
            ),
        )
        .with_suggestion("Pass only one of them; use --add-* / --remove-* for incremental edits")
        .into());
    }
    Ok(match (from_json, cleared) {
        (Some(names), _) => Some(names),
        (None, true) => Some(Vec::new()),
        (None, false) => (!from_flag.is_empty()).then(|| from_flag.to_vec()),
    })
}

/// Turns the raw flags into a patch. Fails fast on the first unusable value.
pub fn resolve_patch(flags: &SoundbankFlags) -> Result<SoundbankPatch> {
    let json = match &flags.from_json {
        Some(path) => lists_from_json(path)?,
        None => Vec::new(),
    };
    let include = specs(&flags.include)?;
    let added = specs(&flags.add)?;
    let removed = specs(&flags.remove)?;
    let mut patch = SoundbankPatch::default();
    for list in SoundbankList::ALL {
        let mut add = flags.edit.added(list).to_vec();
        add.extend(names_for(&include, list));
        add.extend(names_for(&added, list));
        let mut remove = flags.edit.removed(list).to_vec();
        remove.extend(names_for(&removed, list));
        patch = patch.with_list(
            list,
            ListPatch {
                replace: replacement(flags, &json, list)?,
                remove,
                add,
            },
        );
    }
    Ok(patch)
}
