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

//! Runtime names and SDK-derived rules of `Soundbank`.

mod common;

use am::assets::{Asset, AssetType, ProjectContext, Soundbank, SoundbankList};
use common::fixtures::AssetTestFixture;
use serde_json::json;

fn fixture_with_assets() -> AssetTestFixture {
    let fx = AssetTestFixture::new("sb_rules").unwrap();
    fx.create_test_sound("boom", 11).unwrap();
    std::fs::create_dir_all(fx.sources_dir().join("sounds/fx")).unwrap();
    std::fs::write(
        fx.sources_dir().join("sounds/fx/step.wav.json"),
        json!({"id": 12, "name": "step"}).to_string(),
    )
    .unwrap();
    fx.create_test_collection("crowd", 21, &[]).unwrap();
    fx.write_asset_json("effects", "echo", json!({"id": 31, "name": "echo"}))
        .unwrap();
    fx
}

fn context(fx: &AssetTestFixture) -> ProjectContext {
    ProjectContext::new(fx.project_root().to_path_buf())
        .with_validator(fx.create_project_validator().unwrap())
}

#[test]
fn test_runtime_names_round_trip_for_every_list() {
    for list in SoundbankList::ALL {
        let runtime = list.runtime_name("fx/boom.json");
        assert_eq!(runtime, format!("fx/boom{}", list.extension()));
        assert_eq!(
            list.source_relative(&runtime).as_deref(),
            Some("fx/boom.json")
        );
    }
    // dots in the source stem survive: only the trailing `.json` is replaced
    assert_eq!(
        SoundbankList::Sounds.runtime_name("throw_01.wav.json"),
        "throw_01.wav.amsound"
    );
    assert_eq!(
        SoundbankList::Sounds
            .source_relative("throw_01.wav.amsound")
            .as_deref(),
        Some("throw_01.wav.json")
    );
}

#[test]
fn test_runtime_names_use_the_sdk_extensions() {
    let expected = [
        (SoundbankList::Sounds, ".amsound"),
        (SoundbankList::Collections, ".amcollection"),
        (SoundbankList::Events, ".amevent"),
        (SoundbankList::Switches, ".amswitch"),
        (SoundbankList::SwitchContainers, ".amswitchcontainer"),
        (SoundbankList::Effects, ".amfx"),
        (SoundbankList::Attenuators, ".amattenuation"),
        (SoundbankList::Rtpc, ".amrtpc"),
    ];
    for (list, ext) in expected {
        assert_eq!(list.extension(), ext);
    }
    // an entry in the wrong list's extension is not a source of that list
    assert_eq!(
        SoundbankList::Collections.source_relative("boom.amsound"),
        None
    );
    assert_eq!(SoundbankList::Sounds.source_relative("boom.json"), None);
    assert_eq!(
        SoundbankList::Effects
            .source_relative("echo.amenv")
            .as_deref(),
        Some("echo.json")
    );
}

#[test]
fn test_builder_writes_every_list_because_the_sdk_dereferences_them() {
    let bank = Soundbank::builder(1, "b").sound("boom.amsound").build();
    let value = serde_json::to_value(&bank).unwrap();
    for list in SoundbankList::ALL {
        assert!(
            value.get(list.field()).is_some_and(|v| v.is_array()),
            "{}",
            list.field()
        );
    }
}

#[test]
fn test_a_missing_list_is_an_error_naming_the_list() {
    let mut bank = Soundbank::builder(1, "b").sound("boom.amsound").build();
    bank.rtpc = None;
    let err = bank.validate_rules(&ProjectContext::empty()).unwrap_err();
    assert_eq!(err.field.as_deref(), Some("rtpc"));
    assert!(err.what().contains("rtpc"), "{}", err.what());
}

#[test]
fn test_validation_accepts_runtime_names_including_subdirectories() {
    let fx = fixture_with_assets();
    let bank = Soundbank::builder(1, "b")
        .sound("boom.amsound")
        .sound("fx/step.wav.amsound")
        .collection("crowd.amcollection")
        .effect("echo.amfx")
        .build();
    bank.validate_rules(&context(&fx)).unwrap();
    let legacy = Soundbank::builder(1, "b").effect("echo.amenv").build();
    legacy.validate_rules(&context(&fx)).unwrap();
}

#[test]
fn test_validation_rejects_names_without_the_runtime_extension() {
    let fx = fixture_with_assets();
    for entry in ["boom.json", "boom", "crowd.amcollection"] {
        let bank = Soundbank::builder(1, "b").sound(entry).build();
        let err = bank.validate_rules(&context(&fx)).unwrap_err();
        let text = format!("{err:?}");
        assert!(text.contains(".amsound"), "{entry}: {text}");
        assert_eq!(err.field.as_deref(), Some("sounds"), "{entry}");
    }
}

#[test]
fn test_validation_rejects_missing_sources_listing_the_entry() {
    let fx = fixture_with_assets();
    let bank = Soundbank::builder(1, "b").sound("ghost.amsound").build();
    let err = bank.validate_rules(&context(&fx)).unwrap_err();
    assert_eq!(
        err.code(),
        am::common::errors::codes::ERR_VALIDATION_REFERENCE
    );
    let text = format!("{err:?}");
    assert!(text.contains("ghost.amsound"), "{text}");
}

#[test]
fn test_validator_lists_source_assets_with_their_files() {
    let fx = fixture_with_assets();
    let validator = fx.create_project_validator().unwrap();
    let sounds = validator.source_assets(AssetType::Sound);
    let pairs: Vec<(&str, &str)> = sounds
        .iter()
        .map(|s| (s.name.as_str(), s.relative.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![("boom", "boom.json"), ("step", "fx/step.wav.json")]
    );
}
