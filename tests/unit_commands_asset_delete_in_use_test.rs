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

//! Deleting an asset that others still reference is refused (`ERR_ASSET_IN_USE`).
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::{Path, PathBuf};

use am::app::{App, Commands};
use am::commands::asset::handler;
use am::common::errors::{as_cli_error, codes};
use am::input::NonInteractiveInput;
use clap::Parser;
use common::fixtures::{AssetTestFixture, CaptureOutput};
use serde_json::{Value, json};

struct CwdGuard(Option<PathBuf>);

impl CwdGuard {
    fn enter(path: &Path) -> Self {
        let previous = std::env::current_dir().ok();
        std::env::set_current_dir(path).unwrap();
        Self(previous)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        if let Some(dir) = self.0.take() {
            let _ = std::env::set_current_dir(dir);
        }
    }
}

fn fixture(name: &str) -> AssetTestFixture {
    AssetTestFixture::new(name).unwrap()
}

/// Runs `am asset <args...>` in JSON mode (no prompts possible).
async fn run(fx: &AssetTestFixture, args: &[&str]) -> (anyhow::Result<()>, CaptureOutput) {
    let mut full = vec!["am", "asset"];
    full.extend_from_slice(args);
    let Commands::Asset { command } = App::try_parse_from(full).expect("should parse").command
    else {
        panic!("expected an asset command");
    };
    let _cwd = CwdGuard::enter(fx.project_root());
    let output = CaptureOutput::json();
    let result = handler(&command, None, &NonInteractiveInput, &output).await;
    (result, output)
}

/// Writes an asset file, filling in the fields its type requires to be parsed.
fn put(fx: &AssetTestFixture, dir: &str, name: &str, value: Value) -> PathBuf {
    let defaults = match dir {
        "collections" => json!({
            "attenuation": 0, "effect": 0, "gain": {"kind": "Static", "value": 1.0},
            "priority": {"kind": "Static", "value": 128.0}, "fader": "Linear",
            "spatialization": "None", "scope": "World", "play_mode": "PlayOne",
            "scheduler": {"mode": "Random"}
        }),
        "switch_containers" => json!({
            "effect": 0, "attenuation": 0, "scope": "World", "spatialization": "None",
            "update_behavior": "UpdateOnPlay", "default_switch_state": 0, "entries": []
        }),
        "events" => json!({"run_mode": "Parallel", "actions": []}),
        _ => json!({}),
    };
    let mut merged = defaults;
    merged
        .as_object_mut()
        .unwrap()
        .extend(value.as_object().unwrap().clone());
    fx.write_asset_json(dir, name, merged).unwrap()
}

fn bank(fx: &AssetTestFixture, name: &str, list: &str, entry: &str) {
    put(
        fx,
        "soundbanks",
        name,
        json!({"id": 7000, "name": name, list: [entry]}),
    );
}

/// The delete command line of a type (each type asks for confirmation its own way).
fn delete_args(kind: &'static str, name: &'static str) -> Vec<&'static str> {
    match kind {
        "event" => vec![kind, "delete", name, "--yes", "--force"],
        "soundbank" => vec![kind, "delete", name, "--yes"],
        _ => vec![kind, "delete", name, "--force"],
    }
}

/// Asserts the delete is refused, names `referrer`, and leaves `target` on disk.
async fn assert_refused(
    fx: &AssetTestFixture,
    kind: &'static str,
    name: &'static str,
    target: &Path,
    referrer: &str,
) {
    let (result, output) = run(fx, &delete_args(kind, name)).await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_ASSET_IN_USE,
        "{err:#}"
    );
    let text = format!("{err:#}");
    assert!(text.contains(referrer), "should name '{referrer}': {text}");
    assert!(target.exists(), "refused delete must keep the file");
    assert!(output.all_successes().is_empty());
}

async fn assert_deleted(fx: &AssetTestFixture, kind: &'static str, name: &'static str, p: &Path) {
    let (result, output) = run(fx, &delete_args(kind, name)).await;
    result.unwrap();
    assert!(!p.exists());
    assert_eq!(output.last_success().unwrap()["deleted"], true);
}

// -- collection ---------------------------------------------------------------

#[tokio::test]
async fn test_collection_refused_when_an_event_plays_it() {
    let fx = fixture("del_col_event");
    let p = put(
        &fx,
        "collections",
        "crowd",
        json!({"id": 100, "name": "crowd", "bus": 2}),
    );
    put(
        &fx,
        "events",
        "start",
        json!({"id": 200, "name": "start", "actions": [{"type": "Play", "targets": [100]}]}),
    );
    assert_refused(&fx, "collection", "crowd", &p, "start").await;
}

