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

//! Interactive input implementation backed by `inquire`.
//!
//! This is the default input provider when neither `--json` nor `--non-interactive` is set.

use crate::input::Input;
use anyhow::Result;
use inquire::validator::Validation;
use inquire::{Confirm, MultiSelect, Select, Text};

#[derive(Debug, Default, Clone, Copy)]
pub struct InteractiveInput;

impl InteractiveInput {
    pub fn new() -> Self {
        Self
    }
}

impl Input for InteractiveInput {
    fn prompt_text(
        &self,
        prompt: &str,
        placeholder: Option<&str>,
        formatter: Option<&dyn Fn(&str) -> String>,
        validator: Option<&dyn Fn(&str) -> Result<Validation, inquire::CustomUserError>>,
    ) -> Result<String> {
        let mut t = Text::new(prompt);

        if let Some(ph) = placeholder {
            t = t.with_placeholder(ph);
        }

        if let Some(f) = formatter {
            t = t.with_formatter(f);
        }

        if let Some(v) = validator {
            t = t.with_validator(v);
        }

        Ok(t.prompt()?)
    }

    fn select(&self, prompt: &str, options: &[String]) -> Result<String> {
        let s = Select::new(prompt, options.to_vec());
        Ok(s.prompt()?)
    }

    fn confirm(&self, prompt: &str, default: Option<bool>) -> Result<bool> {
        let mut c = Confirm::new(prompt);

        if let Some(d) = default {
            c = c.with_default(d);
        }

        Ok(c.prompt()?)
    }

    fn prompt_text_with_default(
        &self,
        prompt: &str,
        default: &str,
        validator: Option<&dyn Fn(&str) -> Result<Validation, inquire::CustomUserError>>,
    ) -> Result<String> {
        let mut t = Text::new(prompt).with_default(default);

        if let Some(v) = validator {
            t = t.with_validator(v);
        }

        Ok(t.prompt()?)
    }

    fn multi_select(&self, prompt: &str, options: &[String]) -> Result<Vec<String>> {
        let ms = MultiSelect::new(prompt, options.to_vec())
            .with_help_message("Use Space to toggle, Enter to confirm");
        Ok(ms.prompt()?)
    }
}
