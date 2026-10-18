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

//! Bus creation, update and removal inside one bus file, independent of how
//! values were gathered.
//!
//! Every function takes the parsed file and returns a new, validated one
//! (`assets::bus::validate_bus_file`); inputs are never mutated and nothing
//! touches the file system. Keys the CLI does not model, on the file and on
//! every bus entry, are carried over unchanged.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::assets::bus::{
    DEFAULT_FADER, MASTER_BUS_ID, MASTER_BUS_NAME, buses_of, validate_bus_file,
};
use crate::common::errors::{CliError, codes};
use crate::common::json_merge::merge_update_unknown_only;
use crate::common::utils::generate_unique_id;

/// Number of times id generation is retried after a collision.
const ID_COLLISION_RETRIES: usize = 8;

/// Schema fields of `BusDefinition` the CLI models.
const KNOWN_BUS_KEYS: &[&str] = &["id", "name", "gain", "child_buses", "duck_buses", "fader"];

/// One duck bus requested on a bus (a `DuckBusDefinition`), by bus name.
#[derive(Debug, Clone, PartialEq)]
pub struct DuckSpec {
    pub bus: String,
    pub target_gain: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub fade_in_fader: String,
    pub fade_out_fader: String,
}

/// Every value a new bus needs.
#[derive(Debug, Clone, PartialEq)]
pub struct BusValues {
    pub name: String,
    pub gain: f64,
    pub parent: String,
    pub fader: String,
    pub ducks: Vec<DuckSpec>,
}

impl BusValues {
    /// Values with the defaults the commands use (gain 1, parent master, Linear fader).
    pub fn with_defaults(name: &str) -> Self {
        Self {
            name: name.to_string(),
            gain: 1.0,
            parent: MASTER_BUS_NAME.to_string(),
            fader: DEFAULT_FADER.to_string(),
            ducks: Vec::new(),
        }
    }
}

/// The fields of an existing bus a caller wants to change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BusPatch {
    pub gain: Option<f64>,
    pub parent: Option<String>,
    pub fader: Option<String>,
    /// `Some` replaces every duck bus (an empty list clears them).
    pub ducks: Option<Vec<DuckSpec>>,
}

impl BusPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Schema names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("gain", self.gain.is_some()),
            ("parent", self.parent.is_some()),
            ("fader", self.fader.is_some()),
            ("duck_buses", self.ducks.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

/// A row for listings.
#[derive(Debug, Clone, PartialEq)]
pub struct BusSummary {
    pub id: u64,
    pub name: String,
    pub gain: f64,
    pub fader: String,
    pub parent: Option<String>,
    pub children: usize,
    pub ducks: usize,
}

/// The modelled fields of a bus entry; everything else stays in the raw JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BusEntry {
    #[serde(default)]
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gain: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    child_buses: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    duck_buses: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fader: Option<String>,
}

fn field_error(code: i32, what: String, why: String, suggestion: String) -> CliError {
    CliError::new(code, what, why).with_suggestion(suggestion)
}

fn not_found(name: &str) -> CliError {
    field_error(
        codes::ERR_ASSET_NOT_FOUND,
        format!("Bus '{name}' not found"),
        "No bus with this name exists in the bus file".to_string(),
        "Use 'am asset bus list' to see available buses".to_string(),
    )
}

/// A fresh bus file content holding only the `master` bus.
pub fn new_bus_file() -> Value {
    json!({"buses": [{
        "id": MASTER_BUS_ID,
        "name": MASTER_BUS_NAME,
        "gain": 1.0,
        "fader": DEFAULT_FADER,
    }]})
}

/// Checks a bus name: letters, digits, `_`, `-` and `.` only (it is used in flags like `--duck`).
pub fn validate_bus_name(name: &str) -> Result<(), CliError> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if valid {
        return Ok(());
    }
    Err(field_error(
        codes::ERR_VALIDATION_FIELD,
        format!("Invalid bus name '{name}'"),
        "A bus name may only contain letters, digits, underscores, hyphens and dots".to_string(),
        "Choose a name such as 'sfx' or 'ui-sounds'".to_string(),
    ))
}

/// The entry of the bus called `name`.
pub fn find_bus<'a>(file: &'a Value, name: &str) -> Option<&'a Value> {
    buses_of(file)
        .iter()
        .find(|b| b.get("name").and_then(Value::as_str) == Some(name))
}

fn id_of(bus: &Value) -> u64 {
    bus.get("id").and_then(Value::as_u64).unwrap_or(0)
}

fn name_of(bus: &Value) -> &str {
    bus.get("name").and_then(Value::as_str).unwrap_or_default()
}

fn children_of(bus: &Value) -> Vec<u64> {
    bus.get("child_buses")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default()
}

/// Names of the buses listing `id` as a child (at most one in a valid file).
fn parent_entry(file: &Value, id: u64) -> Option<&Value> {
    buses_of(file).iter().find(|b| children_of(b).contains(&id))
}

/// Name of the parent of the bus called `name`, if it has one.
pub fn parent_name(file: &Value, name: &str) -> Option<String> {
    let id = id_of(find_bus(file, name)?);
    parent_entry(file, id).map(|p| name_of(p).to_string())
}