#[tokio::test]
async fn test_collection_refused_when_a_switch_container_plays_it() {
    let fx = fixture("del_col_sc");
    let p = put(
        &fx,
        "collections",
        "crowd",
        json!({"id": 100, "name": "crowd", "bus": 2}),
    );
    put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 10, "entries": [{"object": 100, "switch_states": [11]}]}),
    );
    assert_refused(&fx, "collection", "crowd", &p, "steps").await;
}

#[tokio::test]
async fn test_collection_refused_when_a_soundbank_includes_it() {
    let fx = fixture("del_col_bank");
    let p = put(
        &fx,
        "collections",
        "crowd",
        json!({"id": 100, "name": "crowd", "bus": 2}),
    );
    bank(&fx, "main", "collections", "crowd.amcollection");
    assert_refused(&fx, "collection", "crowd", &p, "main").await;
}

#[tokio::test]
async fn test_collection_deletes_when_unreferenced() {
    let fx = fixture("del_col_free");
    let p = put(
        &fx,
        "collections",
        "crowd",
        json!({"id": 100, "name": "crowd", "bus": 2}),
    );
    put(
        &fx,
        "events",
        "start",
        json!({"id": 200, "name": "start", "actions": [{"type": "Play", "targets": [999]}]}),
    );
    assert_deleted(&fx, "collection", "crowd", &p).await;
}

// -- event --------------------------------------------------------------------

#[tokio::test]
async fn test_event_refused_when_a_soundbank_includes_it() {
    let fx = fixture("del_event_bank");
    let p = put(&fx, "events", "start", json!({"id": 200, "name": "start"}));
    bank(&fx, "main", "events", "start.amevent");
    assert_refused(&fx, "event", "start", &p, "main").await;
}

#[tokio::test]
async fn test_event_deletes_even_when_other_soundbanks_exist() {
    let fx = fixture("del_event_free");
    let p = put(&fx, "events", "start", json!({"id": 200, "name": "start"}));
    bank(&fx, "main", "events", "other.amevent");
    assert_deleted(&fx, "event", "start", &p).await;
}

// -- effect -------------------------------------------------------------------

#[tokio::test]
async fn test_effect_refused_when_sound_collection_or_container_uses_it() {
    for (dir, extra) in [
        ("sounds", json!({})),
        ("collections", json!({})),
        ("switch_containers", json!({"switch_group": 10})),
    ] {
        let fx = fixture("del_effect_users");
        let p = put(&fx, "effects", "echo", json!({"id": 66, "name": "echo"}));
        let mut user = json!({"id": 400, "name": "user", "bus": 2, "effect": 66});
        user.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        put(&fx, dir, "user", user);
        assert_refused(&fx, "effect", "echo", &p, "user").await;
    }
}

#[tokio::test]
async fn test_effect_refused_when_a_soundbank_includes_it() {
    let fx = fixture("del_effect_bank");
    let p = put(&fx, "effects", "echo", json!({"id": 66, "name": "echo"}));
    bank(&fx, "main", "effects", "echo.amfx");
    assert_refused(&fx, "effect", "echo", &p, "main").await;
}

#[tokio::test]
async fn test_effect_deletes_when_unreferenced() {
    let fx = fixture("del_effect_free");
    let p = put(&fx, "effects", "echo", json!({"id": 66, "name": "echo"}));
    put(
        &fx,
        "sounds",
        "boom",
        json!({"id": 400, "name": "boom", "bus": 2, "effect": 12}),
    );
    assert_deleted(&fx, "effect", "echo", &p).await;
}

// -- switch -------------------------------------------------------------------

#[tokio::test]
async fn test_switch_refused_when_a_container_uses_it() {
    let fx = fixture("del_switch_sc");
    let p = put(
        &fx,
        "switches",
        "surface",
        json!({"id": 10, "name": "surface", "states": [{"id": 11, "name": "grass"}]}),
    );
    put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 10}),
    );
    assert_refused(&fx, "switch", "surface", &p, "steps").await;
}

#[tokio::test]
async fn test_switch_refused_when_a_soundbank_includes_it() {
    let fx = fixture("del_switch_bank");
    let p = put(
        &fx,
        "switches",
        "surface",
        json!({"id": 10, "name": "surface"}),
    );
    bank(&fx, "main", "switches", "surface.amswitch");
    assert_refused(&fx, "switch", "surface", &p, "main").await;
}

#[tokio::test]
async fn test_switch_deletes_when_unreferenced() {
    let fx = fixture("del_switch_free");
    let p = put(
        &fx,
        "switches",
        "surface",
        json!({"id": 10, "name": "surface"}),
    );
    put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 99}),
    );
    assert_deleted(&fx, "switch", "surface", &p).await;
}

// -- switch container ---------------------------------------------------------

