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

//! Tests for the pure switch container service.

use am::assets::{ProjectContext, SwitchContainer};
use am::services::switch_container::{
    SwitchContainerPatch, SwitchContainerValues, apply_patch, build_switch_container,
};
use am::services::switch_container_entries::{EntriesPatch, EntrySpec, apply_entries, no_fade};

fn values(bus: u64, group: u64, state: u64) -> SwitchContainerValues {
    let patch = SwitchContainerPatch {
        bus: Some(bus),
        switch_group: Some(group),
        default_switch_state: Some(state),
        ..Default::default()
    };
    SwitchContainerValues::from_patch("steps", &patch, &ProjectContext::empty()).unwrap()
}

#[test]
fn test_build_rejects_unset_required_references() {
    let ctx = ProjectContext::empty();
    for (bus, group, state, field) in [
        (0, 10, 11, "bus"),
        (2, 0, 11, "switch_group"),
        (2, 10, 0, "default_switch_state"),
    ] {
        let err = build_switch_container(values(bus, group, state), &ctx).unwrap_err();
        assert_eq!(err.field.as_deref(), Some(field));
    }
}

#[test]
fn test_build_fills_sdk_required_defaults() {
    let c = build_switch_container(values(2, 10, 11), &ProjectContext::empty()).unwrap();
    assert_eq!(c.fader.as_deref(), Some("Linear"));
    assert_eq!(c.gain.unwrap().as_static(), Some(1.0));
    assert_eq!(c.entries, Some(vec![]));
}

#[test]
fn test_entries_get_no_fade_defaults_and_map_merges_states() {
    let entry = EntrySpec::new(101, vec![11]).to_entry();
    assert_eq!(entry.fade_in, Some(no_fade()));
    assert_eq!(entry.fade_out, Some(no_fade()));

    let patch = EntriesPatch {
        map: vec![(101, 12), (101, 12), (102, 11)],
        ..Default::default()
    };
    let result = apply_entries(&[entry], &patch).unwrap();
    assert_eq!(result[0].switch_states, vec![11, 12]);
    assert_eq!(result[1].object, 102);
}

#[test]
fn test_apply_patch_leaves_the_original_and_unset_fields_alone() {
    let existing: SwitchContainer =
        build_switch_container(values(2, 10, 11), &ProjectContext::empty()).unwrap();
    let before = existing.clone();
    let patch = SwitchContainerPatch {
        effect: Some(66),
        ..Default::default()
    };
    let updated = apply_patch(&existing, &patch).unwrap();
    assert_eq!(existing, before);
    assert_eq!(updated.effect, 66);
    assert_eq!(updated.bus, before.bus);
    assert_eq!(updated.entries, before.entries);
    assert_eq!(patch.changed_fields(), vec!["effect"]);
}
