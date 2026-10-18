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

//! Tests for the full sound field surface of `sound create` / `sound update`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use std::path::Path;

use am::app::{App, Commands};
use am::assets::Sound;
use am::assets::generated::{Scope, Spatialization, ValueKind};
use am::commands::asset::{AssetCommands, SoundCommands, handle_sound_command};
use am::common::errors::{as_cli_error, codes};
use am::input::{Input, NonInteractiveInput};
use clap::Parser;
use common::fixtures::{AssetTestFixture, CaptureOutput};
use common::scripted_input::ScriptedInput;
use serde_json::json;

/// Restores the working directory when dropped.
struct CwdGuard(Option<std::path::PathBuf>);

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

fn parse(args: &[&str]) -> SoundCommands {
    let mut full = vec!["am", "asset", "sound"];
    full.extend_from_slice(args);
    match App::try_parse_from(full).expect("should parse").command {
        Commands::Asset {
            command: AssetCommands::Sound { command },
        } => command,
        _ => panic!("expected a sound command"),
    }
}

fn fixture(name: &str) -> AssetTestFixture {
    let fx = AssetTestFixture::new(name).unwrap();
    fx.create_data_file("a.wav").unwrap();
    fx.write_asset_json("attenuators", "short", json!({"id": 55, "name": "short"}))
        .unwrap();
    fx.write_asset_json("effects", "echo", json!({"id": 66, "name": "echo"}))
        .unwrap();
    fx.write_asset_json("rtpc", "speed", json!({"id": 77, "name": "speed"}))
        .unwrap();
    fx
}

fn read_sound(fx: &AssetTestFixture, name: &str) -> Sound {
    let path = fx
        .project_root()
        .join("sources/sounds")
        .join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn sound_path(fx: &AssetTestFixture, name: &str) -> std::path::PathBuf {
    fx.project_root()
        .join("sources/sounds")
        .join(format!("{name}.json"))
}

async fn run(
    fx: &AssetTestFixture,
    args: &[&str],
    input: &dyn Input,
) -> (anyhow::Result<()>, CaptureOutput) {
    let _cwd = CwdGuard::enter(fx.project_root());
    let output = CaptureOutput::interactive();
    let result = handle_sound_command(&parse(args), None, input, &output).await;
    (result, output)
}

// ---------------------------------------------------------------------------
// Clap surface
// ---------------------------------------------------------------------------

#[test]
fn test_create_parses_every_new_flag() {
    let cmd = parse(&[
        "create",
        "boom",
        "--bus",
        "sfx",
        "--attenuation",
        "short",
        "--effect",
        "echo",
        "--scope",
        "entity",
        "--fader",
        "EaseIn",
        "--pitch",
        "1.2",
        "--near-field-gain",
        "0.5",
        "--gain-rtpc",
        "g",
        "--pitch-rtpc",
        "p",
        "--priority-rtpc",
        "pr",
        "--near-field-gain-rtpc",
        "n",
        "--spread-rtpc",
        "s",
    ]);
    let SoundCommands::Create { bus, extra, .. } = cmd else {
        panic!("expected create");
    };
    assert_eq!(bus.as_deref(), Some("sfx"));
    assert_eq!(extra.attenuation.as_deref(), Some("short"));
    assert_eq!(extra.effect.as_deref(), Some("echo"));
    assert_eq!(extra.scope.as_deref(), Some("entity"));
    assert_eq!(extra.fader.as_deref(), Some("EaseIn"));
    assert_eq!(extra.pitch, Some(1.2));
    assert_eq!(extra.near_field_gain, Some(0.5));
    assert_eq!(extra.gain_rtpc.as_deref(), Some("g"));
    assert_eq!(extra.pitch_rtpc.as_deref(), Some("p"));
    assert_eq!(extra.priority_rtpc.as_deref(), Some("pr"));
    assert_eq!(extra.near_field_gain_rtpc.as_deref(), Some("n"));
    assert_eq!(extra.spread_rtpc.as_deref(), Some("s"));
}

#[test]
fn test_update_parses_every_new_flag() {
    let cmd = parse(&[
        "update",
        "boom",
        "--bus",
        "3",
        "--attenuation",
        "short",
        "--effect",
        "echo",
        "--scope",
        "world",
        "--fader",
        "Linear",
        "--pitch",
        "0.9",
        "--near-field-gain",
        "0.1",
        "--spread-rtpc",
        "s",
    ]);
    let SoundCommands::Update { bus, extra, .. } = cmd else {
        panic!("expected update");
    };
    assert_eq!(bus.as_deref(), Some("3"));
    assert_eq!(extra.attenuation.as_deref(), Some("short"));
    assert_eq!(extra.effect.as_deref(), Some("echo"));
    assert_eq!(extra.scope.as_deref(), Some("world"));
    assert_eq!(extra.fader.as_deref(), Some("Linear"));
    assert_eq!(extra.pitch, Some(0.9));
    assert_eq!(extra.near_field_gain, Some(0.1));
    assert_eq!(extra.spread_rtpc.as_deref(), Some("s"));
}

// ---------------------------------------------------------------------------
// Non-interactive create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_create_resolves_names_and_writes_every_field() {
    let fx = fixture("create_names");
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "boom",
            "--file",
            "a.wav",
            "--bus",
            "sfx",
            "--attenuation",
            "short",
            "--effect",
            "echo",
            "--pitch",
            "1.2",
            "--scope",
            "entity",
            "--fader",
            "easein",
            "--near-field-gain",
            "0.5",
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();

    let sound = read_sound(&fx, "boom");
    assert_eq!(sound.bus, 2);
    assert_eq!(sound.attenuation, 55);
    assert_eq!(sound.effect, 66);
    assert_eq!(sound.scope, Scope::Entity);
    assert_eq!(sound.fader.as_deref(), Some("EaseIn"));
    assert_eq!(sound.pitch.as_ref().and_then(|p| p.as_static()), Some(1.2));
    assert_eq!(
        sound.near_field_gain.as_ref().and_then(|p| p.as_static()),
        Some(0.5)
    );
    let raw = std::fs::read_to_string(sound_path(&fx, "boom")).unwrap();
    assert!(raw.contains("\"Entity\""), "{raw}");
}

