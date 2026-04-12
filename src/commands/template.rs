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

use anyhow::Result;
use colored::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{
    app::Resource,
    common::{
        errors::{CliError, codes},
        utils::{truncate_string_at_word, validate_template_directory, validate_template_name},
    },
    database::{
        Database, db_create_template, db_delete_template_by_name, db_get_template_by_name,
        db_get_templates,
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
    #[command(after_help = "Examples:\n  am template list\n")]
    List {},

    /// Display detailed information about a template
    #[command(after_help = "Examples:\n  am template info default\n")]
    Info {
        /// Name of the template to display
        name: String,
    },

    /// Register a custom template from a directory
    #[command(after_help = "Examples:\n  am template register /path/to/template --name my_template\n")]
    Register {
        /// Path to the template directory
        path: String,

        /// Template name (optional; uses manifest name or prompts if not provided)
        #[arg(short, long)]
        name: Option<String>,

        /// Overwrite existing template with the same name
        #[arg(short, long, default_value = "false")]
        force: bool,
    },

    /// Unregister a custom template
    #[command(after_help = "Examples:\n  am template unregister my_template\n")]
    Unregister {
        /// Name of the template to unregister
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(short, long, default_value = "false")]
        force: bool,
    },
}

pub async fn handler(
    command: &TemplateCommands,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        TemplateCommands::List {} => handle_list_templates_command(database, output).await,
        TemplateCommands::Info { name } => {
            handle_template_info_command(name, database, output).await
        }
        TemplateCommands::Register { path, name, force } => {
            handle_template_register_command(path, name.clone(), *force, database, input, output)
                .await
        }
        TemplateCommands::Unregister { name, force } => {
            handle_template_unregister_command(name, *force, database, input, output).await
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
                    truncate_string_at_word(description, DESCRIPTION_MAX_LENGTH)
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
                    output.progress(&format!(
                        "    • {}: {} (default: {})",
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

/// Handle the `am template register <path>` command.
///
/// Registers a custom template from a directory path.
/// Validates the template structure, handles name resolution via manifest/prompt/arg,
/// and stores the template in the database.
async fn handle_template_register_command(
    path: &str,
    name: Option<String>,
    force: bool,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Normalize and validate the path
    let template_path = PathBuf::from(path);
    let normalized_path = template_path
        .canonicalize()
        .map_err(|_| {
            CliError::new(
                codes::ERR_INVALID_TEMPLATE_STRUCTURE,
                format!("Template path '{}' does not exist", path),
                "The specified path does not exist on the filesystem",
            )
            .with_suggestion("Verify the path is correct and the directory exists")
        })?
        .to_string_lossy()
        .to_string();

    // Step 2: Validate template structure (checks for required files)
    let validation_result = validate_template_directory(Path::new(&normalized_path))?;

    // Step 3: Determine template name (priority: CLI arg > manifest > prompt)
    let template_name = if let Some(provided_name) = name {
        // Name provided via --name flag
        provided_name
    } else if let Some(ref manifest) = validation_result.manifest {
        if let Some(ref manifest_name) = manifest.name {
            // Name from manifest
            manifest_name.clone()
        } else {
            // No name in manifest, need to prompt or fail
            prompt_for_template_name(input, output)?
        }
    } else {
        // No manifest, need to prompt or fail
        prompt_for_template_name(input, output)?
    };

    validate_template_name(&template_name).map_err(|msg| {
        CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid template name '{}'", template_name),
            msg,
        )
        .with_suggestion("Use only letters, numbers, underscores, and hyphens")
    })?;

    // Step 4: Check for name conflict with embedded templates
    if EMBEDDED_TEMPLATES.iter().any(|t| t.name == template_name) {
        return Err(CliError::new(
            codes::ERR_TEMPLATE_NAME_CONFLICT,
            format!("Template '{}' is a built-in template", template_name),
            "Cannot overwrite embedded templates",
        )
        .with_suggestion("Choose a different name for your custom template")
        .into());
    }

    // Step 5: Check for name conflict with existing custom templates
    if let Some(existing) = db_get_template_by_name(&template_name, database.clone())? {
        if force {
            // Delete existing template to allow overwrite
            db_delete_template_by_name(&existing.name, database.clone())?;
            log::debug!(
                "Deleted existing template '{}' for overwrite",
                template_name
            );
        } else {
            return Err(CliError::new(
                codes::ERR_TEMPLATE_NAME_CONFLICT,
                format!("Template '{}' already exists", template_name),
                "A custom template with this name is already registered",
            )
            .with_suggestion("Use a different name or --force to overwrite")
            .into());
        }
    }

    // Step 6: Create template entity
    let template = Template {
        id: None,
        name: template_name.clone(),
        path: normalized_path.clone(),
        engine: validation_result
            .manifest
            .as_ref()
            .and_then(|m| m.engine.clone())
            .or(Some("generic".to_string())),
        description: validation_result
            .manifest
            .as_ref()
            .and_then(|m| m.description.clone()),
        source: TemplateSource::Custom,
    };

    // Step 7: Insert into database
    db_create_template(&template, database)?;

    // Step 8: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "name": template.name,
                    "engine": template.engine.as_deref().unwrap_or("generic"),
                    "source": "custom",
                    "path": template.path
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Template '{}' registered successfully.",
                    template_name
                )),
                None,
            );
            output.progress("");
            output.progress(&format!(
                "Use '{}' to see all available templates.",
                "am template list".green()
            ));
        }
    }

    Ok(())
}

