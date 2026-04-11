//! Effect asset commands.
//!
//! Implements CRUD operations for Effect assets in Amplitude projects.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use inquire::validator::Validation;
use serde_json::json;

use crate::common::utils::generate_unique_id;
use crate::{
    assets::{Asset, Effect, ProjectContext, ProjectValidator, RtpcCompatibleValue},
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        utils::read_amproject_file,
    },
    database::Database,
    input::Input,
    presentation::{Output, OutputMode},
};

/// The name of the current asset.
const ASSET_NAME: &str = "Effect";

/// Maximum number of ID generation retries before giving up.
const MAX_ID_RETRIES: u32 = 3;

/// Effect asset subcommands.
#[derive(Subcommand, Debug)]
pub enum EffectCommands {
    /// Create a new effect asset
    #[command(after_help = "Examples:
  am asset effect create reverb
  am asset effect create eq --effect-type equalizer
")]
    Create {
        /// Name of the effect asset
        name: String,

        /// Effect type name (e.g., "reverb", "eq")
        #[arg(long)]
        effect_type: Option<String>,

        /// Parameter values as static floats (repeatable)
        #[arg(long = "param")]
        param: Option<Vec<f32>>,
    },

    /// List all effect assets in the project
    #[command(after_help = "Examples:
  am asset effect list
  am asset effect list --json
")]
    List {},

    /// Update an existing effect asset
    #[command(after_help = "Examples:
  am asset effect update reverb --effect-type hall_reverb
")]
    Update {
        /// Name of the effect asset to update
        name: String,

        /// New effect type name
        #[arg(long)]
        effect_type: Option<String>,

        /// New parameter values as static floats (repeatable)
        #[arg(long = "param")]
        param: Option<Vec<f32>>,
    },

    /// Delete an effect asset
    #[command(after_help = "Examples:
  am asset effect delete reverb
  am asset effect delete reverb --force
")]
    Delete {
        /// Name of the effect asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle effect commands by routing to the appropriate handler.
pub async fn handler(
    command: &EffectCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        EffectCommands::Create {
            name,
            effect_type,
            param,
        } => create_effect(name, effect_type.clone(), param.clone(), input, output).await,
        EffectCommands::List {} => list_effects(output).await,
        EffectCommands::Update {
            name,
            effect_type,
            param,
        } => update_effect(name, effect_type.clone(), param.clone(), input, output).await,
        EffectCommands::Delete { name, force } => delete_effect(name, *force, input, output).await,
    }
}

// =============================================================================
// Create
// =============================================================================

/// Create a new effect asset.
async fn create_effect(
    name: &str,
    effect_type: Option<String>,
    param: Option<Vec<f32>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Validate name is not empty and contains no path separators
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Effect name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset effect create <name>'")
        .into());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Effect name contains invalid characters",
            "Name cannot contain path separators (/, \\) or parent directory references (..)",
        )
        .with_suggestion("Use a simple name without slashes or dots")
        .into());
    }

    // Step 2: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating effect '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 3: Validate effect name doesn't already exist (filesystem + registry)
    let effects_dir = current_dir.join("sources").join("effects");
    let effect_file_path = effects_dir.join(format!("{}.json", name));

    if effect_file_path.exists() {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset effect update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Build populated ProjectContext for validation
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Check name uniqueness via ProjectContext registry
    if context.has_name(crate::assets::AssetType::Effect, name) {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset effect update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Step 4: Get effect type (validate whitespace-only)
    let effect_type_value = if let Some(et) = effect_type {
        let trimmed = et.trim().to_string();
        if trimmed.is_empty() {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Effect type cannot be whitespace-only",
                "Provide a non-empty effect type or omit the flag",
            )
            .into());
        }
        Some(trimmed)
    } else {
        prompt_effect_type(input)?
    };

    // Step 5: Get parameters (validate finite values)
    let parameters_value = if let Some(params) = param {
        for &v in &params {
            if !v.is_finite() {
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Parameter value must be finite",
                    "NaN and Infinity are not valid parameter values",
                )
                .into());
            }
        }
        Some(
            params
                .iter()
                .map(|&v| RtpcCompatibleValue::static_value(v))
                .collect(),
        )
    } else {
        prompt_parameters(input)?
    };

    // Step 6: Generate unique ID with collision check
    let mut id = generate_unique_id(name);
    let mut retries = 0;
    while context.has_id(id) && retries < MAX_ID_RETRIES {
        id = generate_unique_id(&format!("{}{}", name, retries));
        retries += 1;
    }
    if context.has_id(id) {
        return Err(CliError::new(
            codes::ERR_ASSET_ALREADY_EXISTS,
            format!("Generated ID {} collides with an existing asset", id),
            "All generated ID attempts collided with existing assets in the project",
        )
        .with_suggestion("Try a different name or wait a moment and retry")
        .into());
    }

    // Step 7: Build the Effect asset
    let mut builder = Effect::builder(id, name);
    if let Some(et) = effect_type_value {
        builder = builder.effect_type(et);
    }
    if let Some(params) = parameters_value {
        builder = builder.parameters(params);
    }
    let effect = builder.build();

    // Step 8: Validate
    effect.validate_rules(&context)?;

    // Step 9: Serialize to JSON
    let json_content =
        serde_json::to_string_pretty(&effect).context("Failed to serialize effect to JSON")?;

    // Step 10: Ensure directory exists and write atomically
    fs::create_dir_all(&effects_dir)?;
    atomic_write(&effect_file_path, json_content.as_bytes())?;

    // Step 11: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": effect.id,
                    "name": effect.name(),
                    "path": effect_file_path.to_string_lossy(),
                    "effect_type": effect.effect,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Effect '{}' created successfully at {}",
                    name,
                    effect_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}