/// Names of the buses that duck the bus `id`.
pub fn duckers(file: &Value, id: u64) -> Vec<String> {
    buses_of(file)
        .iter()
        .filter(|b| {
            b.get("duck_buses")
                .and_then(Value::as_array)
                .is_some_and(|d| {
                    d.iter()
                        .any(|x| x.get("id").and_then(Value::as_u64) == Some(id))
                })
        })
        .map(|b| name_of(b).to_string())
        .collect()
}

/// One summary row per bus, in file order.
pub fn summarize(file: &Value) -> Vec<BusSummary> {
    buses_of(file)
        .iter()
        .map(|b| BusSummary {
            id: id_of(b),
            name: name_of(b).to_string(),
            gain: b.get("gain").and_then(Value::as_f64).unwrap_or(1.0),
            fader: b
                .get("fader")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            parent: parent_entry(file, id_of(b)).map(|p| name_of(p).to_string()),
            children: children_of(b).len(),
            ducks: b
                .get("duck_buses")
                .and_then(Value::as_array)
                .map_or(0, Vec::len),
        })
        .collect()
}

fn with_buses(file: &Value, buses: Vec<Value>) -> Value {
    let mut next = file.clone();
    if let Some(object) = next.as_object_mut() {
        object.insert("buses".to_string(), Value::Array(buses));
    }
    next
}

/// Re-serialises the entry through `edit`, keeping keys the CLI does not know.
fn rewrite_entry(raw: &Value, edit: impl FnOnce(BusEntry) -> BusEntry) -> Value {
    let entry: BusEntry = serde_json::from_value(raw.clone()).unwrap_or_default();
    let updated = serde_json::to_value(edit(entry)).unwrap_or(Value::Null);
    merge_update_unknown_only(raw, &updated, KNOWN_BUS_KEYS)
}

fn with_child_change(file: &Value, parent_id: u64, change: impl Fn(&mut Vec<u64>)) -> Vec<Value> {
    buses_of(file)
        .iter()
        .map(|bus| {
            if id_of(bus) != parent_id {
                return bus.clone();
            }
            rewrite_entry(bus, |mut entry| {
                change(&mut entry.child_buses);
                entry
            })
        })
        .collect()
}

fn duck_value(file: &Value, spec: &DuckSpec) -> Result<Value, CliError> {
    let Some(target) = find_bus(file, &spec.bus) else {
        return Err(field_error(
            codes::ERR_VALIDATION_REFERENCE,
            format!("Duck bus '{}' is not defined in this bus file", spec.bus),
            "A duck bus must be a bus of the same file".to_string(),
            "Pass --duck with the name of a bus of the file (see 'am asset bus list')".to_string(),
        ));
    };
    Ok(json!({
        "id": id_of(target),
        "target_gain": spec.target_gain,
        "fade_in": {"duration": spec.fade_in, "fader": spec.fade_in_fader},
        "fade_out": {"duration": spec.fade_out, "fader": spec.fade_out_fader},
    }))
}

fn duck_values(file: &Value, specs: &[DuckSpec]) -> Result<Vec<Value>, CliError> {
    specs.iter().map(|spec| duck_value(file, spec)).collect()
}

fn resolve_parent(file: &Value, parent: &str) -> Result<u64, CliError> {
    find_bus(file, parent).map(id_of).ok_or_else(|| {
        field_error(
            codes::ERR_VALIDATION_REFERENCE,
            format!("Parent bus '{parent}' is not defined in this bus file"),
            "A parent bus must be a bus of the same file: the SDK loads one file as one bus tree"
                .to_string(),
            "Pass --parent with the name of a bus of the file (see 'am asset bus list')"
                .to_string(),
        )
    })
}

fn unique_id(file: &Value, name: &str, is_taken: &dyn Fn(u64) -> bool) -> Result<u64, CliError> {
    let taken = |id: u64| {
        id <= MASTER_BUS_ID || is_taken(id) || buses_of(file).iter().any(|b| id_of(b) == id)
    };
    let mut id = generate_unique_id(name);
    for attempt in 0..ID_COLLISION_RETRIES {
        if !taken(id) {
            return Ok(id);
        }
        id = generate_unique_id(&format!("{name}{attempt}"));
    }
    if !taken(id) {
        return Ok(id);
    }
    Err(field_error(
        codes::ERR_ASSET_ALREADY_EXISTS,
        format!("Generated ID {id} for bus '{name}' collides with an existing object"),
        "All generated ID attempts collided with existing buses or assets".to_string(),
        "Try a different name or wait a moment and retry".to_string(),
    ))
}

fn validated(file: Value) -> Result<Value, CliError> {
    validate_bus_file(&file).map_err(CliError::from)?;
    Ok(file)
}

