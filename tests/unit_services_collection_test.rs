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

//! Tests for the collection service (value building and patching, no I/O front end).

mod common;

use am::assets::generated::{
    CollectionEntry, RandomSoundSchedulerConfig, SequenceSoundSchedulerConfig,
    SequenceSoundSchedulerEndBehavior, SoundSchedulerConfig, SoundSchedulerMode,
};
use am::assets::{Asset, Collection, ProjectContext, RtpcCompatibleValue};
use am::common::errors::codes;
use am::services::collection::{CollectionPatch, CollectionValues, apply_patch, build_collection};
use am::services::collection_entries::{EntriesPatch, EntrySpec};
use common::fixtures::AssetTestFixture;
use serde_json::json;

fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.create_test_sound("step1", 101).unwrap();
    fx.create_test_sound("step2", 102).unwrap();
    fx.write_asset_json("effects", "echo", json!({"id": 66, "name": "echo"}))
        .unwrap();
    fx
}

fn context(fx: &AssetTestFixture) -> ProjectContext {
    let validator = fx.create_project_validator().unwrap();
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator)
}

fn spec(sound: u64) -> EntrySpec {
    EntrySpec {
        sound,
        gain: None,
        pitch: None,
        weight: None,
    }
}

fn patch_with_bus(bus: u64) -> CollectionPatch {
    CollectionPatch {
        bus: Some(bus),
        ..Default::default()
    }
}

fn build(fx: &AssetTestFixture, patch: &CollectionPatch) -> Collection {
    let values = CollectionValues::from_patch("steps", patch).unwrap();
    build_collection(values, &context(fx)).unwrap()
}

#[test]
fn test_build_rejects_bus_zero_even_without_a_validator() {
    let fx = fixture("svc_col_zero");
    let values = CollectionValues::from_patch("steps", &CollectionPatch::default()).unwrap();
    let ctx = ProjectContext::new(fx.project_root().to_path_buf());
    let err = build_collection(values, &ctx).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_EMPTY_REFERENCE);
    assert_eq!(err.field.as_deref(), Some("bus"));
}

#[test]
fn test_build_rejects_dangling_effect() {
    let fx = fixture("svc_col_effect");
    let patch = CollectionPatch {
        effect: Some(999),
        ..patch_with_bus(2)
    };
    let values = CollectionValues::from_patch("steps", &patch).unwrap();
    let err = build_collection(values, &context(&fx)).unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
}

#[test]
fn test_build_random_entries_carry_weight_and_scheduler_config() {
    let fx = fixture("svc_col_random");
    let patch = CollectionPatch {
        avoid_repeat: Some(true),
        repeat_count: Some(2),
        entries: EntriesPatch {
            replace: Some(vec![
                EntrySpec {
                    weight: Some(3.0),
                    gain: Some(RtpcCompatibleValue::static_value(0.5)),
                    ..spec(101)
                },
                spec(102),
            ]),
            ..Default::default()
        },
        ..patch_with_bus(2)
    };
    let collection = build(&fx, &patch);

    let scheduler = collection.scheduler.as_ref().unwrap();
    assert_eq!(scheduler.mode, SoundSchedulerMode::Random);
    assert_eq!(
        scheduler.config,
        Some(SoundSchedulerConfig::Random(RandomSoundSchedulerConfig {
            avoid_repeat: true,
            repeat_count: 2,
        }))
    );
    let sounds = collection.sounds.as_ref().unwrap();
    let CollectionEntry::Random(first) = &sounds[0] else {
        panic!("random collections hold Random entries");
    };
    assert_eq!(first.sound, 101);
    assert_eq!(first.weight, 3.0);
    assert_eq!(first.gain.as_ref().and_then(|g| g.as_static()), Some(0.5));
    let CollectionEntry::Random(second) = &sounds[1] else {
        panic!("random collections hold Random entries");
    };
    assert_eq!(second.weight, 1.0, "an unset weight is the SDK default");
}

#[test]
fn test_build_without_config_flags_leaves_the_scheduler_config_unset() {
    let fx = fixture("svc_col_no_config");
    let collection = build(&fx, &patch_with_bus(2));
    assert_eq!(collection.scheduler.unwrap().config, None);
}

