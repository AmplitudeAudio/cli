//! Interactive input implementation backed by `inquire`.
//!
//! This is the default input provider when neither `--json` nor `--non-interactive` is set.

use crate::input::Input;
use anyhow::Result;
use inquire::validator::Validation;
use inquire::{Confirm, Select, Text};

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
}