// =============================================================================
// List
// =============================================================================

/// List all effect assets in the current project.
async fn list_effects(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Scan effects directory
    let effects_dir = current_dir.join("sources").join("effects");

    // Step 3: Handle missing directory
    if !effects_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "effects": [],
                        "count": 0,
                        "warnings": ["No effects directory found. Create effects with 'am asset effect create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No effects directory found.");
                output.progress(&format!(
                    "Create effects with '{}'.",
                    "am asset effect create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 4: Read and parse all .json files
    let mut effects: Vec<Effect> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let entries = match fs::read_dir(&effects_dir) {
        Ok(entries) => entries,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read effects directory",
                format!("Permission denied on {}", effects_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    let canonical_effects_dir = effects_dir
        .canonicalize()
        .unwrap_or_else(|_| effects_dir.clone());

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Skip symlinks that resolve outside the effects directory
        if path.is_symlink()
            && path
                .canonicalize()
                .is_ok_and(|resolved| !resolved.starts_with(&canonical_effects_dir))
        {
            log::warn!(
                "Skipping symlink outside effects directory: {}",
                path.display()
            );
            continue;
        }

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Effect>(&content) {
                    Ok(effect) => {
                        effects.push(effect);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        log::warn!("Skipping invalid effect file: {}", path.display());
                        warnings.push(format!("Invalid JSON in {}: {}", filename, e));
                    }
                },
                Err(e) => {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    log::warn!("Failed to read effect file: {}", path.display());
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 5: Sort by name
    effects.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 6: Handle empty
    if effects.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "effects": [],
                        "count": 0,
                        "warnings": warnings
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                for warning in &warnings {
                    output.progress(&format!("{} {}", "Warning:".yellow(), warning));
                }
                if !warnings.is_empty() {
                    output.progress("");
                }
                output.progress("No effects found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset effect create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 7: Output
    match output.mode() {
        OutputMode::Json => {
            let effect_data: Vec<serde_json::Value> = effects
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id,
                        "name": e.name(),
                        "effect_type": e.effect,
                        "parameter_count": e.parameters.as_ref().map(|p| p.len()).unwrap_or(0)
                    })
                })
                .collect();

            output.success(
                json!({
                    "effects": effect_data,
                    "count": effects.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = effects
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id,
                        "name": e.name(),
                        "effect_type": e.effect.as_deref().unwrap_or("")
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} effect(s) found", effects.len()));
        }
    }

    Ok(())
}

// =============================================================================
// Update
// =============================================================================

/// Update an existing effect asset.
async fn update_effect(
    name: &str,
    effect_type: Option<String>,
    param: Option<Vec<f32>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating effect '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Locate existing effect file
    let effects_dir = current_dir.join("sources").join("effects");
    let effect_file_path = effects_dir.join(format!("{}.json", name));

    if !effect_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset effect list' to see available effects, or 'am asset effect create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing effect
    let content = fs::read_to_string(&effect_file_path).context(format!(
        "Failed to read effect file: {}",
        effect_file_path.display()
    ))?;
    let mut effect: Effect = serde_json::from_str(&content).context(format!(
        "Failed to parse effect file: {}",
        effect_file_path.display()
    ))?;

    // Step 4: Determine if we have any flag values (non-interactive mode)
    let has_any_flag = effect_type.is_some() || param.is_some();

    // Step 5: Apply updates
    let updated_fields: Vec<String> = if has_any_flag {
        apply_flag_updates(&mut effect, effect_type, param)?
    } else {
        prompt_effect_updates(&mut effect, input)?
    };

    // Step 6: Validate
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    effect.validate_rules(&context)?;

    // Step 7: Serialize and write atomically
    let json_content =
        serde_json::to_string_pretty(&effect).context("Failed to serialize effect to JSON")?;
    atomic_write(&effect_file_path, json_content.as_bytes())?;

    // Step 8: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": effect.id,
                    "name": effect.name(),
                    "path": effect_file_path.to_string_lossy(),
                    "updated_fields": updated_fields,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Effect '{}' updated successfully at {}",
                    name,
                    effect_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Apply flag updates to an effect (non-interactive mode).
fn apply_flag_updates(
    effect: &mut Effect,
    effect_type: Option<String>,
    param: Option<Vec<f32>>,
) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    if let Some(et) = effect_type {
        let trimmed = et.trim().to_string();
        if trimmed.is_empty() {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Effect type cannot be whitespace-only",
                "Provide a non-empty effect type or omit the flag",
            )
            .into());
        }
        effect.effect = Some(trimmed);
        updated_fields.push("effect_type".to_string());
    }

    if let Some(params) = param {
        effect.parameters = Some(
            params
                .iter()
                .map(|&v| RtpcCompatibleValue::static_value(v))
                .collect(),
        );
        updated_fields.push("parameters".to_string());
    }

    Ok(updated_fields)
}

/// Prompt for effect updates in interactive mode.
fn prompt_effect_updates(effect: &mut Effect, input: &dyn Input) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    // Prompt for effect type
    let current_effect_type = effect.effect.as_deref().unwrap_or("").to_string();
    let prompt = format!(
        "Effect type (current: {}, Enter to keep)",
        if current_effect_type.is_empty() {
            "none"
        } else {
            &current_effect_type
        }
    );

    if let Ok(value) = input.prompt_text(
        &prompt,
        Some(if current_effect_type.is_empty() {
            ""
        } else {
            &current_effect_type
        }),
        None,
        None,
    ) {
        let trimmed = value.trim().to_string();
        if trimmed != current_effect_type {
            effect.effect = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            };
            updated_fields.push("effect_type".to_string());
        }
    }

    // Prompt for parameters
    let current_params_str = effect
        .parameters
        .as_ref()
        .map(|params| {
            params
                .iter()
                .map(|p| {
                    p.as_static()
                        .map(|v| format!("{}", v))
                        .unwrap_or_else(|| "RTPC".to_string())
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    let param_prompt = format!(
        "Parameters (comma-separated floats, current: {}, Enter to keep)",
        if current_params_str.is_empty() {
            "none"
        } else {
            &current_params_str
        }
    );

    if let Ok(value) = input.prompt_text(
        &param_prompt,
        Some(if current_params_str.is_empty() {
            ""
        } else {
            &current_params_str
        }),
        None,
        Some(&|value: &str| {
            if value.trim().is_empty() {
                return Ok(Validation::Valid);
            }
            for part in value.split(',') {
                if part.trim().parse::<f32>().is_err() {
                    return Ok(Validation::Invalid(
                        format!("'{}' is not a valid number", part.trim()).into(),
                    ));
                }
            }
            Ok(Validation::Valid)
        }),
    ) {
        let trimmed = value.trim().to_string();
        if trimmed != current_params_str {
            if trimmed.is_empty() {
                effect.parameters = None;
            } else {
                let parsed: Vec<RtpcCompatibleValue> = trimmed
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| {
                        CliError::new(
                            codes::ERR_VALIDATION_FIELD,
                            format!("Invalid parameter value: {}", e),
                            "All parameters must be valid finite numbers",
                        )
                    })?
                    .into_iter()
                    .map(RtpcCompatibleValue::static_value)
                    .collect();
                effect.parameters = Some(parsed);
            }
            updated_fields.push("parameters".to_string());
        }
    }

    Ok(updated_fields)
}

// =============================================================================
// Delete
// =============================================================================

/// Delete an effect asset.
async fn delete_effect(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Locate effect file
    let effects_dir = current_dir.join("sources").join("effects");
    let effect_file_path = effects_dir.join(format!("{}.json", name));

    if !effect_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset effect list' to see available effects")
            .into());
    }

    // Step 3: Read effect for response data
    let content = fs::read_to_string(&effect_file_path).context(format!(
        "Failed to read effect file: {}",
        effect_file_path.display()
    ))?;
    let effect: Effect = serde_json::from_str(&content).context(format!(
        "Failed to parse effect file: {}",
        effect_file_path.display()
    ))?;

    // Step 4: Confirm deletion
    let confirmed = if force {
        true
    } else {
        match input.confirm(
            &format!("Delete effect '{}'? This cannot be undone.", name),
            Some(false),
        ) {
            Ok(value) => value,
            Err(_) => {
                // Non-interactive mode without --force
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Deletion requires confirmation",
                    "In non-interactive mode, use --force to confirm deletion",
                )
                .with_suggestion(format!(
                    "Use 'am asset effect delete {} --force' to delete without prompting",
                    name
                ))
                .into());
            }
        }
    };

    if !confirmed {
        output.progress("Deletion cancelled.");
        return Ok(());
    }

    // Step 5: Remove file
    fs::remove_file(&effect_file_path).context(format!(
        "Failed to delete effect file: {}",
        effect_file_path.display()
    ))?;

    // Step 6: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": effect.id,
                    "name": effect.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Effect '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}