#[tokio::test]
async fn test_create_numeric_bus_still_works() {
    let fx = fixture("create_numeric");
    let (result, _out) = run(
        &fx,
        &["create", "boom", "--file", "a.wav", "--bus", "1"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    assert_eq!(read_sound(&fx, "boom").bus, 1);
}

#[tokio::test]
async fn test_create_gain_rtpc_binds_by_name() {
    let fx = fixture("create_gain_rtpc");
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "boom",
            "--file",
            "a.wav",
            "--bus",
            "sfx",
            "--gain-rtpc",
            "speed",
        ],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap();
    let gain = read_sound(&fx, "boom").gain.unwrap();
    assert_eq!(gain.kind, ValueKind::RTPC);
    assert_eq!(gain.rtpc.unwrap().id, 77);
}

#[tokio::test]
async fn test_create_rejects_nan_pitch() {
    let fx = fixture("create_nan");
    let (result, _out) = run(
        &fx,
        &[
            "create", "boom", "--file", "a.wav", "--bus", "sfx", "--pitch", "NaN",
        ],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert!(!sound_path(&fx, "boom").exists());
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_FIELD
    );
}

#[tokio::test]
async fn test_create_rejects_infinite_near_field_gain() {
    let fx = fixture("create_inf");
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "boom",
            "--file",
            "a.wav",
            "--bus",
            "sfx",
            "--near-field-gain",
            "inf",
        ],
        &NonInteractiveInput,
    )
    .await;
    assert!(result.is_err());
    assert!(!sound_path(&fx, "boom").exists());
}

#[tokio::test]
async fn test_create_unknown_attenuation_is_a_reference_error() {
    let fx = fixture("create_unknown_att");
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "boom",
            "--file",
            "a.wav",
            "--bus",
            "sfx",
            "--attenuation",
            "nope",
        ],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    let cli = as_cli_error(&err).unwrap();
    assert_eq!(cli.code, codes::ERR_VALIDATION_REFERENCE);
    assert!(cli.suggestion.contains("short"), "{}", cli.suggestion);
    assert!(!sound_path(&fx, "boom").exists());
}

