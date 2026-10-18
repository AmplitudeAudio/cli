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

//! Hand-maintained field documentation.
//!
//! The compiled `.bfbs` schemas carry no doc comments, so one-line summaries
//! are copied from the SDK `.fbs` sources. Asset types not listed return
//! `None` until later tasks extend this table.

use crate::assets::AssetType;

const SOUND_DOCS: &[(&str, &str)] = &[
    ("id", "Unique identifier for the sound."),
    ("name", "The name of the sound."),
    ("effect", "The effect to apply to this sound on playback."),
    (
        "gain",
        "Linear gain (0.0 .. 1.0) of the sound when played back.",
    ),
    (
        "near_field_gain",
        "Near field effect gain (0.0 .. 1.0), applied to sources closer than 1m to the listener.",
    ),
    (
        "pitch",
        "Custom pitch applied to the sound, combined with the doppler pitch.",
    ),
    ("bus", "ID of the bus this sound should be played on."),
    (
        "priority",
        "Priority of this sound relative to others when playback channels run out.",
    ),
    (
        "stream",
        "Whether this sound is streamed (music) or loaded into a buffer (effects).",
    ),
    ("loop", "Configures how this sound loops when playing."),
    (
        "spatialization",
        "Whether gain is adjusted by distance to the listener (positional) or not.",
    ),
    (
        "attenuation",
        "The ID of the attenuation to apply to this sound.",
    ),
    (
        "scope",
        "The scope where to apply this sound. The default is World.",
    ),
    ("fader", "The fader algorithm used by this sound."),
    ("path", "Path to the audio sample file."),
    (
        "spread",
        "Width in degrees (0 - 360) of a multichannel sound's image; ignored for mono.",
    ),
];

/// Returns the one-line documentation of `field` for `asset_type`, if known.
pub fn doc_for(asset_type: AssetType, field: &str) -> Option<&'static str> {
    let table = match asset_type {
        AssetType::Sound => SOUND_DOCS,
        _ => return None,
    };
    table
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, doc)| *doc)
}