#[test]
fn test_build_sequence_entries_and_end_behavior() {
    let fx = fixture("svc_col_seq");
    let patch = CollectionPatch {
        scheduler_mode: Some(SoundSchedulerMode::Sequence),
        end_behavior: Some(SequenceSoundSchedulerEndBehavior::Reverse),
        entries: EntriesPatch {
            replace: Some(vec![spec(101), spec(102)]),
            ..Default::default()
        },
        ..patch_with_bus(2)
    };
    let collection = build(&fx, &patch);
    assert_eq!(
        collection.scheduler.unwrap().config,
        Some(SoundSchedulerConfig::Sequence(
            SequenceSoundSchedulerConfig {
                end_behavior: SequenceSoundSchedulerEndBehavior::Reverse,
            }
        ))
    );
    assert!(
        collection
            .sounds
            .unwrap()
            .iter()
            .all(|e| matches!(e, CollectionEntry::Sequence(_)))
    );
}

#[test]
fn test_weight_is_rejected_for_sequence_collections() {
    let patch = CollectionPatch {
        scheduler_mode: Some(SoundSchedulerMode::Sequence),
        entries: EntriesPatch {
            replace: Some(vec![EntrySpec {
                weight: Some(2.0),
                ..spec(101)
            }]),
            ..Default::default()
        },
        ..patch_with_bus(2)
    };
    let err = CollectionValues::from_patch("steps", &patch).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("sounds.weight"));
}

#[test]
fn test_config_flag_of_the_other_scheduler_is_rejected() {
    let patch = CollectionPatch {
        scheduler_mode: Some(SoundSchedulerMode::Sequence),
        avoid_repeat: Some(true),
        ..patch_with_bus(2)
    };
    let err = CollectionValues::from_patch("steps", &patch).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("scheduler"));
}

#[test]
fn test_dangling_and_zero_entry_sounds_are_rejected() {
    let fx = fixture("svc_col_entry_refs");
    for (id, code) in [
        (999, codes::ERR_VALIDATION_REFERENCE),
        (0, codes::ERR_VALIDATION_EMPTY_REFERENCE),
    ] {
        let patch = CollectionPatch {
            entries: EntriesPatch {
                replace: Some(vec![spec(id)]),
                ..Default::default()
            },
            ..patch_with_bus(2)
        };
        let values = CollectionValues::from_patch("steps", &patch).unwrap();
        let err = build_collection(values, &context(&fx)).unwrap_err();
        assert_eq!(err.code(), code, "entry sound {id}");
        assert_eq!(err.field.as_deref(), Some("sounds[0].sound"));
    }
}

#[test]
fn test_negative_or_nan_weight_is_rejected() {
    let fx = fixture("svc_col_weight");
    for weight in [-1.0, f32::NAN] {
        let patch = CollectionPatch {
            entries: EntriesPatch {
                replace: Some(vec![EntrySpec {
                    weight: Some(weight),
                    ..spec(101)
                }]),
                ..Default::default()
            },
            ..patch_with_bus(2)
        };
        let values = CollectionValues::from_patch("steps", &patch).unwrap();
        let err = build_collection(values, &context(&fx)).unwrap_err();
        assert_eq!(err.field.as_deref(), Some("sounds[0].weight"), "{weight}");
    }
}

fn rich_collection(fx: &AssetTestFixture) -> Collection {
    build(
        fx,
        &CollectionPatch {
            avoid_repeat: Some(true),
            repeat_count: Some(2),
            entries: EntriesPatch {
                replace: Some(vec![
                    EntrySpec {
                        weight: Some(3.0),
                        ..spec(101)
                    },
                    spec(102),
                ]),
                ..Default::default()
            },
            ..patch_with_bus(2)
        },
    )
}

#[test]
fn test_apply_patch_keeps_scheduler_config_and_entries() {
    let fx = fixture("svc_col_keep");
    let existing = rich_collection(&fx);
    let patch = CollectionPatch {
        gain: Some(RtpcCompatibleValue::static_value(0.4)),
        ..Default::default()
    };
    let updated = apply_patch(&existing, &patch).unwrap();

    let mut expected = existing.clone();
    expected.gain = Some(RtpcCompatibleValue::static_value(0.4));
    assert_eq!(updated, expected);
    assert_eq!(updated.id(), existing.id());
}

