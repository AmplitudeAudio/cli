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

//! Interactive prompts for `event create` and `event update`.
//!
//! Every function here is only called when the session can prompt; flag values
//! already present in the patch are never asked for again.

use anyhow::Result;

use crate::assets::generated::EventActionRunningMode;
use crate::assets::{Event, ProjectValidator};
use crate::input::{Input, select_index};
use crate::services::event::{ActionsPatch, EventPatch};

use super::event_action_prompts::{prompt_action_edits, prompt_new_actions, prompt_settings};
use super::sound_prompts::{ACCEPT_DEFAULTS_PROMPT, ask_choice, to_labels};

const RUN_MODES: [EventActionRunningMode; 2] = [
    EventActionRunningMode::Parallel,
    EventActionRunningMode::Sequential,
];
const RUN_MODE_LABELS: [&str; 2] = ["Parallel", "Sequential"];

/// Walks a new event through the actions and every optional field no flag already set.
///
/// The actions come first (an event needs at least one); then the author may
/// accept defaults for what is left (the run mode, and `active`/`scope` of the
/// actions that were just prompted) in a single answer.
pub fn complete_create_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    patch: EventPatch,
) -> Result<EventPatch> {
    let prompted = patch.actions.replace.is_none() && patch.actions.add.is_empty();
    let mut actions = match patch.actions.replace {
        Some(actions) => actions,
        None if prompted => prompt_new_actions(input, validator)?,
        None => Vec::new(),
    };

    let walkthrough_needed = patch.run_mode.is_none() || prompted;
    let accept_defaults =
        !walkthrough_needed || input.confirm(ACCEPT_DEFAULTS_PROMPT, Some(false))?;

    let mut run_mode = patch.run_mode;
    if !accept_defaults {
        if run_mode.is_none() {
            let index = select_index(input, "Action run mode", &to_labels(&RUN_MODE_LABELS))?;
            run_mode = Some(RUN_MODES[index]);
        }
        if prompted {
            actions = actions
                .iter()
                .enumerate()
                .map(|(i, action)| prompt_settings(input, action, i + 1))
                .collect::<Result<_>>()?;
        }
    }

    Ok(EventPatch {
        run_mode,
        actions: ActionsPatch {
            replace: (prompted || !actions.is_empty()).then_some(actions),
            ..patch.actions
        },
    })
}

/// Offers every event field with the current value; unchanged fields stay out of the patch.
pub fn prompt_update_patch(
    input: &dyn Input,
    validator: &ProjectValidator,
    event: &Event,
) -> Result<EventPatch> {
    let current = RUN_MODES
        .iter()
        .position(|m| *m == event.run_mode)
        .unwrap_or(0);
    let run_mode = ask_choice(
        input,
        "Action run mode",
        &to_labels(&RUN_MODE_LABELS),
        current,
    )?
    .map(|i| RUN_MODES[i]);

    let actions = prompt_action_edits(input, validator, event.actions.as_deref().unwrap_or(&[]))?;
    Ok(EventPatch {
        run_mode,
        actions: ActionsPatch {
            replace: actions,
            ..Default::default()
        },
    })
}
