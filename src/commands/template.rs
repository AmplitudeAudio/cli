use anyhow::Result;
use colored::*;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

use crate::{
    app::Resource,
    common::errors::{CliError, codes},
    database::{
        Database, db_get_template_by_name, db_get_templates,
        entities::{Template, TemplateSource},
    },
    input::Input,
    presentation::{Output, OutputMode},
};
use clap::Subcommand;

/// Maximum character length for template descriptions before truncation.
const DESCRIPTION_MAX_LENGTH: usize = 60;

/// Configuration option for templates.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateConfigOption {
    pub name: &'static str,
    pub description: &'static str,
    pub default_value: &'static str,
}

/// Embedded template definition for bundled templates.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedTemplate {
    pub name: &'static str,
    pub engine: &'static str,
    pub description: &'static str,
    pub config_options: &'static [TemplateConfigOption],
}

/// Default embedded templates bundled with the CLI.
pub const EMBEDDED_TEMPLATES: &[EmbeddedTemplate] = &[EmbeddedTemplate {
    name: "default",
    engine: "generic",
    description: "Default project template for any engine",
    config_options: &[],
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

    /// Display detailed information about a template
    Info {
        /// Name of the template to display
        name: String,
    },
}

pub async fn handler(
    command: &TemplateCommands,
    database: Option<Arc<Database>>,
    _input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        TemplateCommands::List {} => handle_list_templates_command(database, output).await,
        TemplateCommands::Info { name } => {
            handle_template_info_command(name, database, output).await
        }
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

/// Get the list of files for an embedded template by enumerating bundled resources.
///
/// Uses rust-embed `Resource::iter()` to find files matching the template prefix.
/// Results are sorted for deterministic output (Resource::iter() order is not guaranteed).
///
/// # Arguments
/// * `template_name` - The name of the embedded template (e.g., "default")
///
/// # Returns
/// A sorted vector of file names belonging to the template (e.g., ["default.buses.json", ...])
pub fn get_embedded_template_files(template_name: &str) -> Vec<String> {
    let prefix = format!("{}.", template_name);
    let mut files: Vec<String> = Resource::iter()
        .filter(|name| name.starts_with(&prefix))
        .map(|name| name.to_string())
        .collect();
    files.sort();
    files
}

/// Get the list of files and directories for a custom template by reading the directory.
///
/// Recursively lists files and directories up to 2 levels deep with relative paths.
/// Directories are indicated with a trailing "/" to distinguish them from files.
///
/// # Arguments
/// * `path` - The path to the custom template directory
///
/// # Returns
/// A sorted vector of file and directory names with relative paths
fn get_custom_template_files(path: &Path) -> Result<Vec<String>> {
    let mut entries = Vec::new();

    if !path.exists() {
        return Err(CliError::new(
            codes::ERR_TEMPLATE_NOT_FOUND,
            format!("Template path '{}' does not exist", path.display()),
            "The registered template path is no longer valid",
        )
        .with_suggestion("Re-register the template with 'am template register <path>'")
        .into());
    }

    // Helper to recursively collect entries up to a given depth
    // depth: current depth (0 = root directory being read)
    // max_depth: maximum depth to descend into (2 = can descend into 2 levels of subdirs)
    fn collect_entries(
        dir: &Path,
        prefix: &str,
        depth: usize,
        max_depth: usize,
        entries: &mut Vec<String>,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let full_name = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };

            if entry.file_type()?.is_file() {
                entries.push(full_name);
            } else if entry.file_type()?.is_dir() {
                // Only list directories and recurse if we haven't reached max depth
                // depth 0 can descend (depth < 2), depth 1 can descend (depth < 2), depth 2 cannot
                if depth < max_depth {
                    // Add directory entry with trailing "/"
                    entries.push(format!("{}/", full_name));

                    // Recurse into subdirectory
                    collect_entries(&entry.path(), &full_name, depth + 1, max_depth, entries)?;
                }
            }
        }
        Ok(())
    }

    // Collect entries up to 2 levels deep
    // max_depth=2: depth 0 reads root (adds level1/), depth 1 reads level1 (adds level2/)
    // depth 2 reads level2 (files only, no level3/ added because depth >= max_depth)
    collect_entries(path, "", 0, 2, &mut entries)?;
    entries.sort();
    Ok(entries)
}

/// Handle the `am template info <name>` command.
///
/// Displays detailed information about a template including
/// - Template name, engine, source, description
/// - List of files included in the template
async fn handle_template_info_command(
    name: &str,
    database: Option<Arc<Database>>,
    output: &dyn Output,
) -> Result<()> {
    // First, check if it's an embedded template
    if let Some(embedded) = EMBEDDED_TEMPLATES.iter().find(|t| t.name == name) {
        let files = get_embedded_template_files(name);
        display_template_info(
            name,
            embedded.engine,
            TemplateSource::Embedded,
            embedded.description,
            "bundled",
            files,
            embedded.config_options,
            output,
        );
        return Ok(());
    }

    // Check custom templates in the database
    if let Some(custom) = db_get_template_by_name(name, database)? {
        let files = get_custom_template_files(Path::new(&custom.path))?;
        display_template_info(
            &custom.name,
            custom.engine.as_deref().unwrap_or("generic"),
            TemplateSource::Custom,
            custom.description.as_deref().unwrap_or(""),
            &custom.path,
            files,
            &[], // Custom templates don't have config options stored
            output,
        );
        return Ok(());
    }

    // Template not found
    Err(CliError::new(
        codes::ERR_TEMPLATE_NOT_FOUND,
        format!("Template '{}' not found", name),
        "No embedded or registered template matches this name",
    )
    .into())
}

/// Display template information in the appropriate format.
fn display_template_info(
    name: &str,
    engine: &str,
    source: TemplateSource,
    description: &str,
    path: &str,
    files: Vec<String>,
    config_options: &[TemplateConfigOption],
    output: &dyn Output,
) {
    match output.mode() {
        OutputMode::Json => {
            let source_value = match source {
                TemplateSource::Embedded => "embedded",
                TemplateSource::Custom => "custom",
            };
            let config_options_json: Vec<serde_json::Value> = config_options
                .iter()
                .map(|opt| {
                    json!({
                        "name": opt.name,
                        "description": opt.description,
                        "default_value": opt.default_value
                    })
                })
                .collect();
            output.success(
                json!({
                    "name": name,
                    "engine": engine,
                    "source": source_value,
                    "description": description,
                    "path": path,
                    "files": files,
                    "config_options": config_options_json
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            let source_display = match source {
                TemplateSource::Embedded => "Embedded (bundled with CLI)".to_string(),
                TemplateSource::Custom => format!("Custom ({})", path),
            };

            output.progress(&format!("Template: {}", name.cyan().bold()));
            output.progress(&"═".repeat(59));
            output.progress("");
            output.progress(&format!("  Engine:       {}", engine));
            output.progress(&format!("  Source:       {}", source_display));
            output.progress(&format!("  Description:  {}", description));
            output.progress("");

            if !config_options.is_empty() {
                output.progress("  Configuration Options:");
                for option in config_options {
                    output.progress(&format!("    • {}: {} (default: {})",
                        option.name.cyan(),
                        option.description,
                        option.default_value.green()
                    ));
                }
                output.progress("");
            }

            output.progress("  Files:");
            for file in &files {
                output.progress(&format!("    • {}", file));
            }
        }
    }
}