#[tokio::test]
async fn test_switch_container_refused_when_an_event_plays_it() {
    let fx = fixture("del_sc_event");
    let p = put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 10}),
    );
    put(
        &fx,
        "events",
        "walk",
        json!({"id": 200, "name": "walk", "actions": [{"type": "Play", "targets": [300]}]}),
    );
    assert_refused(&fx, "switch-container", "steps", &p, "walk").await;
}

#[tokio::test]
async fn test_switch_container_refused_when_a_soundbank_includes_it() {
    let fx = fixture("del_sc_bank");
    let p = put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 10}),
    );
    bank(&fx, "main", "switch_containers", "steps.amswitchcontainer");
    assert_refused(&fx, "switch-container", "steps", &p, "main").await;
}

#[tokio::test]
async fn test_switch_container_deletes_when_unreferenced() {
    let fx = fixture("del_sc_free");
    let p = put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 10}),
    );
    assert_deleted(&fx, "switch-container", "steps", &p).await;
}

// -- soundbank ----------------------------------------------------------------

#[tokio::test]
async fn test_soundbank_deletes_because_nothing_references_it() {
    let fx = fixture("del_bank_free");
    let p = put(
        &fx,
        "soundbanks",
        "main",
        json!({"id": 7000, "name": "main"}),
    );
    assert_deleted(&fx, "soundbank", "main", &p).await;
}

// -- sounds, attenuators, rtpc: referrers that matter to the other types -----

#[tokio::test]
async fn test_attenuation_and_rtpc_refused_when_a_soundbank_includes_them() {
    let fx = fixture("del_att_bank");
    let create_att = [
        "attenuation",
        "create",
        "room",
        "--max-distance",
        "50",
        "--shape",
        "sphere",
        "--inner-radius",
        "5",
        "--outer-radius",
        "10",
        "--curve-part",
        "0,1,1,0,Linear",
    ];
    run(&fx, &create_att).await.0.unwrap();
    run(
        &fx,
        &[
            "rtpc",
            "create",
            "wind",
            "--min-value",
            "0",
            "--max-value",
            "10",
        ],
    )
    .await
    .0
    .unwrap();
    let att = fx.project_root().join("sources/attenuators/room.json");
    let rtpc = fx.project_root().join("sources/rtpc/wind.json");
    bank(&fx, "main", "attenuators", "room.amattenuation");
    assert_refused(&fx, "attenuation", "room", &att, "main").await;
    put(
        &fx,
        "soundbanks",
        "main",
        json!({"id": 7000, "name": "main", "rtpc": ["wind.amrtpc"]}),
    );
    assert_refused(&fx, "rtpc", "wind", &rtpc, "main").await;
}

// -- shared behaviour ---------------------------------------------------------

#[tokio::test]
async fn test_all_referrers_are_listed_and_a_fix_is_suggested() {
    let fx = fixture("del_all_referrers");
    let p = put(
        &fx,
        "collections",
        "crowd",
        json!({"id": 100, "name": "crowd", "bus": 2}),
    );
    put(
        &fx,
        "events",
        "start",
        json!({"id": 200, "name": "start", "actions": [{"type": "Play", "targets": [100]}]}),
    );
    put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 10, "entries": [{"object": 100}]}),
    );
    bank(&fx, "main", "collections", "crowd.amcollection");

    let (result, _out) = run(&fx, &delete_args("collection", "crowd")).await;
    let err = result.unwrap_err();
    let cli = as_cli_error(&err).unwrap();
    assert_eq!(cli.code, codes::ERR_ASSET_IN_USE);
    let text = format!("{err:#}");
    for name in ["start", "steps", "main"] {
        assert!(text.contains(name), "missing {name}: {text}");
    }
    assert!(
        format!("{cli:?}").contains("--remove-collection crowd"),
        "should suggest how to drop the soundbank reference: {cli:?}"
    );
    assert!(p.exists());
}

#[tokio::test]
async fn test_malformed_referrer_files_do_not_panic() {
    let fx = fixture("del_malformed");
    let p = put(&fx, "effects", "echo", json!({"id": 66, "name": "echo"}));
    for dir in [
        "sounds",
        "collections",
        "events",
        "switch_containers",
        "soundbanks",
    ] {
        std::fs::write(fx.sources_dir().join(dir).join("broken.json"), "{").unwrap();
        std::fs::write(fx.sources_dir().join(dir).join("binary.json"), [0xff, 0xfe]).unwrap();
    }
    assert_deleted(&fx, "effect", "echo", &p).await;
}

// -- sound --------------------------------------------------------------------

fn sound(fx: &AssetTestFixture) -> PathBuf {
    put(
        fx,
        "sounds",
        "boom",
        json!({
            "id": 101, "name": "boom", "bus": 2, "path": "boom.wav",
            "gain": {"kind": "Static", "value": 1.0},
            "priority": {"kind": "Static", "value": 128.0},
            "stream": false, "loop": {"enabled": false, "loop_count": 0},
            "spatialization": "None", "attenuation": 0, "scope": "World",
            "fader": "Linear", "effect": 0
        }),
    )
}

