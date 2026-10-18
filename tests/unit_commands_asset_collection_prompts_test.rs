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

//! Tests for the interactive walk-throughs of `collection create` and `collection update`.
//!
//! These change the process working directory; run with `--test-threads=1`.

mod common;

use am::assets::generated::{
    CollectionEntry, RandomSoundSchedulerConfig, Scope, SequenceSoundSchedulerConfig,
    SequenceSoundSchedulerEndBehavior, SoundSchedulerConfig,
};
use common::collection_helpers::*;
use common::fixtures::AssetTestFixture;
use common::scripted_input::ScriptedInput;

// ---------------------------------------------------------------------------
// Interactive create
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_interactive_create_walks_every_field() {
    let fx = fixture("col_walk");
    let input = ScriptedInput::new(
        &[
            "sfx",      // bus
            "echo",     // effect
            "(none)",   // attenuation
            "Entity",   // scope
            "EaseIn",   // fader
            "PlayAll",  // play mode
            "Random",   // scheduler mode
            "static",   // gain
            "static",   // pitch
            "static",   // priority
            "Position", // spatialization
            "step1",    // entry 1 sound
            "step2",    // entry 2 sound
        ],
        &["3", "0.7", "1.1", "100", "2", "1"],
    )
    .with_confirms(&[
        false, // accept defaults?
        true,  // avoid repeating
        true,  // set pitch
        true,  // add a sound entry
        false, // entry 1 gain
        false, // entry 1 pitch
        true,  // add another
        false, // entry 2 gain
        false, // entry 2 pitch
        false, // add another
    ]);

    let (result, _out) = run(&fx, &["create", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();

    for label in [
        "bus",
        "effect",
        "attenuation",
        "scope",
        "fader",
        "play mode",
        "scheduler mode",
        "avoid repeating",
        "repeat count",
        "gain",
        "pitch",
        "priority",
        "spatialization",
        "sound entry",
        "weight",
        "remaining optional fields",
    ] {
        assert!(input.saw_prompt(label), "no prompt mentioned '{label}'");
    }

    let c = read_collection(&fx, "steps");
    assert_eq!((c.bus, c.effect, c.attenuation), (2, 66, 0));
    assert_eq!(c.scope, Scope::Entity);
    assert_eq!(c.fader.as_deref(), Some("EaseIn"));
    assert_eq!(c.play_mode.to_string(), "PlayAll");
    assert_eq!(c.gain.as_ref().and_then(|g| g.as_static()), Some(0.7));
    assert_eq!(c.pitch.as_ref().and_then(|g| g.as_static()), Some(1.1));
    assert_eq!(c.priority.as_ref().and_then(|g| g.as_static()), Some(100.0));
    assert_eq!(
        c.scheduler.unwrap().config,
        Some(SoundSchedulerConfig::Random(RandomSoundSchedulerConfig {
            avoid_repeat: true,
            repeat_count: 3,
        }))
    );
    let sounds = c.sounds.unwrap();
    assert!(matches!(&sounds[0], CollectionEntry::Random(e) if e.sound == 101 && e.weight == 2.0));
    assert!(matches!(&sounds[1], CollectionEntry::Random(e) if e.sound == 102 && e.weight == 1.0));
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_interactive_create_sequence_asks_the_end_behavior_and_no_weights() {
    let fx = fixture("col_walk_seq");
    let input = ScriptedInput::new(
        &[
            "sfx", "(none)", "(none)", "World", "Linear", "PlayOne", "Reverse", "static", "static",
            "None", "step1",
        ],
        &["1.0", "128"],
    )
    .with_confirms(&[false, false, true, false, false, false]);
    let (result, _out) = run(
        &fx,
        &["create", "stairs", "--scheduler-mode", "Sequence"],
        &input,
    )
    .await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("weight"));
    assert!(!input.saw_prompt("avoid repeating"));

    let c = read_collection(&fx, "stairs");
    assert_eq!(
        c.scheduler.unwrap().config,
        Some(SoundSchedulerConfig::Sequence(
            SequenceSoundSchedulerConfig {
                end_behavior: SequenceSoundSchedulerEndBehavior::Reverse,
            }
        ))
    );
    assert!(matches!(&c.sounds.unwrap()[0], CollectionEntry::Sequence(e) if e.sound == 101));
}

#[tokio::test]
async fn test_interactive_create_defaults_shortcut_yields_a_complete_collection() {
    let fx = fixture("col_shortcut");
    let input = ScriptedInput::new(&["sfx"], &[]).with_confirms(&[true]);
    let (result, _out) = run(&fx, &["create", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();

    // Only the required bus and the shortcut question were asked.
    assert!(input.saw_prompt("bus"));
    assert!(input.saw_prompt("remaining optional fields"));
    assert!(!input.saw_prompt("scope"));
    assert!(!input.saw_prompt("entry"));

    let c = read_collection(&fx, "steps");
    assert_eq!(c.bus, 2);
    assert_eq!(c.scope, Scope::World);
    assert_eq!(c.fader.as_deref(), Some("Linear"));
    assert_eq!(c.gain.as_ref().and_then(|g| g.as_static()), Some(1.0));
    assert_eq!(c.scheduler.unwrap().config, None);
    assert!(c.sounds.unwrap().is_empty());
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_interactive_create_does_not_ask_for_flags_already_given() {
    let fx = fixture("col_flags_skip");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _out) = run(
        &fx,
        &[
            "create",
            "steps",
            "--bus",
            "sfx",
            "--effect",
            "none",
            "--attenuation",
            "none",
            "--scope",
            "world",
            "--fader",
            "Linear",
            "--play-mode",
            "PlayOne",
            "--scheduler-mode",
            "Random",
            "--avoid-repeat",
            "false",
            "--gain",
            "0.5",
            "--pitch",
            "1.0",
            "--priority",
            "10",
            "--spatialization",
            "none",
            "--entry",
            "step1",
        ],
        &input,
    )
    .await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_interactive_create_with_bad_bus_fails_before_any_prompt() {
    let fx = fixture("col_fail_fast");
    let input = ScriptedInput::new(&[], &[]);
    let (result, _out) = run(&fx, &["create", "steps", "--bus", "nope"], &input).await;
    assert!(result.is_err());
    assert!(input.prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_interactive_create_without_sounds_skips_the_entry_questions() {
    let fx = AssetTestFixture::new("col_no_sounds").unwrap();
    let input = ScriptedInput::new(
        &[
            "sfx", "(none)", "(none)", "World", "Linear", "PlayOne", "Random", "static", "static",
            "None",
        ],
        &["1.0", "128"],
    )
    .with_confirms(&[false, false, false]);
    let (result, _out) = run(&fx, &["create", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    assert!(!input.saw_prompt("entry"));
}

// ---------------------------------------------------------------------------
// Interactive update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_interactive_update_offers_the_new_fields_and_edits_entries() {
    let fx = fixture("col_update_interactive");
    rich_collection(&fx).await;
    let before = read_collection(&fx, "steps");

    let input = ScriptedInput::new(
        &[
            "(keep current)", // bus
            "(keep current)", // attenuation
            "echo",           // effect
            "Entity",         // scope
            "Linear",         // fader (current)
            "PlayOne",        // play mode (current)
            "Random",         // scheduler mode (current)
            "Position",       // spatialization
            "Add an entry",   // entries menu
            "step2",          // entry sound
            "Done",
        ],
        &["2", "5"],
    )
    .with_confirms(&[
        true,  // avoid repeating (current: yes, unchanged)
        false, // gain
        false, // pitch
        false, // priority
        true,  // edit entries
        false, // entry gain
        false, // entry pitch
    ]);
    let (result, _out) = run(&fx, &["update", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    for label in [
        "bus",
        "attenuation",
        "effect",
        "scope",
        "fader",
        "play mode",
        "scheduler mode",
        "avoid repeating",
        "repeat count",
        "gain",
        "pitch",
        "priority",
        "spatialization",
        "entries",
    ] {
        assert!(input.saw_prompt(label), "no prompt mentioned '{label}'");
    }

    let after = read_collection(&fx, "steps");
    let mut expected = before.clone();
    expected.effect = 66;
    expected.scope = Scope::Entity;
    expected.spatialization = am::assets::generated::Spatialization::Position;
    expected.sounds = after.sounds.clone();
    assert_eq!(after, expected);
    let sounds = after.sounds.unwrap();
    assert_eq!(sounds.len(), 3);
    assert!(matches!(&sounds[2], CollectionEntry::Random(e) if e.sound == 102 && e.weight == 5.0));
    assert_flatc(&fx, "steps");
}

#[tokio::test]
async fn test_interactive_update_can_remove_an_entry() {
    let fx = fixture("col_update_interactive_remove");
    rich_collection(&fx).await;
    let input = ScriptedInput::new(
        &[
            "(keep current)",
            "(keep current)",
            "(keep current)",
            "World",
            "Linear",
            "PlayOne",
            "Random",
            "None",
            "Remove an entry",
            "step1",
            "Done",
        ],
        // The repeat count (current 2) is kept by answering with it.
        &["2"],
    )
    .with_confirms(&[
        true,  // avoid repeating (current value)
        false, // gain
        false, // pitch
        false, // priority
        true,  // edit entries
    ]);
    let (result, _out) = run(&fx, &["update", "steps"], &input).await;
    result.unwrap();
    input.assert_all_consumed();
    let sounds = read_collection(&fx, "steps").sounds.unwrap();
    assert_eq!(sounds.len(), 1);
    assert!(matches!(&sounds[0], CollectionEntry::Random(e) if e.sound == 102));
}

#[tokio::test]
async fn test_update_with_flags_does_not_prompt() {
    let fx = fixture("col_update_flags_no_prompt");
    rich_collection(&fx).await;
    let input = ScriptedInput::new(&[], &[]);
    let (result, _out) = run(&fx, &["update", "steps", "--gain", "0.3"], &input).await;
    result.unwrap();
    assert!(input.prompts.lock().unwrap().is_empty());
}
