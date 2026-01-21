//! Input abstraction for interactive user prompts.
//!
//! This module separates *input acquisition* (prompting/selecting/confirming)
//! from *presentation/output* (InteractiveOutput/JsonOutput).
//!
//! Design goals:
//! - Commands call `&dyn Input` for all user input.
//! - `InteractiveInput` wraps `inquire` and supports validators/formatters/placeholders.
//! - `NonInteractiveInput` always fails with a helpful error suggesting CLI args.
//! - `--json` implies non-interactive input (handled by mode selection in main).

mod interactive;
mod non_interactive;

pub use interactive::InteractiveInput;
pub use non_interactive::NonInteractiveInput;

use anyhow::Result;
use inquire::validator::Validation;
use std::fmt::Display;

/// Input mode for CLI prompting.
///
/// Determines which input implementation is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Interactive prompting via `inquire`.
    #[default]
    Interactive,
    /// Prompts are disabled; any attempt to prompt/select/confirm returns an error suggesting CLI args.
    NonInteractive,
}

/// Abstraction over user input mechanisms (interactive prompts, non-interactive errors, etc.).
///
/// Commands should use this trait instead of calling `inquire::*` directly so that:
/// - non-interactive behavior can be enforced uniformly
/// - JSON mode can imply non-interactive input
/// - tests can supply a mock input provider if needed later
pub trait Input: Send + Sync {
    /// Prompt the user for text input.
    ///
    /// This should support the core `inquire::Text` capabilities:
    /// - placeholder
    /// - formatter
    /// - validator
    ///
    /// Notes:
    /// - Validators are optional; pass `None` to skip validation.
    /// - `formatter` receives the current input and should return the display string.
    ///
    /// `validator` is a function pointer/closure to match `inquire::Text::with_validator`'s
    /// generic bounds (trait objects do not work for `StringValidator` here).
    fn prompt_text(
        &self,
        prompt: &str,
        placeholder: Option<&str>,
        formatter: Option<&dyn Fn(&str) -> String>,
        validator: Option<&dyn Fn(&str) -> Result<Validation, inquire::CustomUserError>>,
    ) -> Result<String>;

    /// Prompt the user to select one option from a list.
    ///
    /// `options` is a slice of owned option labels. The return value is the selected label.
    fn select(&self, prompt: &str, options: &[String]) -> Result<String>;

    /// Prompt the user for confirmation (yes/no).
    fn confirm(&self, prompt: &str, default: Option<bool>) -> Result<bool>;
}

/// Create an `Input` implementation based on `InputMode`.
pub fn create_input(mode: InputMode) -> Box<dyn Input> {
    match mode {
        InputMode::Interactive => Box::new(InteractiveInput::new()),
        InputMode::NonInteractive => Box::new(NonInteractiveInput::new()),
    }
}

/// Helper for selecting from a list of items that implement `Display`, while keeping `Input`
/// object-safe (`&dyn Input`).
///
/// Returns the selected item's index from the provided slice.
///
/// Note: This renders options using `Display` (via `to_string()`), delegates selection to
/// `Input::select`, and then maps the selected label back to an index. If display labels are
/// not unique, the first matching item is returned.
pub fn select_index<T: Display>(input: &dyn Input, prompt: &str, options: &[T]) -> Result<usize> {
    let labels: Vec<String> = options.iter().map(|o| o.to_string()).collect();
    let selected = input.select(prompt, &labels)?;

    labels
        .iter()
        .position(|l| l == &selected)
        .ok_or_else(|| anyhow::anyhow!("Selection '{}' not found in options list", selected))
}
