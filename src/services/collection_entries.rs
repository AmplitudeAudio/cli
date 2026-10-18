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

//! Collection entries: the `CollectionEntry` union seen as one plain value.
//!
//! The schema has one entry table per scheduler (`Default`, `Random`,
//! `Sequence`) that differ only by the Random `weight`. Callers describe an
//! entry as an [`EntrySpec`]; this module picks the variant that matches the
//! collection's scheduler mode, as `RandomScheduler` / `SequenceScheduler`
//! read the entries of their own kind.

use crate::assets::generated::{
    CollectionEntry, RandomSchedulerCollectionEntry, SequenceSchedulerCollectionEntry,
    SoundSchedulerMode,
};
use crate::assets::{RtpcCompatibleValue, ValidationError};
use crate::common::errors::codes::ERR_VALIDATION_FIELD;

/// Weight an entry has when none is given (the schema default).
pub const DEFAULT_WEIGHT: f32 = 1.0;

/// One sound entry of a collection, independent of the union variant.
#[derive(Debug, Clone, PartialEq)]
pub struct EntrySpec {
    pub sound: u64,
    pub gain: Option<RtpcCompatibleValue>,
    pub pitch: Option<RtpcCompatibleValue>,
    /// Selection weight; only meaningful (and only accepted) with the Random scheduler.
    pub weight: Option<f32>,
}

/// Changes to the entry list, applied in order: replace, remove, add.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntriesPatch {
    /// Replaces the whole list (an empty list clears it).
    pub replace: Option<Vec<EntrySpec>>,
    /// Sound ids whose entries are removed.
    pub remove: Vec<u64>,
    /// Entries appended at the end.
    pub add: Vec<EntrySpec>,
}

impl EntriesPatch {
    /// Whether the patch leaves the list alone.
    pub fn is_empty(&self) -> bool {
        self.replace.is_none() && self.remove.is_empty() && self.add.is_empty()
    }
}

impl EntrySpec {
    /// The spec of an existing entry (a Sequence or Default entry has no weight).
    pub fn from_entry(entry: &CollectionEntry) -> Self {
        Self {
            sound: entry.sound(),
            gain: entry.gain().cloned(),
            pitch: entry.pitch().cloned(),
            weight: entry.weight(),
        }
    }

    /// Builds the entry variant that matches `mode`.
    ///
    /// A weight on a Sequence collection is an error: the sequence entry has no
    /// such field and the value would silently vanish.
    pub fn to_entry(&self, mode: SoundSchedulerMode) -> Result<CollectionEntry, ValidationError> {
        let (sound, gain, pitch) = (self.sound, self.gain.clone(), self.pitch.clone());
        Ok(match mode {
            SoundSchedulerMode::Random => CollectionEntry::Random(RandomSchedulerCollectionEntry {
                sound,
                gain,
                pitch,
                weight: self.weight.unwrap_or(DEFAULT_WEIGHT),
            }),
            SoundSchedulerMode::Sequence => {
                if self.weight.is_some() {
                    return Err(weight_needs_random());
                }
                CollectionEntry::Sequence(SequenceSchedulerCollectionEntry { sound, gain, pitch })
            }
        })
    }
}

fn weight_needs_random() -> ValidationError {
    ValidationError::new(
        crate::assets::ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        "Entry weights need the Random scheduler",
        "Only the Random scheduler picks entries by weight; Sequence entries have no weight",
    )
    .with_suggestion("Drop the weight, or use --scheduler-mode Random")
    .with_field("sounds.weight")
}

fn convert(entry: &CollectionEntry, mode: SoundSchedulerMode) -> CollectionEntry {
    let spec = EntrySpec::from_entry(entry);
    let (sound, gain, pitch) = (spec.sound, spec.gain, spec.pitch);
    match mode {
        SoundSchedulerMode::Random => CollectionEntry::Random(RandomSchedulerCollectionEntry {
            sound,
            gain,
            pitch,
            weight: spec.weight.unwrap_or(DEFAULT_WEIGHT),
        }),
        SoundSchedulerMode::Sequence => {
            CollectionEntry::Sequence(SequenceSchedulerCollectionEntry { sound, gain, pitch })
        }
    }
}

/// Re-tags existing entries for a new scheduler mode, keeping sound, gain and pitch.
///
/// Moving to Sequence drops the weights; moving to Random gives every entry the
/// default weight.
pub fn retag(entries: &[CollectionEntry], mode: SoundSchedulerMode) -> Vec<CollectionEntry> {
    entries.iter().map(|e| convert(e, mode)).collect()
}

fn not_found_error(sound: u64) -> ValidationError {
    ValidationError::new(
        crate::assets::ValidationLayer::TypeRules,
        ERR_VALIDATION_FIELD,
        format!("The collection has no entry for sound ID {sound}"),
        "--remove-entry names a sound that is not in the collection",
    )
    .with_suggestion("Check the entries with 'am asset collection list' or the collection file")
    .with_field("sounds")
}

/// Returns the entries after `patch`; `existing` stays untouched.
///
/// New entries take the variant of `mode`. Entries that survive keep the
/// variant they have, so hand-written `Default` entries are not rewritten.
pub fn apply_entries(
    existing: &[CollectionEntry],
    patch: &EntriesPatch,
    mode: SoundSchedulerMode,
) -> Result<Vec<CollectionEntry>, ValidationError> {
    let mut entries: Vec<CollectionEntry> = match &patch.replace {
        Some(specs) => specs
            .iter()
            .map(|s| s.to_entry(mode))
            .collect::<Result<_, _>>()?,
        None => existing.to_vec(),
    };
    for sound in &patch.remove {
        let before = entries.len();
        entries.retain(|e| e.sound() != *sound);
        if entries.len() == before {
            return Err(not_found_error(*sound));
        }
    }
    for spec in &patch.add {
        entries.push(spec.to_entry(mode)?);
    }
    Ok(entries)
}
