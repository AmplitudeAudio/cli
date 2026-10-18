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

//! Interactive prompts for `soundbank create` and `soundbank update`.
//!
//! Every function here is only called when the session can prompt. Assets are picked
//! from the project's source assets, one multi-select per type; the answers are source
//! asset names, which the service resolves to runtime file names.

use anyhow::Result;

use crate::assets::{Soundbank, SoundbankList};
use crate::input::{Input, select_index};
use crate::services::soundbank::{Catalog, ListPatch, SoundbankPatch};

use super::sound_prompts::ACCEPT_DEFAULTS_PROMPT;

const DONE_LABEL: &str = "Done";
const ADD_LABEL: &str = "Add assets";
const REMOVE_LABEL: &str = "Remove assets";

fn title(list: SoundbankList) -> String {
    let field = list.field().replace('_', " ");
    let mut chars = field.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// A source asset's label and the token that names it unambiguously (its file path).
fn choices(catalog: &Catalog, list: SoundbankList, taken: &[String]) -> Vec<(String, String)> {
    catalog
        .sources(list)
        .iter()
        .filter(|s| !Catalog::runtime_names(list, s).iter().any(|r| taken.contains(r)))
        .map(|s| {
            let token = s.relative.trim_end_matches(".json").to_string();
            let label = if token == s.name {
                s.name.clone()
            } else {
                format!("{} ({token})", s.name)
            };
            (label, token)
        })
        .collect()
}

fn pick(input: &dyn Input, prompt: &str, options: &[(String, String)]) -> Result<Vec<String>> {
    let labels: Vec<String> = options.iter().map(|(label, _)| label.clone()).collect();
    let picked = input.multi_select(prompt, &labels)?;
    Ok(picked
        .iter()
        .filter_map(|p| options.iter().find(|(label, _)| label == p))
        .map(|(_, token)| token.clone())
        .collect())
}

/// Walks a new soundbank through the assets it can include, one type at a time. After the
/// first type that yields an asset the author may accept the defaults (nothing else).
/// Flag values already in the patch are never asked for again.
pub fn complete_create_patch(
    input: &dyn Input,
    catalog: &Catalog,
    patch: SoundbankPatch,
) -> Result<SoundbankPatch> {
    if !patch.is_empty() {
        return Ok(patch);
    }
    let mut patch = patch;
    let mut asked_defaults = false;
    for list in SoundbankList::ALL {
        let options = choices(catalog, list, &[]);
        if options.is_empty() {
            continue;
        }
        if !patch.is_empty() && !asked_defaults {
            asked_defaults = true;
            if input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))? {
                break;
            }
        }
        let tokens = pick(input, &format!("{} to include", title(list)), &options)?;
        patch = patch.with_list(
            list,
            ListPatch {
                add: tokens,
                ..Default::default()
            },
        );
    }
    Ok(patch)
}

/// Lets the user add and remove assets list by list; untouched lists stay out of the patch.
///
/// Entries the project no longer has (or that an older CLI wrote with source names) show
/// as they are in the file and can be removed here.
pub fn prompt_update_patch(
    input: &dyn Input,
    catalog: &Catalog,
    bank: &Soundbank,
) -> Result<SoundbankPatch> {
    let mut patch = SoundbankPatch::default();
    loop {
        let lists: Vec<SoundbankList> = SoundbankList::ALL
            .into_iter()
            .filter(|l| !bank.list(*l).is_empty() || !catalog.sources(*l).is_empty())
            .collect();
        let mut menu: Vec<String> = lists
            .iter()
            .map(|l| format!("{} ({} in the soundbank)", title(*l), bank.list(*l).len()))
            .collect();
        menu.push(DONE_LABEL.to_string());
        let index = select_index(input, "Which assets do you want to change?", &menu)?;
        let Some(list) = lists.get(index).copied() else {
            return Ok(patch);
        };
        let action = select_index(
            input,
            &format!("What to do with the {}?", list.field().replace('_', " ")),
            &[ADD_LABEL, REMOVE_LABEL],
        )?;
        let change = patch.list(list).cloned().unwrap_or_default();
        let change = if action == 0 {
            let taken = bank.list(list).to_vec();
            let options = choices(catalog, list, &taken);
            let tokens = pick(input, &format!("{} to add", title(list)), &options)?;
            ListPatch {
                add: [change.add, tokens].concat(),
                ..change
            }
        } else {
            let options: Vec<(String, String)> = bank
                .list(list)
                .iter()
                .map(|e| (e.clone(), e.clone()))
                .collect();
            let tokens = pick(input, &format!("{} to remove", title(list)), &options)?;
            ListPatch {
                remove: [change.remove, tokens].concat(),
                ..change
            }
        };
        patch = patch.with_list(list, change);
    }
}
