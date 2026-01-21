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
            "Interactive {} '{}' blocked: non-interactive mode is enabled. \
             Please provide the required input via command-line arguments instead.",
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
}
