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

//! Non-interactive input implementation.
//!
//! This input provider is used when interactive prompts must be disabled,
//! e.g. when `--non-interactive` is provided or when `--json` output mode
//! is active (JSON implies non-interactive).
//!
//! All input methods fail with a clear error message suggesting the user
//! provide the required value via command-line arguments.

use crate::input::Input;
use anyhow::Result;
use inquire::validator::Validation;

#[derive(Debug, Default, Clone, Copy)]
pub struct NonInteractiveInput;

impl NonInteractiveInput {
    pub fn new() -> Self {
        Self
    }

    fn blocked(&self, kind: &str, prompt: &str) -> anyhow::Error {
        anyhow::anyhow!(
            "Interactive {} '{}' blocked: non-interactive mode is active. \
             Provide the required value via command-line flags instead. \
             Use --help on the command to see available flags.",
            kind,
            prompt
        )
    }
}

impl Input for NonInteractiveInput {
    fn prompt_text(
        &self,
        prompt: &str,
        _placeholder: Option<&str>,
        _formatter: Option<&dyn Fn(&str) -> String>,
        _validator: Option<&dyn Fn(&str) -> Result<Validation, inquire::CustomUserError>>,
    ) -> Result<String> {
        Err(self.blocked("prompt", prompt))
    }

    fn select(&self, prompt: &str, _options: &[String]) -> Result<String> {
        Err(self.blocked("selection", prompt))
    }

    fn confirm(&self, prompt: &str, _default: Option<bool>) -> Result<bool> {
        Err(self.blocked("confirmation", prompt))
    }

    fn prompt_text_with_default(
        &self,
        prompt: &str,
        _default: &str,
        _validator: Option<&dyn Fn(&str) -> Result<Validation, inquire::CustomUserError>>,
    ) -> Result<String> {
        Err(self.blocked("prompt", prompt))
    }

    fn multi_select(&self, prompt: &str, _options: &[String]) -> Result<Vec<String>> {
        Err(self.blocked("multi-select", prompt))
    }
}