#[tokio::test]
async fn test_sound_refused_when_a_collection_entry_uses_it() {
    let fx = fixture("del_sound_col");
    let p = sound(&fx);
    put(
        &fx,
        "collections",
        "crowd",
        json!({"id": 100, "name": "crowd", "bus": 2, "sounds": [{"sound": 101}]}),
    );
    assert_refused(&fx, "sound", "boom", &p, "crowd").await;
}

#[tokio::test]
async fn test_sound_refused_when_a_switch_container_entry_uses_it() {
    let fx = fixture("del_sound_sc");
    let p = sound(&fx);
    put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 10, "entries": [{"object": 101}]}),
    );
    assert_refused(&fx, "sound", "boom", &p, "steps").await;
}

#[tokio::test]
async fn test_sound_refused_when_an_event_targets_it() {
    let fx = fixture("del_sound_event");
    let p = sound(&fx);
    put(
        &fx,
        "events",
        "hit",
        json!({"id": 200, "name": "hit", "actions": [{"type": "Play", "targets": [101]}]}),
    );
    assert_refused(&fx, "sound", "boom", &p, "hit").await;
}

#[tokio::test]
async fn test_sound_refused_when_a_soundbank_includes_it() {
    let fx = fixture("del_sound_bank");
    let p = sound(&fx);
    bank(&fx, "main", "sounds", "boom.amsound");
    assert_refused(&fx, "sound", "boom", &p, "main").await;
}

#[tokio::test]
async fn test_sound_deletes_when_unreferenced_and_keeps_the_audio_file() {
    let fx = fixture("del_sound_free");
    let p = sound(&fx);
    let audio = fx.project_root().join("data").join("boom.wav");
    std::fs::create_dir_all(audio.parent().unwrap()).unwrap();
    std::fs::write(&audio, b"RIFF").unwrap();
    put(
        &fx,
        "collections",
        "crowd",
        json!({"id": 100, "name": "crowd", "bus": 2, "sounds": [{"sound": 999}]}),
    );
    assert_deleted(&fx, "sound", "boom", &p).await;
    assert!(audio.exists(), "the audio data file must be kept");
}

#[tokio::test]
async fn test_sound_delete_needs_force_when_not_interactive() {
    let fx = fixture("del_sound_noforce");
    let p = sound(&fx);
    let (result, output) = run(&fx, &["sound", "delete", "boom"]).await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_FIELD
    );
    assert!(p.exists());
    assert!(output.all_successes().is_empty());
}

#[tokio::test]
async fn test_sound_delete_accepts_yes_as_an_alias_for_force() {
    let fx = fixture("del_sound_yes");
    let p = sound(&fx);
    let (result, _) = run(&fx, &["sound", "delete", "boom", "--yes"]).await;
    result.unwrap();
    assert!(!p.exists());
}

#[tokio::test]
async fn test_sound_delete_of_unknown_sound_fails() {
    let fx = fixture("del_sound_unknown");
    let (result, _) = run(&fx, &["sound", "delete", "ghost", "--force"]).await;
    assert_eq!(
        as_cli_error(&result.unwrap_err()).unwrap().code,
        codes::ERR_ASSET_NOT_FOUND
    );
}

// -- the shared check, called directly ---------------------------------------

#[test]
fn test_sound_in_use_check_finds_collections_containers_events_and_banks() {
    let fx = fixture("del_sound_helper");
    put(
        &fx,
        "collections",
        "crowd",
        json!({"id": 100, "name": "crowd", "bus": 2, "sounds": [{"sound": 101}]}),
    );
    put(
        &fx,
        "switch_containers",
        "steps",
        json!({"id": 300, "name": "steps", "bus": 2, "switch_group": 10, "entries": [{"object": 101}]}),
    );
    put(
        &fx,
        "events",
        "hit",
        json!({"id": 200, "name": "hit", "actions": [{"type": "Play", "targets": [101]}]}),
    );
    bank(&fx, "main", "sounds", "boom.amsound");
    put(
        &fx,
        "sounds",
        "boom",
        json!({"id": 101, "name": "boom", "bus": 2}),
    );
    let validator = fx.create_project_validator().unwrap();
    let err =
        am::forms::in_use::assert_not_in_use(&validator, am::assets::AssetType::Sound, 101, "boom")
            .unwrap_err();
    assert_eq!(err.code, codes::ERR_ASSET_IN_USE);
    let text = format!("{err:?}");
    for name in ["crowd", "steps", "hit", "main"] {
        assert!(text.contains(name), "missing {name}: {text}");
    }
    assert!(
        am::forms::in_use::assert_not_in_use(
            &validator,
            am::assets::AssetType::Sound,
            555,
            "ghost"
        )
        .is_ok()
    );
}
