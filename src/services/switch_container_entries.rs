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

//! Switch container entries: which object plays in which switch states, and how.

use crate::assets::generated::{FadeTransitionSettings, SwitchContainerEntry};
use crate::assets::{RtpcCompatibleValue, ValidationError, ValidationLayer};
use crate::common::errors::codes::ERR_VALIDATION_FIELD;

/// Fader of a fade that was not described (with a zero duration it means "no fade").
pub const DEFAULT_FADE_FADER: &str = "Linear";

/// A fade that does nothing. The SDK reads both fades of every entry without a null
/// check, so an entry always carries them.
pub fn no_fade() -> FadeTransitionSettings {
    FadeTransitionSettings {
        duration: 0.0,
        fader: Some(DEFAULT_FADE_FADER.to_string()),
    }
}

/// One entry of a switch container, with the fades optional.
#[derive(Debug, Clone, PartialEq)]
pub struct EntrySpec {
    /// A sound or a collection.
    pub object: u64,
    /// The states of the container's switch in which the object plays.
    pub switch_states: Vec<u64>,
    pub continue_between_states: bool,
    pub fade_in: Option<FadeTransitionSettings>,
    pub fade_out: Option<FadeTransitionSettings>,
    pub gain: Option<RtpcCompatibleValue>,
    pub pitch: Option<RtpcCompatibleValue>,
}

impl EntrySpec {
    /// An entry that plays `object` in `switch_states`, with every other setting at its default.
    pub fn new(object: u64, switch_states: Vec<u64>) -> Self {
        Self {
            object,
            switch_states,
            continue_between_states: false,
            fade_in: None,
            fade_out: None,
            gain: None,
            pitch: None,
        }
    }

    /// The schema entry; a fade left unset becomes [`no_fade`].
    pub fn to_entry(&self) -> SwitchContainerEntry {
        SwitchContainerEntry {
            object: self.object,
            switch_states: self.switch_states.clone(),
            continue_between_states: self.continue_between_states,
            fade_in: Some(self.fade_in.clone().unwrap_or_else(no_fade)),
            fade_out: Some(self.fade_out.clone().unwrap_or_else(no_fade)),
            gain: self.gain.clone(),
            pitch: self.pitch.clone(),
        }
    }
}

/// Changes to the entry list, applied in order: replace, remove, add, map.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntriesPatch {
    /// Replaces the whole list (an empty list clears it).
    pub replace: Option<Vec<EntrySpec>>,
    /// Object ids whose entries are removed.
    pub remove: Vec<u64>,
    /// Entries appended at the end.
    pub add: Vec<EntrySpec>,
    /// `(object, state)` pairs: the state is added to the entry of the object, which is
    /// created (with default settings) when the container has none.
    pub map: Vec<(u64, u64)>,
}

impl EntriesPatch {
    /// Whether the patch leaves the list alone.
    pub fn is_empty(&self) -> bool {
        self.replace.is_none() && self.remove.is_empty() && self.add.is_empty() && self.map.is_empty()
    }

    /// The entries the patch brings in (replacements, additions, then mappings), as
    /// a new container receives them.
    pub fn incoming(&self) -> Vec<EntrySpec> {
        let mut entries: Vec<EntrySpec> = self
            .replace
            .iter()
            .flatten()
            .chain(&self.add)
            .cloned()
            .collect();
        for (object, state) in &self.map {
            match entries.iter_mut().find(|e| e.object == *object) {
                Some(entry) if entry.switch_states.contains(state) => {}
                Some(entry) => entry.switch_states.push(*state),
                None => entries.push(EntrySpec::new(*object, vec![*state])),
            }
        }
        entries
    }
}

fn not_found_error(object: u64) -> ValidationError {
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        format!("The switch container has no entry for object ID {object}"),
        "--remove-entry names an object that has no entry in the container",
    )
    .with_suggestion("Check the entries in the switch container file")
    .with_field("entries")
}

/// Returns the entries after `patch`; `existing` stays untouched.
pub fn apply_entries(
    existing: &[SwitchContainerEntry],
    patch: &EntriesPatch,
) -> Result<Vec<SwitchContainerEntry>, ValidationError> {
    let mut entries: Vec<SwitchContainerEntry> = match &patch.replace {
        Some(specs) => specs.iter().map(EntrySpec::to_entry).collect(),
        None => existing.to_vec(),
    };
    for object in &patch.remove {
        let before = entries.len();
        entries.retain(|e| e.object != *object);
        if entries.len() == before {
            return Err(not_found_error(*object));
        }
    }
    entries.extend(patch.add.iter().map(EntrySpec::to_entry));
    for (object, state) in &patch.map {
        match entries.iter_mut().find(|e| e.object == *object) {
            Some(entry) if entry.switch_states.contains(state) => {}
            Some(entry) => entry.switch_states.push(*state),
            None => entries.push(EntrySpec::new(*object, vec![*state]).to_entry()),
        }
    }
    Ok(entries)
}