/// Handle the `am template unregister <name>` command.
///
/// Unregisters a custom template from the database.
/// - Embedded templates cannot be unregistered (they are bundled with the CLI)
/// - Requires confirmation prompt in interactive mode (unless --force is used)
/// - In non-interactive mode, --force flag is required
async fn handle_template_unregister_command(
    name: &str,
    force: bool,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Check if it's an embedded template (cannot be unregistered)
    if EMBEDDED_TEMPLATES.iter().any(|t| t.name == name) {
        return Err(CliError::new(
            codes::ERR_TEMPLATE_OPERATION_NOT_ALLOWED,
            format!("Cannot unregister embedded template '{}'", name),
            "Embedded templates are bundled with the CLI and cannot be removed",
        )
        .into());
    }

    // Step 2: Check if template exists in database
    let template = db_get_template_by_name(name, database.clone())?;
    if template.is_none() {
        return Err(CliError::new(
            codes::ERR_TEMPLATE_NOT_FOUND,
            format!("Template '{}' not found", name),
            "No registered template matches this name",
        )
        .into());
    }

    // Step 3: Handle confirmation
    if !force {
        // Try to get confirmation from user
        let confirm_result =
            input.confirm(&format!("Unregister template '{}'?", name), Some(false));

        match confirm_result {
            Ok(confirmed) => {
                if !confirmed {
                    // User cancelled
                    match output.mode() {
                        OutputMode::Json => {
                            output.success(
                                json!({
                                    "name": name,
                                    "removed": false,
                                    "cancelled": true
                                }),
                                None,
                            );
                        }
                        OutputMode::Interactive => {
                            output.progress("Cancelled.");
                        }
                    }
                    return Ok(());
                }
            }
            Err(e) => {
                // Confirmation prompt failed - this typically happens in non-interactive mode.
                // Preserve the underlying error context for debugging while providing actionable guidance.
                let error_detail = e.to_string();
                let why = if error_detail.contains("non-interactive")
                    || error_detail.contains("blocked")
                {
                    "Cannot confirm in non-interactive mode"
                } else {
                    "Confirmation prompt failed"
                };
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Confirmation required",
                    why,
                )
                .with_suggestion("Use --force flag to skip confirmation in non-interactive mode")
                .with_context(error_detail)
                .into());
            }
        }
    }

    // Step 4: Delete from database
    let deleted = db_delete_template_by_name(name, database)?;
    if !deleted {
        // Should be unreachable if step 2 succeeded, but handle safely
        return Err(CliError::new(
            codes::ERR_TEMPLATE_NOT_FOUND,
            format!("Template '{}' not found", name),
            "Template may have been removed by another process",
        )
        .into());
    }

    // Step 5: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "name": name,
                    "removed": true
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Template '{}' unregistered successfully.", name)),
                None,
            );
        }
    }

    Ok(())
}

/// Prompt for template name in interactive mode.
///
/// Returns an error in non-interactive mode when name is required.
/// Uses the Input trait to attempt prompting - if input is non-interactive,
/// the prompt will fail and we return a helpful error with --name suggestion.
fn prompt_for_template_name(input: &dyn Input, _output: &dyn Output) -> Result<String> {
    // Attempt to prompt for template name.
    // If input is NonInteractiveInput (due to --json or --non-interactive flags),
    // the prompt will fail. We catch that and return a more helpful error.
    input
        .prompt_text(
            "Template name",
            None,
            None,
            Some(&|value: &str| match validate_template_name(value) {
                Ok(()) => Ok(inquire::validator::Validation::Valid),
                Err(msg) => Ok(inquire::validator::Validation::Invalid(msg.into())),
            }),
        )
        .map_err(|_| {
            // Convert the generic "blocked" error to a more helpful CliError
            CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Template name is required",
                "No --name provided and no name in template manifest",
            )
            .with_suggestion("Provide --name flag or add 'name' to template.json manifest")
            .into()
        })
}
