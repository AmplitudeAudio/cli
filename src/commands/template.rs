use anyhow::Result;
use colored::*;
use serde_json::json;
use std::sync::Arc;

use crate::{
    database::{
        Database, db_get_templates,
        entities::{Template, TemplateSource},
    },
    input::Input,
    presentation::{Output, OutputMode},
};
use clap::Subcommand;

/// Maximum character length for template descriptions before truncation.
const DESCRIPTION_MAX_LENGTH: usize = 60;

/// Embedded template definition for bundled templates.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedTemplate {
    pub name: &'static str,
    pub engine: &'static str,
    pub description: &'static str,
}

/// Default embedded templates bundled with the CLI.
pub const EMBEDDED_TEMPLATES: &[EmbeddedTemplate] = &[EmbeddedTemplate {
    name: "default",
    engine: "generic",
    description: "Default project template for any engine",
}];

/// Truncate a string to a maximum length, adding ellipsis if truncated.
fn truncate_description(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // Find a good break point (space) near the limit
        let truncated = &s[..max_len.saturating_sub(3)];
        let break_point = truncated.rfind(' ').unwrap_or(truncated.len());
        format!("{}...", &s[..break_point])
    }
}

impl EmbeddedTemplate {
    /// Convert to a Template struct for display.
    pub fn to_template(&self) -> Template {
        Template {
            id: None,
            name: self.name.to_string(),
            path: "bundled".to_string(),
            engine: Some(self.engine.to_string()),
            description: Some(self.description.to_string()),
            source: TemplateSource::Embedded,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum TemplateCommands {
    /// List all available templates
    List {},
}

pub async fn handler(
    command: &TemplateCommands,
    database: Option<Arc<Database>>,
    _input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        TemplateCommands::List {} => handle_list_templates_command(database, output).await,
    }
}

async fn handle_list_templates_command(
    database: Option<Arc<Database>>,
    output: &dyn Output,
) -> Result<()> {
    // Get embedded templates first
    let embedded: Vec<Template> = EMBEDDED_TEMPLATES.iter().map(|t| t.to_template()).collect();

    // Get custom templates from database (sorted alphabetically)
    // Propagate database errors properly; empty list is only valid for healthy DB with no templates
    let custom = db_get_templates(database)?;

    // Combine: embedded first, then custom
    let mut all_templates = embedded;
    all_templates.extend(custom);

    // Check if we have any custom templates
    let has_custom = all_templates
        .iter()
        .any(|t| t.source == TemplateSource::Custom);

    // Build display data
    // Use snake_case for source in JSON mode for schema consistency
    // Use colored source values for interactive mode for visual distinction
    // Truncate descriptions for interactive mode to prevent table overflow
    let display_data: Vec<serde_json::Value> = all_templates
        .iter()
        .map(|t| {
            let source_value = match output.mode() {
                OutputMode::Json => match t.source {
                    TemplateSource::Embedded => "embedded".to_string(),
                    TemplateSource::Custom => "custom".to_string(),
                },
                OutputMode::Interactive => match t.source {
                    TemplateSource::Embedded => format!("{}", "Embedded".cyan()),
                    TemplateSource::Custom => format!("{}", "Custom".yellow()),
                },
            };
            let description = t.description.as_deref().unwrap_or("");
            let display_description = match output.mode() {
                OutputMode::Json => description.to_string(),
                OutputMode::Interactive => {
                    truncate_description(description, DESCRIPTION_MAX_LENGTH)
                }
            };
            json!({
                "name": t.name,
                "engine": t.engine.as_deref().unwrap_or("generic"),
                "source": source_value,
                "description": display_description
            })
        })
        .collect();

    // Display based on output mode
    match output.mode() {
        OutputMode::Json => {
            output.success(json!(display_data), None);
        }
        OutputMode::Interactive => {
            output.table(Some("Available Templates"), json!(display_data));

            if !has_custom {
                output.progress("");
                output.progress(&format!(
                    "Tip: Register custom templates with {}",
                    "am template register <path>".green()
                ));
            }
        }
    }

    Ok(())
}
