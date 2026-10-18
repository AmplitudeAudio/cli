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

//! RTPC creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI produce an [`RtpcValues`] (create) or an
//! [`RtpcPatch`] (update); this module turns them into a validated [`Rtpc`]
//! without touching the terminal or the file system. Functions return new
//! values and never mutate their inputs.

use crate::assets::generated::{FadeTransitionSettings, RtpcFadingSettings};
use crate::assets::{Asset, ProjectContext, Rtpc, ValidationError, ValidationLayer};
use crate::common::errors::codes::ERR_ASSET_ALREADY_EXISTS;
use crate::common::utils::generate_unique_id;

/// Number of times id generation is retried after a collision.
const ID_COLLISION_RETRIES: usize = 3;

/// Fader used for a transition when none is given (a registered SDK fader).
pub const DEFAULT_FADER: &str = "Linear";

/// A fade transition (attack or release) requested for a new RTPC.
#[derive(Debug, Clone, PartialEq)]
pub struct FadeTransitionValues {
    pub duration: f64,
    pub fader: String,
}

impl Default for FadeTransitionValues {
    fn default() -> Self {
        Self {
            duration: 0.0,
            fader: DEFAULT_FADER.to_string(),
        }
    }
}

/// Fading settings requested for a new RTPC.
#[derive(Debug, Clone, PartialEq)]
pub struct FadeValues {
    pub enabled: bool,
    pub attack: FadeTransitionValues,
    pub release: FadeTransitionValues,
}

/// Every value a new RTPC needs.
#[derive(Debug, Clone, PartialEq)]
pub struct RtpcValues {
    pub name: String,
    pub min_value: f64,
    pub max_value: f64,
    /// `None` uses the schema default (0), clamped into `[min_value, max_value]`.
    pub default_value: Option<f64>,
    /// `None` leaves the block out (no fading).
    pub fade: Option<FadeValues>,
}

/// Requested fading changes; `None` keeps the current value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FadePatch {
    pub enabled: Option<bool>,
    pub attack_duration: Option<f64>,
    pub attack_fader: Option<String>,
    pub release_duration: Option<f64>,
    pub release_fader: Option<String>,
}

impl FadePatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none()
            && self.attack_duration.is_none()
            && self.attack_fader.is_none()
            && self.release_duration.is_none()
            && self.release_fader.is_none()
    }
}

/// The fields of an existing RTPC a caller wants to change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RtpcPatch {
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub default_value: Option<f64>,
    pub fade: FadePatch,
}

impl RtpcPatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.changed_fields().is_empty()
    }

    /// Schema names of the fields this patch sets, in a stable order.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        [
            ("min_value", self.min_value.is_some()),
            ("max_value", self.max_value.is_some()),
            ("default_value", self.default_value.is_some()),
            ("fade_settings", !self.fade.is_empty()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

fn unique_id(name: &str, ctx: &ProjectContext) -> Result<u64, ValidationError> {
    let mut id = generate_unique_id(name);
    for attempt in 0..ID_COLLISION_RETRIES {
        if !ctx.has_id(id) {
            break;
        }
        id = generate_unique_id(&format!("{name}{attempt}"));
    }
    if ctx.has_id(id) {
        return Err(ValidationError::new(
            ValidationLayer::IdUniqueness,
            ERR_ASSET_ALREADY_EXISTS,
            format!("Generated ID {id} for RTPC '{name}' collides with an existing asset"),
            "All generated ID attempts collided with existing assets in the project",
        )
        .with_suggestion("Try a different name or wait a moment and retry")
        .with_field("id"));
    }
    Ok(id)
}

fn transition_settings(values: &FadeTransitionValues) -> FadeTransitionSettings {
    FadeTransitionSettings {
        duration: values.duration,
        fader: Some(values.fader.clone()),
    }
}

/// The schema default (0), pulled into the range so the RTPC starts valid.
fn resolve_default(values: &RtpcValues) -> f64 {
    values.default_value.unwrap_or_else(|| {
        if values.min_value <= values.max_value {
            0.0_f64.clamp(values.min_value, values.max_value)
        } else {
            0.0
        }
    })
}

/// Builds a validated RTPC from `values`, generating a unique id.
pub fn build_rtpc(values: RtpcValues, ctx: &ProjectContext) -> Result<Rtpc, ValidationError> {
    let id = unique_id(&values.name, ctx)?;

    let mut builder = Rtpc::builder(id, &values.name)
        .min_value(values.min_value)
        .max_value(values.max_value)
        .default_value(resolve_default(&values));
    if let Some(fade) = &values.fade {
        builder = builder.fade_settings(RtpcFadingSettings {
            enabled: fade.enabled,
            fade_attack: Some(transition_settings(&fade.attack)),
            fade_release: Some(transition_settings(&fade.release)),
        });
    }
    let rtpc = builder.build();

    rtpc.validate_rules(ctx)?;
    Ok(rtpc)
}

fn merged_transition(
    current: Option<&FadeTransitionSettings>,
    duration: Option<f64>,
    fader: Option<&String>,
) -> FadeTransitionSettings {
    let fallback = FadeTransitionValues::default();
    FadeTransitionSettings {
        duration: duration
            .or(current.map(|c| c.duration))
            .unwrap_or(fallback.duration),
        fader: Some(
            fader
                .cloned()
                .or_else(|| current.and_then(|c| c.fader.clone()))
                .unwrap_or(fallback.fader),
        ),
    }
}

fn merged_fade(
    current: Option<&RtpcFadingSettings>,
    patch: &FadePatch,
) -> Option<RtpcFadingSettings> {
    if patch.is_empty() {
        return current.cloned();
    }
    // Giving a transition flag without saying otherwise turns fading on.
    let enabled = patch.enabled.or(current.map(|c| c.enabled)).unwrap_or(true);
    // Both transitions are always written so the SDK never meets a null one.
    Some(RtpcFadingSettings {
        enabled,
        fade_attack: Some(merged_transition(
            current.and_then(|c| c.fade_attack.as_ref()),
            patch.attack_duration,
            patch.attack_fader.as_ref(),
        )),
        fade_release: Some(merged_transition(
            current.and_then(|c| c.fade_release.as_ref()),
            patch.release_duration,
            patch.release_fader.as_ref(),
        )),
    })
}

/// Merges `patch` into a copy of `existing`; fields the patch leaves unset are preserved.
pub fn apply_patch(existing: &Rtpc, patch: &RtpcPatch) -> Rtpc {
    let mut rtpc = existing.clone();
    if let Some(value) = patch.min_value {
        rtpc.min_value = value;
    }
    if let Some(value) = patch.max_value {
        rtpc.max_value = value;
    }
    if let Some(value) = patch.default_value {
        rtpc.default_value = value;
    }
    rtpc.fade_settings = merged_fade(existing.fade_settings.as_ref(), &patch.fade);
    rtpc
}

/// Serializes `updated` on top of the file's `original` JSON, so keys this
/// version of the CLI does not know about survive an update.
pub fn merge_into_original(
    original: &serde_json::Value,
    updated: &Rtpc,
) -> serde_json::Result<serde_json::Value> {
    let mut merged = original.as_object().cloned().unwrap_or_default();
    if let serde_json::Value::Object(fields) = serde_json::to_value(updated)? {
        merged.extend(fields);
    }
    Ok(serde_json::Value::Object(merged))
}
