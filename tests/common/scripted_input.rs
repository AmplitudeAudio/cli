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

//! Scripted `Input` test double shared by interactive-flow tests.

use std::sync::Mutex;

use am::input::Input;
use anyhow::Result;
use inquire::validator::Validation;

/// Pops queued answers (in order) and records every prompt it was asked.
///
/// `confirm` answers come from the queued confirms and fall back to the
/// prompt's default when the queue is empty.
pub struct ScriptedInput {
    selects: Mutex<Vec<String>>,
    texts: Mutex<Vec<String>>,
    confirms: Mutex<Vec<bool>>,
    /// Every prompt label shown, in order, across all prompt kinds.
    pub prompts: Mutex<Vec<String>>,
    /// The option list of every `select`, in order.
    pub options_seen: Mutex<Vec<Vec<String>>>,
}

impl ScriptedInput {
    pub fn new(selects: &[&str], texts: &[&str]) -> Self {
        let rev = |v: &[&str]| v.iter().rev().map(|s| s.to_string()).collect::<Vec<_>>();
        Self {
            selects: Mutex::new(rev(selects)),
            texts: Mutex::new(rev(texts)),
            confirms: Mutex::new(Vec::new()),
            prompts: Mutex::new(Vec::new()),
            options_seen: Mutex::new(Vec::new()),
        }
    }

    /// Queues explicit answers for `confirm` prompts.
    pub fn with_confirms(self, confirms: &[bool]) -> Self {
        *self.confirms.lock().unwrap() = confirms.iter().rev().copied().collect();
        self
    }

    /// Whether any recorded prompt contains `needle` (case-insensitive).
    pub fn saw_prompt(&self, needle: &str) -> bool {
        let needle = needle.to_lowercase();
        self.prompts
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.to_lowercase().contains(&needle))
    }

    /// Asserts every scripted select, text and confirm answer was used.
    pub fn assert_all_consumed(&self) {
        assert!(
            self.selects.lock().unwrap().is_empty(),
            "unused scripted selects: {:?}",
            self.selects.lock().unwrap()
        );
        assert!(
            self.texts.lock().unwrap().is_empty(),
            "unused scripted texts: {:?}",
            self.texts.lock().unwrap()
        );
        assert!(
            self.confirms.lock().unwrap().is_empty(),
            "unused scripted confirms: {:?}",
            self.confirms.lock().unwrap()
        );
    }

    fn record(&self, prompt: &str) {
        self.prompts.lock().unwrap().push(prompt.to_string());
    }
}

impl Input for ScriptedInput {
    fn prompt_text(
        &self,
        prompt: &str,
        _placeholder: Option<&str>,
        _formatter: Option<&dyn Fn(&str) -> String>,
        _validator: Option<&dyn Fn(&str) -> Result<Validation, inquire::CustomUserError>>,
    ) -> Result<String> {
        self.record(prompt);
        self.texts
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| anyhow::anyhow!("no scripted text for '{prompt}'"))
    }

    fn select(&self, prompt: &str, options: &[String]) -> Result<String> {
        self.record(prompt);
        self.options_seen.lock().unwrap().push(options.to_vec());
        self.selects
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| anyhow::anyhow!("no scripted select for '{prompt}'"))
    }

    fn confirm(&self, prompt: &str, default: Option<bool>) -> Result<bool> {
        self.record(prompt);
        Ok(self
            .confirms
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| default.unwrap_or(false)))
    }

    fn prompt_text_with_default(
        &self,
        prompt: &str,
        _default: &str,
        validator: Option<&dyn Fn(&str) -> Result<Validation, inquire::CustomUserError>>,
    ) -> Result<String> {
        self.record(prompt);
        let answer = self
            .texts
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| anyhow::anyhow!("no scripted text for '{prompt}'"))?;
        if let Some(v) = validator
            && let Ok(Validation::Invalid(msg)) = v(&answer)
        {
            anyhow::bail!("invalid: {:?}", msg);
        }
        Ok(answer)
    }

    fn multi_select(&self, _prompt: &str, _options: &[String]) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
}
