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

//! Soundbank creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI describe the lists of a soundbank with *source asset
//! names* (`explosion`, `fx/boom`); this module resolves them against a [`Catalog`] of the
//! project's source assets and writes the *runtime file names* the SDK loads
//! (`explosion.amsound`, see [`SoundbankList`]). It never touches the terminal or the
//! file system, and returns new values instead of mutating its inputs.

use std::collections::BTreeMap;

use crate::assets::validator::SourceAsset;
use crate::assets::{
    Asset, ProjectContext, ProjectValidator, Soundbank, SoundbankList, ValidationError,
    ValidationLayer,
};
use crate::common::errors::codes::{ERR_VALIDATION_FIELD, ERR_VALIDATION_REFERENCE};
use crate::services::sound::unique_id;

/// The source assets of a project, per soundbank list: what a soundbank entry can name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    sources: BTreeMap<SoundbankList, Vec<SourceAsset>>,
}

impl Catalog {
    /// A catalog holding `sources` (lists without an entry have no assets).
    pub fn new(sources: impl IntoIterator<Item = (SoundbankList, Vec<SourceAsset>)>) -> Self {
        Self {
            sources: sources.into_iter().collect(),
        }
    }

    /// The catalog of the assets `validator` scanned.
    pub fn from_validator(validator: &ProjectValidator) -> Self {
        Self::new(
            SoundbankList::ALL
                .into_iter()
                .map(|list| (list, validator.source_assets(list.asset_type()))),
        )
    }

    /// The source assets a `list` entry can name.
    pub fn sources(&self, list: SoundbankList) -> &[SourceAsset] {
        self.sources.get(&list).map_or(&[], Vec::as_slice)
    }

    /// The asset names of `list`, in file order.
    pub fn names(&self, list: SoundbankList) -> Vec<String> {
        self.sources(list).iter().map(|s| s.name.clone()).collect()
    }

    /// The runtime file name a `list` entry for `token` stands for.
    ///
    /// `token` is an asset name, a source file path below the type directory without
    /// `.json` (`fx/boom`), or a runtime name already (`fx/boom.amsound`).
    pub fn resolve(&self, list: SoundbankList, token: &str) -> Result<String, ValidationError> {
        let token = token.trim();
        let sources = self.sources(list);
        let by_name: Vec<&SourceAsset> = sources.iter().filter(|s| s.name == token).collect();
        match by_name.as_slice() {
            [one] => return Ok(list.runtime_name(&one.relative)),
            [] => {}
            many => return Err(ambiguous(list, token, many)),
        }
        let stem = token.replace('\\', "/");
        if let Some(source) = sources
            .iter()
            .find(|s| s.relative.strip_suffix(".json") == Some(stem.as_str()))
        {
            return Ok(list.runtime_name(&source.relative));
        }
        if let Some(relative) = list.source_relative(token)
            && sources.iter().any(|s| s.relative == relative)
        {
            return Ok(token.replace('\\', "/"));
        }
        Err(unknown_asset(list, token, &self.names(list)))
    }

    /// Every runtime name `asset` can be listed under (effects have two accepted spellings).
    pub fn runtime_names(list: SoundbankList, asset: &SourceAsset) -> Vec<String> {
        let stem = asset
            .relative
            .strip_suffix(".json")
            .unwrap_or(&asset.relative);
        list.accepted_extensions()
            .iter()
            .map(|ext| format!("{stem}{ext}"))
            .collect()
    }
}

fn ambiguous(list: SoundbankList, token: &str, matches: &[&SourceAsset]) -> ValidationError {
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_REFERENCE,
        format!("More than one {} is named '{token}'", list.type_key()),
        "A soundbank entry names one file, so the name has to pick one",
    )
    .with_suggestion(format!(
        "Use the file path instead: {}",
        matches
            .iter()
            .map(|s| s.relative.trim_end_matches(".json").to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
    .with_field(list.field())
}

fn unknown_asset(list: SoundbankList, token: &str, available: &[String]) -> ValidationError {
    let type_key = list.type_key();
    let listing = if available.is_empty() {
        format!("the project has no {type_key} yet")
    } else {
        format!("available {}: {}", list.field(), available.join(", "))
    };
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_REFERENCE,
        format!("No {type_key} named '{token}' in the project"),
        format!("A soundbank can only include assets that exist in sources/ ({listing})"),
    )
    .with_suggestion(format!(
        "Pass the name of an existing {type_key}, or create it first with 'am asset {} create {token}'",
        type_key.replace('_', "-")
    ))
    .with_field(list.field())
}

/// Changes to one list, applied in order: replace, remove, add. Every item is a source
/// asset name (see [`Catalog::resolve`]); `remove` also accepts an entry as written in
/// the file, so stale entries can be dropped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListPatch {
    /// Replaces the whole list (an empty list clears it).
    pub replace: Option<Vec<String>>,
    pub remove: Vec<String>,
    pub add: Vec<String>,
}

