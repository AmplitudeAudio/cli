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

//! Merges an updated asset into the JSON of the file it replaces.
//!
//! Typed round-trips drop keys the CLI does not model. These helpers carry
//! such keys over on update without reviving values the update changed.

use serde_json::{Map, Value};

/// Returns the updated asset, keeping top-level keys from `existing` that `updated` lacks.
///
/// Only top-level keys are carried over; the merge never recurses. A recursive
/// merge would resurrect stale union-variant payloads (for example an
/// attenuation zone changed from Box to Sphere) and fields the update
/// intentionally cleared. Values from `updated` are taken as-is.
///
/// The result holds every key of `updated` in its order, followed by the keys
/// of `existing` that `updated` does not contain. If either side is not a JSON
/// object, a clone of `updated` is returned. Neither input is modified.
pub fn merge_update(existing: &Value, updated: &Value) -> Value {
    let (Value::Object(existing), Value::Object(updated)) = (existing, updated) else {
        return updated.clone();
    };

    let mut merged = Map::with_capacity(existing.len() + updated.len());
    for (key, value) in updated {
        merged.insert(key.clone(), value.clone());
    }
    for (key, value) in existing {
        if !updated.contains_key(key) {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

/// Like [`merge_update`], but `existing` keys listed in `known_fields` are dropped first.
///
/// Known fields belong to the typed update, so a field the update cleared is
/// not restored from the original file. Only keys the CLI does not model are
/// carried over.
pub fn merge_update_unknown_only(
    existing: &Value,
    updated: &Value,
    known_fields: &[&str],
) -> Value {
    let Value::Object(fields) = existing else {
        return merge_update(existing, updated);
    };

    let unknown: Map<String, Value> = fields
        .iter()
        .filter(|(key, _)| !known_fields.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();

    merge_update(&Value::Object(unknown), updated)
}
