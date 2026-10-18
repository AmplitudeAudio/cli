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

//! Tests for the pure soundbank service: name resolution, building and patching.

use am::assets::validator::SourceAsset;
use am::assets::{ProjectContext, Soundbank, SoundbankList};
use am::common::errors::codes;
use am::services::soundbank::{
    Catalog, ListPatch, SoundbankPatch, SoundbankValues, apply_patch, build_soundbank,
};

fn source(name: &str, relative: &str) -> SourceAsset {
    SourceAsset {
        name: name.to_string(),
        relative: relative.to_string(),
    }
}

fn catalog() -> Catalog {
    Catalog::new([
        (
            SoundbankList::Sounds,
            vec![
                source("boom", "boom.json"),
                source("step", "fx/step.wav.json"),
            ],
        ),
        (
            SoundbankList::Collections,
            vec![source("crowd", "crowd.json")],
        ),
        (SoundbankList::Effects, vec![source("echo", "echo.json")]),
    ])
}

fn add(items: &[&str]) -> ListPatch {
    ListPatch {
        add: items.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn test_names_resolve_to_runtime_names() {
    let c = catalog();
    assert_eq!(
        c.resolve(SoundbankList::Sounds, "boom").unwrap(),
        "boom.amsound"
    );
    // nested sources keep their directory, and a dotted stem keeps its dots
    assert_eq!(
        c.resolve(SoundbankList::Sounds, "step").unwrap(),
        "fx/step.wav.amsound"
    );
    assert_eq!(
        c.resolve(SoundbankList::Effects, "echo").unwrap(),
        "echo.amfx"
    );
}

#[test]
fn test_paths_and_runtime_names_resolve_too() {
    let c = catalog();
    assert_eq!(
        c.resolve(SoundbankList::Sounds, "fx/step.wav").unwrap(),
        "fx/step.wav.amsound"
    );
    assert_eq!(
        c.resolve(SoundbankList::Sounds, "boom.amsound").unwrap(),
        "boom.amsound"
    );
    // a runtime name of another type is not a sound
    assert!(
        c.resolve(SoundbankList::Sounds, "crowd.amcollection")
            .is_err()
    );
}

#[test]
fn test_unknown_name_lists_the_available_names() {
    let err = catalog()
        .resolve(SoundbankList::Sounds, "ghost")
        .unwrap_err();
    assert_eq!(err.code(), codes::ERR_VALIDATION_REFERENCE);
    let text = format!("{err:?}");
    assert!(
        text.contains("ghost") && text.contains("boom") && text.contains("step"),
        "{text}"
    );
    assert_eq!(err.field.as_deref(), Some("sounds"));
}

#[test]
fn test_unknown_name_in_an_empty_type_says_there_are_none() {
    let err = catalog().resolve(SoundbankList::Rtpc, "wind").unwrap_err();
    assert!(format!("{err:?}").contains("no rtpc yet"));
}

#[test]
fn test_duplicate_names_are_ambiguous() {
    let c = Catalog::new([(
        SoundbankList::Sounds,
        vec![source("hit", "a/hit.json"), source("hit", "b/hit.json")],
    )]);
    let err = c.resolve(SoundbankList::Sounds, "hit").unwrap_err();
    let text = format!("{err:?}");
    assert!(text.contains("a/hit") && text.contains("b/hit"), "{text}");
    assert_eq!(
        c.resolve(SoundbankList::Sounds, "a/hit").unwrap(),
        "a/hit.amsound"
    );
}

#[test]
fn test_from_patch_without_assets_builds_an_empty_bank() {
    let values = SoundbankValues::from_patch("b", &SoundbankPatch::default(), &catalog()).unwrap();
    let bank = build_soundbank(values, &ProjectContext::empty()).unwrap();
    for list in SoundbankList::ALL {
        assert_eq!(
            bank.list_option(list),
            &Some(Vec::new()),
            "{}",
            list.field()
        );
    }
}

#[test]
fn test_build_writes_resolved_names_and_every_list() {
    let patch = SoundbankPatch::default()
        .with_list(SoundbankList::Sounds, add(&["boom", "step", "boom"]))
        .with_list(SoundbankList::Effects, add(&["echo"]));
    let values = SoundbankValues::from_patch("b", &patch, &catalog()).unwrap();
    let bank = build_soundbank(values, &ProjectContext::empty()).unwrap();
    assert_eq!(
        bank.sounds.as_deref().unwrap(),
        ["boom.amsound", "fx/step.wav.amsound"]
    );
    assert_eq!(bank.effects.as_deref().unwrap(), ["echo.amfx"]);
    assert_eq!(bank.rtpc, Some(Vec::new()));
    assert_ne!(bank.id, 0);
}

fn bank() -> Soundbank {
    Soundbank::builder(7, "b")
        .sound("boom.amsound")
        .sound("fx/step.wav.amsound")
        .collection("crowd.amcollection")
        .build()
}

#[test]
fn test_apply_patch_leaves_untouched_lists_alone() {
    let existing = bank();
    let patch = SoundbankPatch::default().with_list(SoundbankList::Effects, add(&["echo"]));
    let updated = apply_patch(&existing, &patch, &catalog()).unwrap();
    assert_eq!(updated.sounds, existing.sounds);
    assert_eq!(updated.collections, existing.collections);
    assert_eq!(updated.effects.as_deref().unwrap(), ["echo.amfx"]);
    assert_eq!(updated.id, 7);
    // the input is not modified
    assert_eq!(existing.effects, Some(Vec::new()));
}

#[test]
fn test_apply_patch_orders_replace_remove_add() {
    let change = ListPatch {
        replace: Some(vec!["boom".into(), "step".into()]),
        remove: vec!["boom".into()],
        add: vec!["boom".into()],
    };
    let patch = SoundbankPatch::default().with_list(SoundbankList::Sounds, change);
    let updated = apply_patch(&bank(), &patch, &catalog()).unwrap();
    assert_eq!(
        updated.sounds.as_deref().unwrap(),
        ["fx/step.wav.amsound", "boom.amsound"]
    );
}

#[test]
fn test_remove_accepts_names_and_entries_as_written() {
    let patch = SoundbankPatch::default().with_list(
        SoundbankList::Sounds,
        ListPatch {
            remove: vec!["boom".into(), "fx/step.wav.amsound".into()],
            ..Default::default()
        },
    );
    let updated = apply_patch(&bank(), &patch, &catalog()).unwrap();
    assert!(updated.sounds.unwrap().is_empty());
}

#[test]
fn test_remove_drops_a_stale_entry_whose_source_is_gone() {
    let stale = bank().with_list(SoundbankList::Sounds, vec!["gone.amsound".into()]);
    let patch = SoundbankPatch::default().with_list(
        SoundbankList::Sounds,
        ListPatch {
            remove: vec!["gone.amsound".into()],
            ..Default::default()
        },
    );
    let updated = apply_patch(&stale, &patch, &catalog()).unwrap();
    assert!(updated.sounds.unwrap().is_empty());
}

#[test]
fn test_removing_what_the_bank_does_not_hold_lists_the_entries() {
    let patch = SoundbankPatch::default().with_list(
        SoundbankList::Collections,
        ListPatch {
            remove: vec!["nothing".into()],
            ..Default::default()
        },
    );
    let err = apply_patch(&bank(), &patch, &catalog()).unwrap_err();
    let text = format!("{err:?}");
    assert!(
        text.contains("nothing") && text.contains("crowd.amcollection"),
        "{text}"
    );
}

#[test]
fn test_clear_empties_a_list_and_absent_lists_are_repaired() {
    let mut legacy = bank();
    legacy.rtpc = None;
    let patch = SoundbankPatch::default().with_list(
        SoundbankList::Collections,
        ListPatch {
            replace: Some(Vec::new()),
            ..Default::default()
        },
    );
    let updated = apply_patch(&legacy, &patch, &catalog()).unwrap();
    assert_eq!(updated.collections, Some(Vec::new()));
    assert_eq!(updated.rtpc, Some(Vec::new()));
}

#[test]
fn test_changed_fields_use_schema_names() {
    let patch = SoundbankPatch::default()
        .with_list(SoundbankList::SwitchContainers, add(&["x"]))
        .with_list(SoundbankList::Sounds, add(&["y"]))
        .with_list(SoundbankList::Events, ListPatch::default());
    assert_eq!(patch.changed_fields(), vec!["sounds", "switch_containers"]);
    assert!(SoundbankPatch::default().is_empty());
}