impl ListPatch {
    /// Whether the patch leaves the list alone.
    pub fn is_empty(&self) -> bool {
        self.replace.is_none() && self.remove.is_empty() && self.add.is_empty()
    }
}

/// The lists of a soundbank a caller wants to change; unset lists are kept.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SoundbankPatch {
    pub lists: BTreeMap<SoundbankList, ListPatch>,
}

impl SoundbankPatch {
    /// A copy of the patch with `change` applied to `list` (the previous change is kept).
    pub fn with_list(mut self, list: SoundbankList, change: ListPatch) -> Self {
        if !change.is_empty() {
            self.lists.insert(list, change);
        }
        self
    }

    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.lists.values().all(ListPatch::is_empty)
    }

    /// The change to `list`, if any.
    pub fn list(&self, list: SoundbankList) -> Option<&ListPatch> {
        self.lists.get(&list).filter(|p| !p.is_empty())
    }

    /// Names of the fields this patch sets, in schema-list order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        SoundbankList::ALL
            .into_iter()
            .filter(|l| self.list(*l).is_some())
            .map(SoundbankList::field)
            .collect()
    }
}

/// Every value a new soundbank needs: its name and its resolved runtime entries.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundbankValues {
    pub name: String,
    pub lists: BTreeMap<SoundbankList, Vec<String>>,
}

impl SoundbankValues {
    /// Turns a patch into the values of a new soundbank. A bank without assets is valid
    /// (the SDK loads it), so an empty patch gives eight empty lists.
    pub fn from_patch(
        name: &str,
        patch: &SoundbankPatch,
        catalog: &Catalog,
    ) -> Result<Self, ValidationError> {
        let mut lists = BTreeMap::new();
        for list in SoundbankList::ALL {
            let entries = match patch.list(list) {
                Some(change) => apply_list(catalog, list, &[], change)?,
                None => Vec::new(),
            };
            lists.insert(list, entries);
        }
        Ok(Self {
            name: name.to_string(),
            lists,
        })
    }
}

/// Builds a validated soundbank from `values`, generating a unique id.
pub fn build_soundbank(
    values: SoundbankValues,
    ctx: &ProjectContext,
) -> Result<Soundbank, ValidationError> {
    let id = unique_id("soundbank", &values.name, ctx)?;
    let bank = values.lists.into_iter().fold(
        Soundbank::builder(id, &values.name).build(),
        |bank, (list, entries)| bank.with_list(list, entries),
    );
    bank.validate_rules(ctx)?;
    Ok(bank)
}

fn entry_not_found(list: SoundbankList, token: &str, entries: &[String]) -> ValidationError {
    let current = if entries.is_empty() {
        format!("the soundbank has no {}", list.field())
    } else {
        format!("current {}: {}", list.field(), entries.join(", "))
    };
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        format!("'{token}' is not in the soundbank's {}", list.field()),
        format!("Only an entry the soundbank holds can be removed ({current})"),
    )
    .with_suggestion("Use a name, or an entry exactly as listed in the soundbank")
    .with_field(list.field())
}

/// The entries of `list` after `change`; `existing` stays untouched.
fn apply_list(
    catalog: &Catalog,
    list: SoundbankList,
    existing: &[String],
    change: &ListPatch,
) -> Result<Vec<String>, ValidationError> {
    let mut entries: Vec<String> = Vec::new();
    let push_new = |entries: &mut Vec<String>, entry: String| {
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    };
    match &change.replace {
        Some(tokens) => {
            for token in tokens {
                push_new(&mut entries, catalog.resolve(list, token)?);
            }
        }
        None => existing
            .iter()
            .for_each(|e| push_new(&mut entries, e.clone())),
    }
    for token in &change.remove {
        let token = token.trim();
        let position = entries.iter().position(|e| e == token).or_else(|| {
            let resolved = catalog.resolve(list, token).ok()?;
            entries.iter().position(|e| *e == resolved)
        });
        match position {
            Some(index) => {
                entries.remove(index);
            }
            None => return Err(entry_not_found(list, token, &entries)),
        }
    }
    for token in &change.add {
        push_new(&mut entries, catalog.resolve(list, token)?);
    }
    Ok(entries)
}

/// Merges `patch` into a copy of `existing`; lists the patch leaves unset are preserved.
///
/// An absent list is written as an empty one: the SDK dereferences all eight lists
/// without a null check. The result is not validated: callers run `validate_rules` on it.
pub fn apply_patch(
    existing: &Soundbank,
    patch: &SoundbankPatch,
    catalog: &Catalog,
) -> Result<Soundbank, ValidationError> {
    let mut bank = existing.clone();
    for list in SoundbankList::ALL {
        let entries = match patch.list(list) {
            Some(change) => Some(apply_list(catalog, list, existing.list(list), change)?),
            None if existing.list_option(list).is_none() => Some(Vec::new()),
            None => None,
        };
        if let Some(entries) = entries {
            bank = bank.with_list(list, entries);
        }
    }
    Ok(bank)
}