#[tokio::test]
async fn test_create_unknown_bus_name_is_a_reference_error() {
    let fx = fixture("create_unknown_bus");
    let (result, _out) = run(
        &fx,
        &["create", "boom", "--file", "a.wav", "--bus", "nope"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
}

#[tokio::test]
async fn test_create_numeric_zero_bus_is_rejected() {
    let fx = fixture("create_zero_bus");
    let (result, _out) = run(
        &fx,
        &["create", "boom", "--file", "a.wav", "--bus", "0"],
        &NonInteractiveInput,
    )
    .await;
    let err = result.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_EMPTY_REFERENCE
    );
}

#[tokio::test]
async fn test_create_rejects_unknown_scope_and_fader_with_choices() {
    let fx = fixture("create_bad_enum");
    let (scope, _) = run(
        &fx,
        &[
            "create", "a", "--file", "a.wav", "--bus", "sfx", "--scope", "galaxy",
        ],
        &NonInteractiveInput,
    )
    .await;
    let text = format!("{:#}", scope.unwrap_err());
    assert!(text.contains("world") && text.contains("entity"), "{text}");

    let (fader, _) = run(
        &fx,
        &[
            "create", "a", "--file", "a.wav", "--bus", "sfx", "--fader", "zigzag",
        ],
        &NonInteractiveInput,
    )
    .await;
    let text = format!("{:#}", fader.unwrap_err());
    assert!(text.contains("SCurveSmooth"), "{text}");
}

#[tokio::test]
async fn test_create_missing_bus_still_names_the_flag() {
    let fx = fixture("create_no_bus");
    let (result, _out) = run(
        &fx,
        &["create", "boom", "--file", "a.wav"],
        &NonInteractiveInput,
    )
    .await;
    assert!(format!("{:#}", result.unwrap_err()).contains("--bus"));
}

// ---------------------------------------------------------------------------
// Interactive create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_interactive_create_walks_every_field() {
    let fx = fixture("create_walk");
    let input = ScriptedInput::new(
        &[
            "sfx",      // bus
            "short",    // attenuation
            "(none)",   // effect
            "Entity",   // scope
            "EaseIn",   // fader
            "static",   // gain
            "static",   // pitch
            "static",   // priority
            "Position", // spatialization
        ],
        &["a.wav", "0.7", "1.1", "100"],
    )
    // shortcut, pitch, near-field gain, spread, stream, loop
    .with_confirms(&[false, true, false, false, true, false]);

    let (result, _out) = run(&fx, &["create", "boom"], &input).await;
    result.unwrap();
    input.assert_all_consumed();

    for label in [
        "audio file",
        "bus",
        "attenuation",
        "effect",
        "scope",
        "fader",
        "gain",
        "pitch",
        "near-field gain",
        "priority",
        "spread",
        "stream",
        "loop",
        "spatialization",
        "remaining optional fields",
    ] {
        assert!(input.saw_prompt(label), "no prompt mentioned '{label}'");
    }

    let sound = read_sound(&fx, "boom");
    assert_eq!(sound.path.as_deref(), Some("a.wav"));
    assert_eq!(sound.bus, 2);
    assert_eq!(sound.attenuation, 55);
    assert_eq!(sound.effect, 0);
    assert_eq!(sound.scope, Scope::Entity);
    assert_eq!(sound.fader.as_deref(), Some("EaseIn"));
    assert_eq!(sound.gain.as_ref().and_then(|g| g.as_static()), Some(0.7));
    assert_eq!(sound.pitch.as_ref().and_then(|g| g.as_static()), Some(1.1));
    assert!(sound.near_field_gain.is_none());
    assert_eq!(
        sound.priority.as_ref().and_then(|g| g.as_static()),
        Some(100.0)
    );
    assert!(sound.spread.is_none());
    assert!(sound.stream);
    assert!(!sound.loop_.unwrap().enabled);
    assert_eq!(sound.spatialization, Spatialization::Position);
}

#[tokio::test]
async fn test_interactive_walkthrough_fader_defaults_to_linear() {
    let fx = fixture("create_fader_default");
    // Accept the first fader option offered; everything else is the default.
    let input = ScriptedInput::new(
        &[
            "sfx", "(none)", "(none)", "World", "Linear", "static", "static", "None",
        ],
        &["1.0", "128"],
    )
    .with_confirms(&[false, false, false, false, false, false]);
    let (result, _out) = run(&fx, &["create", "boom", "--file", "a.wav"], &input).await;
    result.unwrap();
    input.assert_all_consumed();

    let faders = input
        .options_seen
        .lock()
        .unwrap()
        .iter()
        .find(|o| o.contains(&"SCurveSmooth".to_string()))
        .cloned()
        .expect("fader options were offered");
    assert_eq!(faders[0], "Linear");
    assert_eq!(read_sound(&fx, "boom").fader.as_deref(), Some("Linear"));
}