// =============================================================================
// Shared prompt helpers
// =============================================================================

/// Prompt for effect type.
///
/// In non-interactive mode, defaults to None (per AC2: sensible defaults).
fn prompt_effect_type(input: &dyn Input) -> Result<Option<String>> {
    let result = input.prompt_text("Effect type (e.g., reverb, eq)", Some("reverb"), None, None);

    match result {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(_) => {
            log::debug!("Non-interactive mode: using default effect type None");
            Ok(None)
        }
    }
}

/// Prompt for parameters.
///
/// In non-interactive mode, defaults to None (per AC2: sensible defaults).
fn prompt_parameters(input: &dyn Input) -> Result<Option<Vec<RtpcCompatibleValue>>> {
    let result = input.prompt_text(
        "Parameter values (comma-separated floats)",
        Some("0.8, 0.5"),
        None,
        Some(&|value: &str| {
            if value.trim().is_empty() {
                return Ok(Validation::Valid);
            }
            for part in value.split(',') {
                match part.trim().parse::<f32>() {
                    Ok(v) if v.is_finite() => {}
                    Ok(_) => {
                        return Ok(Validation::Invalid(
                            "Parameter values must be finite numbers".into(),
                        ));
                    }
                    Err(_) => {
                        return Ok(Validation::Invalid(
                            format!("'{}' is not a valid number", part.trim()).into(),
                        ));
                    }
                }
            }
            Ok(Validation::Valid)
        }),
    );

    match result {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                let params: Vec<RtpcCompatibleValue> = trimmed
                    .split(',')
                    .map(|s| s.trim().parse::<f32>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| {
                        CliError::new(
                            codes::ERR_VALIDATION_FIELD,
                            format!("Invalid parameter value: {}", e),
                            "All parameters must be valid finite numbers",
                        )
                    })?
                    .into_iter()
                    .map(RtpcCompatibleValue::static_value)
                    .collect();
                if params.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(params))
                }
            }
        }
        Err(_) => {
            log::debug!("Non-interactive mode: using default parameters None");
            Ok(None)
        }
    }
}