#[test]
fn test_apply_patch_merges_one_config_flag_into_the_existing_config() {
    let fx = fixture("svc_col_merge_config");
    let existing = rich_collection(&fx);
    let patch = CollectionPatch {
        repeat_count: Some(5),
        ..Default::default()
    };
    let updated = apply_patch(&existing, &patch).unwrap();
    assert_eq!(
        updated.scheduler.unwrap().config,
        Some(SoundSchedulerConfig::Random(RandomSoundSchedulerConfig {
            avoid_repeat: true,
            repeat_count: 5,
        }))
    );
}

#[test]
fn test_apply_patch_mode_change_drops_config_and_retags_entries() {
    let fx = fixture("svc_col_mode");
    let existing = rich_collection(&fx);
    let patch = CollectionPatch {
        scheduler_mode: Some(SoundSchedulerMode::Sequence),
        ..Default::default()
    };
    let updated = apply_patch(&existing, &patch).unwrap();
    let scheduler = updated.scheduler.unwrap();
    assert_eq!(scheduler.mode, SoundSchedulerMode::Sequence);
    assert_eq!(scheduler.config, None);
    let sounds = updated.sounds.unwrap();
    assert_eq!(sounds.len(), 2);
    assert!(matches!(&sounds[0], CollectionEntry::Sequence(e) if e.sound == 101));
    assert!(matches!(&sounds[1], CollectionEntry::Sequence(e) if e.sound == 102));
}

#[test]
fn test_apply_patch_mode_change_with_new_config() {
    let fx = fixture("svc_col_mode_cfg");
    let existing = rich_collection(&fx);
    let patch = CollectionPatch {
        scheduler_mode: Some(SoundSchedulerMode::Sequence),
        end_behavior: Some(SequenceSoundSchedulerEndBehavior::Reverse),
        ..Default::default()
    };
    let updated = apply_patch(&existing, &patch).unwrap();
    assert_eq!(
        updated.scheduler.unwrap().config,
        Some(SoundSchedulerConfig::Sequence(
            SequenceSoundSchedulerConfig {
                end_behavior: SequenceSoundSchedulerEndBehavior::Reverse,
            }
        ))
    );
}

#[test]
fn test_apply_patch_entry_add_remove_and_clear() {
    let fx = fixture("svc_col_entries");
    let existing = build(&fx, &patch_with_bus(2));

    let added = apply_patch(
        &existing,
        &CollectionPatch {
            entries: EntriesPatch {
                add: vec![spec(101), spec(102)],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(added.sounds.as_ref().unwrap().len(), 2);

    let removed = apply_patch(
        &added,
        &CollectionPatch {
            entries: EntriesPatch {
                remove: vec![101],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    let sounds = removed.sounds.as_ref().unwrap();
    assert_eq!(sounds.len(), 1);
    assert!(matches!(&sounds[0], CollectionEntry::Random(e) if e.sound == 102));

    let cleared = apply_patch(
        &removed,
        &CollectionPatch {
            entries: EntriesPatch {
                replace: Some(Vec::new()),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert!(cleared.sounds.unwrap().is_empty());
}

#[test]
fn test_apply_patch_removing_an_absent_entry_is_an_error() {
    let fx = fixture("svc_col_remove_absent");
    let existing = build(&fx, &patch_with_bus(2));
    let err = apply_patch(
        &existing,
        &CollectionPatch {
            entries: EntriesPatch {
                remove: vec![101],
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_FIELD);
}

#[test]
fn test_apply_patch_does_not_mutate_its_input() {
    let fx = fixture("svc_col_immutable");
    let existing = rich_collection(&fx);
    let snapshot = existing.clone();
    let _ = apply_patch(
        &existing,
        &CollectionPatch {
            scheduler_mode: Some(SoundSchedulerMode::Sequence),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(existing, snapshot);
}

#[test]
fn test_changed_fields_lists_every_set_field() {
    let patch = CollectionPatch {
        bus: Some(2),
        avoid_repeat: Some(true),
        entries: EntriesPatch {
            add: vec![spec(101)],
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        patch.changed_fields(),
        vec!["bus", "avoid_repeat", "entries"]
    );
    assert!(CollectionPatch::default().is_empty());
}