#[tokio::test]
async fn test_interactive_spread_prompt_does_not_suggest_full_circle() {
    let fx = fixture("create_spread_prompt");
    let input = ScriptedInput::new(
        &[
            "sfx", "(none)", "(none)", "World", "Linear", "static", "static", "static", "None",
        ],
        &["1.0", "128", "60"],
    )
    .with_confirms(&[false, false, false, true, false, false]);
    let (result, _out) = run(&fx, &["create", "boom", "--file", "a.wav"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(input.saw_prompt("Spread in degrees (0-360)"));
    let sound = read_sound(&fx, "boom");
    assert_eq!(
        sound.spread.as_ref().and_then(|s| s.as_static()),
        Some(60.0)
    );
}

#[tokio::test]
async fn test_interactive_update_rejects_empty_audio_file() {
    let fx = fixture("update_empty_file");
    let (created, _) = run(
        &fx,
        &["create", "boom", "--file", "a.wav", "--bus", "sfx"],
        &NonInteractiveInput,
    )
    .await;
    created.unwrap();
    let input = ScriptedInput::new(&[], &["  "]);
    let (result, _) = run(&fx, &["update", "boom"], &input).await;
    assert!(result.is_err());
    assert_eq!(read_sound(&fx, "boom").path.as_deref(), Some("a.wav"));
}

#[tokio::test]
async fn test_interactive_create_defaults_shortcut_yields_a_complete_sound() {
    let fx = fixture("create_shortcut");
    let input = ScriptedInput::new(&["sfx"], &[]).with_confirms(&[true]);

    let (result, _out) = run(&fx, &["create", "boom", "--file", "a.wav"], &input).await;
    result.unwrap();
    input.assert_all_consumed();

    // Only the required field and the shortcut question were asked.
    assert!(input.saw_prompt("bus"));
    assert!(input.saw_prompt("remaining optional fields"));
    assert!(!input.saw_prompt("pitch"));
    assert!(!input.saw_prompt("scope"));

    let sound = read_sound(&fx, "boom");
    assert_eq!(sound.bus, 2);
    assert_eq!(sound.attenuation, 0);
    assert_eq!(sound.effect, 0);
    assert_eq!(sound.scope, Scope::World);
    assert_eq!(sound.fader.as_deref(), Some("Linear"));
    assert_eq!(sound.gain.as_ref().and_then(|g| g.as_static()), Some(1.0));
    assert_eq!(
        sound.priority.as_ref().and_then(|g| g.as_static()),
        Some(128.0)
    );
    assert!(!sound.stream);
    assert_eq!(sound.spatialization, Spatialization::None);
}

#[tokio::test]
async fn test_interactive_create_does_not_ask_for_flags_already_given() {
    let fx = fixture("create_flags_skip");
    // Everything optional is on the command line, so no shortcut is offered either.
    let input = ScriptedInput::new(&[], &[]);
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "boom",
            "--file",
            "a.wav",
            "--bus",
            "sfx",
            "--attenuation",
            "none",
            "--effect",
            "none",
            "--scope",
            "world",
            "--fader",
            "Linear",
            "--gain",
            "0.5",
            "--priority",
            "10",
            "--stream",
            "--loop",
            "--spatialization",
            "none",
            "--pitch",
            "1.0",
            "--near-field-gain",
            "0.2",
            "--spread",
            "45",
        ],
        &input,
    )
    .await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_interactive_create_with_bad_bus_fails_before_any_prompt() {
    let fx = fixture("create_fail_fast");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _out) = run(&fx, &["create", "boom", "--bus", "nope"], &input).await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_changes_only_the_provided_fields() {
    let fx = fixture("update_partial");
    let (created, _) = run(
        &fx,
        &[
            "create",
            "boom",
            "--file",
            "a.wav",
            "--bus",
            "sfx",
            "--gain",
            "0.4",
            "--priority",
            "10",
            "--stream",
        ],
        &NonInteractiveInput,
    )
    .await;
    created.unwrap();
    let before = read_sound(&fx, "boom");

    let (updated, _out) = run(
        &fx,
        &[
            "update", "boom", "--effect", "echo", "--scope", "entity", "--pitch", "0.9",
        ],
        &NonInteractiveInput,
    )
    .await;
    updated.unwrap();
    let after = read_sound(&fx, "boom");

    assert_eq!(after.effect, 66);
    assert_eq!(after.scope, Scope::Entity);
    assert_eq!(after.pitch.as_ref().and_then(|p| p.as_static()), Some(0.9));

    // Everything else is preserved.
    let mut expected = before.clone();
    expected.effect = 66;
    expected.scope = Scope::Entity;
    expected.pitch = after.pitch.clone();
    assert_eq!(after, expected);
}

#[tokio::test]
async fn test_update_can_clear_optional_reference_with_none() {
    let fx = fixture("update_clear");
    let (created, _) = run(
        &fx,
        &[
            "create", "boom", "--file", "a.wav", "--bus", "sfx", "--effect", "echo",
        ],
        &NonInteractiveInput,
    )
    .await;
    created.unwrap();
    let (updated, _) = run(
        &fx,
        &["update", "boom", "--effect", "none"],
        &NonInteractiveInput,
    )
    .await;
    updated.unwrap();
    assert_eq!(read_sound(&fx, "boom").effect, 0);
}

#[tokio::test]
async fn test_update_unknown_effect_is_a_reference_error_and_leaves_file_alone() {
    let fx = fixture("update_bad_effect");
    let (created, _) = run(
        &fx,
        &["create", "boom", "--file", "a.wav", "--bus", "sfx"],
        &NonInteractiveInput,
    )
    .await;
    created.unwrap();
    let before = std::fs::read_to_string(sound_path(&fx, "boom")).unwrap();

    let (updated, _) = run(
        &fx,
        &["update", "boom", "--effect", "nope"],
        &NonInteractiveInput,
    )
    .await;
    let err = updated.unwrap_err();
    assert_eq!(
        as_cli_error(&err).unwrap().code,
        codes::ERR_VALIDATION_REFERENCE
    );
    assert_eq!(
        std::fs::read_to_string(sound_path(&fx, "boom")).unwrap(),
        before
    );
}

#[tokio::test]
async fn test_interactive_update_offers_the_new_fields() {
    let fx = fixture("update_interactive");
    let (created, _) = run(
        &fx,
        &["create", "boom", "--file", "a.wav", "--bus", "sfx"],
        &NonInteractiveInput,
    )
    .await;
    created.unwrap();
    let before = read_sound(&fx, "boom");

    // Keep everything except the effect and the scope.
    let input = ScriptedInput::new(
        &[
            "(keep current)", // bus
            "(keep current)", // attenuation
            "echo",           // effect
            "Entity",         // scope
            "Linear",         // fader (current)
            "Position",       // spatialization
        ],
        &["a.wav"],
    );
    // gain, pitch, near-field gain, priority, spread: no change; stream, loop: no change
    let input = input.with_confirms(&[false, false, false, false, false, false, false]);
    let (result, _) = run(&fx, &["update", "boom"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    for label in [
        "audio file",
        "bus",
        "attenuation",
        "effect",
        "scope",
        "fader",
        "gain",
        "pitch",
        "near-field gain",
        "priority",
        "spread",
        "stream",
        "loop",
        "spatialization",
    ] {
        assert!(input.saw_prompt(label), "no prompt mentioned '{label}'");
    }

    let after = read_sound(&fx, "boom");
    let mut expected = before.clone();
    expected.effect = 66;
    expected.scope = Scope::Entity;
    expected.spatialization = Spatialization::Position;
    assert_eq!(after, expected);
}

#[tokio::test]
async fn test_interactive_update_repairs_a_sound_without_fader_and_bus() {
    let fx = fixture("update_legacy_sound");
    fx.write_asset_json(
        "sounds",
        "old",
        json!({"id": 5, "name": "old", "path": "a.wav", "bus": 0, "attenuation": 0,
               "effect": 0, "scope": "World", "spatialization": "None", "stream": false,
               "loop": {"enabled": false, "loop_count": 0}}),
    )
    .unwrap();
    // "Constant" is the first fader option: picking it must count as a change.
    let input = ScriptedInput::new(
        &[
            "sfx",
            "(keep current)",
            "(keep current)",
            "World",
            "Constant",
            "None",
        ],
        &["a.wav"],
    )
    .with_confirms(&[false, false, false, false, false, false, false]);
    let (result, _) = run(&fx, &["update", "old"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    let sound = read_sound(&fx, "old");
    assert_eq!((sound.bus, sound.fader.as_deref()), (2, Some("Constant")));
}

// ---------------------------------------------------------------------------
// Hand-written RTPC binding without a static `value`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_keeps_a_hand_written_rtpc_gain_without_value() {
    let fx = fixture("rtpc_without_value");
    let path = fx
        .write_asset_json(
            "sounds",
            "wind",
            json!({
                "id": 9, "name": "wind", "path": "a.wav", "bus": 2,
                "gain": {"kind": "RTPC", "rtpc": {"id": 77}},
                "priority": {"kind": "Static", "value": 128.0},
                "stream": false, "loop": {"enabled": false, "loop_count": 0},
                "spatialization": "None", "attenuation": 0, "scope": "World",
                "fader": "Linear", "effect": 0
            }),
        )
        .unwrap();

    let (result, _) = run(
        &fx,
        &["update", "wind", "--priority", "10"],
        &NonInteractiveInput,
    )
    .await;
    result.unwrap_or_else(|e| panic!("{e:#}"));
    let sound = read_sound(&fx, "wind");
    let gain = sound.gain.expect("gain stays");
    assert_eq!(gain.kind, ValueKind::RTPC);
    assert_eq!(gain.rtpc.unwrap().id, 77);
    assert!(path.exists());
}