/// Adds a bus to `file` and registers it as a child of its parent.
///
/// `is_taken` reports ids already used by other objects of the project (assets
/// and buses of other files). Returns the new file content and the new bus id.
pub fn add_bus(
    file: &Value,
    values: &BusValues,
    is_taken: &dyn Fn(u64) -> bool,
) -> Result<(Value, u64), CliError> {
    validate_bus_name(&values.name)?;
    if find_bus(file, &values.name).is_some() {
        return Err(field_error(
            codes::ERR_ASSET_ALREADY_EXISTS,
            format!("Bus '{}' already exists", values.name),
            "A bus with this name already exists in the bus file".to_string(),
            format!("Use 'am asset bus update {}' to modify it", values.name),
        ));
    }
    let parent_id = resolve_parent(file, &values.parent)?;
    let id = unique_id(file, &values.name, is_taken)?;

    let entry = BusEntry {
        id,
        name: values.name.clone(),
        gain: Some(values.gain),
        child_buses: Vec::new(),
        duck_buses: duck_values(file, &values.ducks)?,
        fader: Some(values.fader.clone()),
    };
    let entry = serde_json::to_value(entry).unwrap_or(Value::Null);

    let mut buses = with_child_change(file, parent_id, |children| children.push(id));
    buses.push(entry);
    Ok((validated(with_buses(file, buses))?, id))
}

fn reparent(file: &Value, bus_id: u64, new_parent: &str) -> Result<Vec<Value>, CliError> {
    if bus_id == MASTER_BUS_ID {
        return Err(field_error(
            codes::ERR_VALIDATION_FIELD,
            format!("The '{MASTER_BUS_NAME}' bus cannot have a parent"),
            "It is the root of the bus tree".to_string(),
            "Remove --parent".to_string(),
        ));
    }
    let new_parent_id = resolve_parent(file, new_parent)?;
    let old_parent_id = parent_entry(file, bus_id).map(id_of);
    let without = match old_parent_id {
        Some(old) => with_child_change(file, old, |c| c.retain(|x| *x != bus_id)),
        None => buses_of(file).to_vec(),
    };
    let intermediate = with_buses(file, without);
    Ok(with_child_change(&intermediate, new_parent_id, |c| {
        if !c.contains(&bus_id) {
            c.push(bus_id);
        }
    }))
}

/// Applies `patch` to the bus called `name`.
pub fn update_bus(file: &Value, name: &str, patch: &BusPatch) -> Result<Value, CliError> {
    let Some(target) = find_bus(file, name) else {
        return Err(not_found(name));
    };
    let id = id_of(target);

    let buses = match &patch.parent {
        Some(parent) => reparent(file, id, parent)?,
        None => buses_of(file).to_vec(),
    };
    let ducks = patch
        .ducks
        .as_ref()
        .map(|specs| duck_values(file, specs))
        .transpose()?;

    let buses = buses
        .iter()
        .map(|bus| {
            if id_of(bus) != id {
                return bus.clone();
            }
            rewrite_entry(bus, |mut entry| {
                if let Some(gain) = patch.gain {
                    entry.gain = Some(gain);
                }
                if let Some(fader) = &patch.fader {
                    entry.fader = Some(fader.clone());
                }
                if let Some(ducks) = ducks.clone() {
                    entry.duck_buses = ducks;
                }
                entry
            })
        })
        .collect();
    validated(with_buses(file, buses))
}

/// Removes the bus called `name` and its entry in its parent's `child_buses`.
///
/// Refuses `master`, buses that still have children and buses another bus ducks.
pub fn remove_bus(file: &Value, name: &str) -> Result<Value, CliError> {
    let Some(target) = find_bus(file, name) else {
        return Err(not_found(name));
    };
    let id = id_of(target);
    if id == MASTER_BUS_ID {
        return Err(field_error(
            codes::ERR_VALIDATION_FIELD,
            format!("The '{MASTER_BUS_NAME}' bus cannot be deleted"),
            "The SDK needs it as the root of the bus tree".to_string(),
            "Delete the whole file with 'am asset bus-file delete' if it is no longer needed"
                .to_string(),
        ));
    }

    let child_names: Vec<String> = children_of(target)
        .iter()
        .filter_map(|c| buses_of(file).iter().find(|b| id_of(b) == *c))
        .map(|b| name_of(b).to_string())
        .collect();
    if !child_names.is_empty() {
        return Err(field_error(
            codes::ERR_ASSET_IN_USE,
            format!("Bus '{name}' still has child buses"),
            format!("Its children are: {}", child_names.join(", ")),
            "Delete the child buses, or move them with 'am asset bus update <child> --parent <bus>', first"
                .to_string(),
        ));
    }
    let ducking = duckers(file, id);
    if !ducking.is_empty() {
        return Err(field_error(
            codes::ERR_ASSET_IN_USE,
            format!("Bus '{name}' is still a duck bus of other buses"),
            format!("It is ducked by: {}", ducking.join(", ")),
            "Remove it from their duck buses first (am asset bus update <bus> --duck ... or --clear-ducks)"
                .to_string(),
        ));
    }

    let parent_id = parent_entry(file, id).map(id_of);
    let buses = match parent_id {
        Some(parent) => with_child_change(file, parent, |c| c.retain(|x| *x != id)),
        None => buses_of(file).to_vec(),
    };
    let buses = buses.into_iter().filter(|b| id_of(b) != id).collect();
    validated(with_buses(file, buses))
}
