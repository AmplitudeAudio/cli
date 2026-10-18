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

//! Tests for `soundbank_referrers`: which soundbanks include an asset.

mod common;

use am::assets::AssetType;
use am::forms::soundbank_refs::soundbank_referrers;
use common::fixtures::AssetTestFixture;
use common::soundbank_helpers::fixture;
use serde_json::json;

fn write_bank(fx: &AssetTestFixture, name: &str, id: u64, lists: serde_json::Value) {
    let mut bank = json!({"id": id, "name": name});
    for (key, value) in lists.as_object().unwrap() {
        bank[key] = value.clone();
    }
    fx.write_asset_json("soundbanks", name, bank).unwrap();
}

fn names(fx: &AssetTestFixture, asset_type: AssetType, name: &str) -> Vec<String> {
    soundbank_referrers(&fx.create_project_validator().unwrap(), asset_type, name)
        .into_iter()
        .map(|r| r.name)
        .collect()
}

#[test]
fn test_lists_every_bank_that_includes_the_asset() {
    let fx = fixture("sbref_basic");
    write_bank(&fx, "a", 1, json!({"sounds": ["boom.amsound"]}));
    write_bank(
        &fx,
        "b",
        2,
        json!({"sounds": ["boom.amsound", "fx/step.wav.amsound"]}),
    );
    write_bank(&fx, "c", 3, json!({"sounds": ["fx/step.wav.amsound"]}));
    assert_eq!(names(&fx, AssetType::Sound, "boom"), ["a", "b"]);
    assert_eq!(names(&fx, AssetType::Sound, "step"), ["b", "c"]);
}

#[test]
fn test_every_asset_type_a_bank_can_hold_is_found() {
    let fx = fixture("sbref_types");
    write_bank(
        &fx,
        "all",
        1,
        json!({
            "sounds": ["boom.amsound"], "collections": ["crowd.amcollection"],
            "events": ["hit.amevent"], "switches": ["surface.amswitch"],
            "switch_containers": ["footsteps.amswitchcontainer"], "effects": ["echo.amfx"],
            "attenuators": ["room.amattenuation"], "rtpc": ["wind.amrtpc"]
        }),
    );
    for (asset_type, name) in [
        (AssetType::Sound, "boom"),
        (AssetType::Collection, "crowd"),
        (AssetType::Event, "hit"),
        (AssetType::Switch, "surface"),
        (AssetType::SwitchContainer, "footsteps"),
        (AssetType::Effect, "echo"),
        (AssetType::Attenuation, "room"),
        (AssetType::Rtpc, "wind"),
    ] {
        assert_eq!(
            names(&fx, asset_type, name),
            ["all"],
            "{asset_type:?} {name}"
        );
    }
}

#[test]
fn test_the_list_must_match_the_asset_type() {
    let fx = fixture("sbref_wrong_list");
    // a sound's runtime name in the collections list does not include the sound
    write_bank(&fx, "a", 1, json!({"collections": ["boom.amsound"]}));
    assert!(names(&fx, AssetType::Sound, "boom").is_empty());
    assert!(names(&fx, AssetType::Collection, "boom").is_empty());
}

#[test]
fn test_effects_are_found_under_either_extension() {
    let fx = fixture("sbref_effects");
    write_bank(&fx, "new", 1, json!({"effects": ["echo.amfx"]}));
    write_bank(&fx, "built", 2, json!({"effects": ["echo.amenv"]}));
    assert_eq!(names(&fx, AssetType::Effect, "echo"), ["built", "new"]);
}

#[test]
fn test_unknown_assets_and_unholdable_types_have_no_referrers() {
    let fx = fixture("sbref_none");
    write_bank(&fx, "a", 1, json!({"sounds": ["boom.amsound"]}));
    assert!(names(&fx, AssetType::Sound, "ghost").is_empty());
    assert!(names(&fx, AssetType::Soundbank, "a").is_empty());
    assert!(names(&fx, AssetType::Pipeline, "boom").is_empty());
}

#[test]
fn test_banks_in_subdirectories_and_without_lists_are_handled() {
    let fx = fixture("sbref_nested");
    std::fs::create_dir_all(fx.sources_dir().join("soundbanks/level1")).unwrap();
    std::fs::write(
        fx.sources_dir().join("soundbanks/level1/deep.json"),
        json!({"id": 4, "name": "deep", "sounds": ["boom.amsound"]}).to_string(),
    )
    .unwrap();
    write_bank(&fx, "bare", 5, json!({}));
    std::fs::write(fx.sources_dir().join("soundbanks/broken.json"), "{").unwrap();
    std::fs::write(
        fx.sources_dir().join("soundbanks/not_utf8.json"),
        [0xff, 0xfe, 0x00],
    )
    .unwrap();
    assert_eq!(names(&fx, AssetType::Sound, "boom"), ["deep"]);
}
