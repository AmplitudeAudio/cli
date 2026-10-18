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

//! Pipeline asset commands.
//!
//! Implements CRUD operations for pipeline assets in Amplitude projects. The
//! create/update handlers only gather nodes (flags, then prompts when the
//! session is interactive); building and merging lives in `services::pipeline`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;

use crate::{
    assets::{
        Asset, AssetType, Pipeline, ProjectContext, ProjectValidator,
        engine_config::pipeline_referrers,
    },
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        json_merge::merge_update_unknown_only,
        utils::read_amproject_file,
    },
    database::Database,
    forms::coverage::known_field_names,
    forms::in_use::assert_not_in_use,
    input::Input,
    presentation::{Output, OutputMode},
    services::pipeline::{PipelineValues, apply_patch, build_pipeline},
};

use super::find_json_files_recursive;
use super::pipeline_args::{
    NodeArgs, UpdateArgs, missing_nodes_error, nodes_from_flags, resolve_patch,
};
use super::pipeline_prompts::{prompt_new_nodes, prompt_update_nodes};

/// The name of the current asset, used in error messages and output.
const ASSET_NAME: &str = "Pipeline";

/// Pipeline asset subcommands.
#[derive(Subcommand, Debug)]
pub enum PipelineCommands {
    /// Create a new pipeline asset
    #[command(after_help = "Examples:
  am asset pipeline create main --node 1:Input --node 2:Reverb:1:0.5,0.25 --node 3:Output:2
  am asset pipeline create main --from-json nodes.json

Node syntax: ID:Type[:CONSUMED_IDS[:PARAMETERS]] (see --node in --help).
A pipeline needs exactly one Input and one Output node.
")]
    Create {
        /// Name of the pipeline asset
        name: String,

        #[command(flatten)]
        args: NodeArgs,
    },

    /// List all pipeline assets in the project
    #[command(after_help = "Examples:
  am asset pipeline list
  am asset pipeline list --json
")]
    List {},

    /// Update an existing pipeline asset (only the flags you pass change)
    #[command(after_help = "Examples:
  am asset pipeline update main --add-node 4:Clamp:2
  am asset pipeline update main --remove-node 4
  am asset pipeline update main --node 1:Input --node 2:Output:1   (replaces all nodes)
")]
    Update {
        /// Name of the pipeline asset to update
        name: String,

        #[command(flatten)]
        args: UpdateArgs,
    },

    /// Delete a pipeline asset (refused while an asset or an engine config references it)
    #[command(after_help = "Examples:
  am asset pipeline delete main
  am asset pipeline delete main --force

The delete is refused, even with --force, while an engine config
('*.config.json') names the pipeline as its mixer pipeline.
")]
    Delete {
        /// Name of the pipeline asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle pipeline commands by routing to the appropriate handler.
pub async fn handler(
    command: &PipelineCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        PipelineCommands::Create { name, args } => create_pipeline(name, args, input, output),
        PipelineCommands::List {} => list_pipelines(output),
        PipelineCommands::Update { name, args } => update_pipeline(name, args, input, output),
        PipelineCommands::Delete { name, force } => delete_pipeline(name, *force, input, output),
    }
}

/// Whether this session may ask the user questions (never in JSON or non-interactive runs).
fn can_prompt(input: &dyn Input, output: &dyn Output) -> bool {
    input.is_interactive() && output.mode() != OutputMode::Json
}

/// The directory pipelines live in, plus the project's root and name.
fn pipelines_dir() -> Result<(PathBuf, PathBuf, String)> {
    let current_dir = env::current_dir()?;
    let project = read_amproject_file(&current_dir)?;
    let sources_base = if project.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project.sources_dir)
    };
    let dir = sources_base.join(AssetType::Pipeline.directory_name());
    Ok((current_dir, dir, project.name))
}

fn project_context(current_dir: PathBuf, output: &dyn Output) -> Result<ProjectContext> {
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    Ok(ProjectContext::new(current_dir).with_validator(validator))
}

/// Tells the author about node types the SDK only knows once a plugin is loaded.
fn plugin_node_warnings(pipeline: &Pipeline, output: &dyn Output) -> Vec<String> {
    let warnings: Vec<String> = pipeline
        .unknown_node_types()
        .into_iter()
        .map(|name| {
            format!(
                "Node type '{name}' is not an SDK built-in; make sure the plugin providing it is loaded before the engine"
            )
        })
        .collect();
    for warning in &warnings {
        output.warning(warning);
    }
    warnings
}

fn write_pipeline(path: &std::path::Path, value: &serde_json::Value) -> Result<()> {
    let content =
        serde_json::to_string_pretty(value).context("Failed to serialize pipeline to JSON")?;
    atomic_write(path, content.as_bytes())?;
    Ok(())
}

/// Create a new pipeline asset.
fn create_pipeline(
    name: &str,
    args: &NodeArgs,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let (current_dir, dir, project_name) = pipelines_dir()?;
    output.progress(&format!(
        "Creating pipeline '{name}' in project '{project_name}'..."
    ));

    let file_path = dir.join(format!("{name}.json"));
    let already_exists = || {
        asset_already_exists(ASSET_NAME, name).with_suggestion(format!(
            "Use 'am asset pipeline update {name}' to modify it, or choose a different name"
        ))
    };
    if file_path.exists() {
        return Err(already_exists().into());
    }

    let context = project_context(current_dir, output)?;
    if context.has_name(AssetType::Pipeline, name) {
        return Err(already_exists().into());
    }

    // Flags first; without them the nodes are asked for, or reported by flag name.
    let nodes = match nodes_from_flags(args)? {
        Some(nodes) => nodes,
        None if can_prompt(input, output) => prompt_new_nodes(input)?,
        None => return Err(missing_nodes_error().into()),
    };

    let pipeline = build_pipeline(
        PipelineValues {
            name: name.to_string(),
            nodes,
        },
        &context,
    )?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
    write_pipeline(
        &file_path,
        &serde_json::to_value(&pipeline).context("Failed to serialize pipeline to JSON")?,
    )?;
    let warnings = plugin_node_warnings(&pipeline, output);

    match output.mode() {
        OutputMode::Json => output.success(
            json!({
                "id": pipeline.id,
                "name": pipeline.name(),
                "path": file_path.to_string_lossy(),
                "node_count": pipeline.nodes.as_ref().map_or(0, Vec::len),
                "warnings": warnings,
            }),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Pipeline '{name}' created successfully at {}",
                file_path.display()
            )),
            None,
        ),
    }
    Ok(())
}

/// Parses a pipeline file's JSON. A hand-written file may leave out the id,
/// which FlatBuffers (and so the SDK) reads as 0.
fn parse_pipeline(raw: &serde_json::Value) -> serde_json::Result<Pipeline> {
    let mut value = raw.clone();
    if let Some(fields) = value.as_object_mut() {
        fields.entry("id").or_insert_with(|| json!(0));
    }
    serde_json::from_value(value)
}

fn read_pipelines(
    dir: &std::path::Path,
    output: &dyn Output,
) -> Result<(Vec<Pipeline>, Vec<String>)> {
    let mut pipelines = Vec::new();
    let mut warnings = Vec::new();
    for path in find_json_files_recursive(dir)? {
        let parsed = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
            .and_then(|raw| parse_pipeline(&raw).map_err(|e| e.to_string()));
        match parsed {
            Ok(pipeline) => pipelines.push(pipeline),
            Err(reason) => {
                output.warning(&format!(
                    "Skipping invalid pipeline file: {}",
                    path.display()
                ));
                warnings.push(format!(
                    "Invalid pipeline file {}: {reason}",
                    path.display()
                ));
            }
        }
    }
    pipelines.sort_by(|a, b| a.name().cmp(b.name()));
    Ok((pipelines, warnings))
}

fn node_summary(pipeline: &Pipeline) -> String {
    pipeline
        .nodes
        .iter()
        .flatten()
        .map(|n| n.name.as_deref().unwrap_or("?"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// List all pipeline assets in the current project.
fn list_pipelines(output: &dyn Output) -> Result<()> {
    let (_, dir, _) = pipelines_dir()?;
    let (pipelines, warnings) = read_pipelines(&dir, output)?;
    let rows: Vec<_> = pipelines
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "name": p.name(),
                "node_count": p.nodes.as_ref().map_or(0, Vec::len),
                "nodes": node_summary(p),
            })
        })
        .collect();

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"pipelines": rows, "count": pipelines.len(), "warnings": warnings}),
            None,
        ),
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {warning}", "Warning:".yellow()));
            }
            if pipelines.is_empty() {
                output.progress("No pipelines found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset pipeline create <name>".green()
                ));
                return Ok(());
            }
            output.table(None, json!(rows));
            output.progress("");
            output.progress(&format!("{} pipeline(s) found", pipelines.len()));
        }
    }
    Ok(())
}

fn no_changes_error(name: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        "Nothing to update",
        "No flags were given and the session cannot prompt for values",
    )
    .with_suggestion(format!(
        "Pass at least one flag, e.g. 'am asset pipeline update {name} --add-node 4:Clamp:2'"
    ))
}

/// Finds the pipeline file; returns its path, the typed asset and the raw JSON.
fn locate(name: &str, dir: &std::path::Path) -> Result<(PathBuf, Pipeline, serde_json::Value)> {
    let path = dir.join(format!("{name}.json"));
    if !path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset pipeline list' to see available pipelines, or 'am asset pipeline create {name}' to create it"
            ))
            .into());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read pipeline file: {}", path.display()))?;
    let raw = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse pipeline file: {}", path.display()))?;
    let pipeline = parse_pipeline(&raw)
        .with_context(|| format!("Failed to read pipeline file: {}", path.display()))?;
    Ok((path, pipeline, raw))
}

/// Update an existing pipeline asset.
fn update_pipeline(
    name: &str,
    args: &UpdateArgs,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let (current_dir, dir, project_name) = pipelines_dir()?;
    output.progress(&format!(
        "Updating pipeline '{name}' in project '{project_name}'..."
    ));
    let (file_path, existing, raw) = locate(name, &dir)?;

    let mut patch = resolve_patch(args)?;
    if patch.is_empty() {
        if !can_prompt(input, output) {
            return Err(no_changes_error(name).into());
        }
        let current = existing.nodes.clone().unwrap_or_default();
        match prompt_update_nodes(input, &current)? {
            Some(nodes) => patch.replace = Some(nodes),
            None => {
                output.progress("No changes made.");
                return Ok(());
            }
        }
    }

    let context = project_context(current_dir, output)?;
    let pipeline = apply_patch(&existing, &patch)?;
    pipeline.validate_rules(&context)?;

    // Keys the CLI does not know about stay in the file.
    let known = known_field_names(AssetType::Pipeline);
    let known: Vec<&str> = known.iter().map(String::as_str).collect();
    let updated =
        serde_json::to_value(&pipeline).context("Failed to serialize pipeline to JSON")?;
    write_pipeline(
        &file_path,
        &merge_update_unknown_only(&raw, &updated, &known),
    )?;
    let warnings = plugin_node_warnings(&pipeline, output);

    match output.mode() {
        OutputMode::Json => output.success(
            json!({
                "id": pipeline.id,
                "name": pipeline.name(),
                "path": file_path.to_string_lossy(),
                "updated_fields": patch.changed_fields(),
                "warnings": warnings,
            }),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Pipeline '{name}' updated successfully at {}",
                file_path.display()
            )),
            None,
        ),
    }
    Ok(())
}

/// Delete a pipeline asset.
fn delete_pipeline(name: &str, force: bool, input: &dyn Input, output: &dyn Output) -> Result<()> {
    let (current_dir, dir, _) = pipelines_dir()?;
    let (file_path, pipeline, _) = locate(name, &dir)?;

    // Engine configs name their pipeline by file name (`mixer.pipeline` holds
    // `<name>.ampipeline`), so a config pointing at this file would be left dangling.
    // --force does not override this.
    let configs = pipeline_referrers(&current_dir, name)?;
    if !configs.is_empty() {
        return Err(CliError::new(
            codes::ERR_ASSET_IN_USE,
            format!("Pipeline '{name}' is still referenced"),
            format!("It is the mixer pipeline of: {}", configs.join(", ")),
        )
        .with_suggestion(
            "Point those engine configs at another pipeline first (am asset engine-config update <file> --mixer-pipeline <name>), then delete this one",
        )
        .into());
    }

    // Assets that reference the pipeline by id would be left dangling, so --force does
    // not override this. No asset type references pipelines by id today, so this finds nothing.
    let validator = ProjectValidator::new(current_dir, output)?;
    assert_not_in_use(&validator, AssetType::Pipeline, pipeline.id, name)?;

    let confirmed = force
        || input
            .confirm(
                &format!("Delete pipeline '{name}'? This cannot be undone."),
                Some(false),
            )
            .map_err(|_| {
                CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Deletion requires confirmation",
                    "In non-interactive mode, use --force to confirm deletion",
                )
                .with_suggestion(format!(
                    "Use 'am asset pipeline delete {name} --force' to delete without prompting"
                ))
            })?;
    if !confirmed {
        output.progress("Deletion cancelled.");
        return Ok(());
    }

    fs::remove_file(&file_path)
        .with_context(|| format!("Failed to delete pipeline file: {}", file_path.display()))?;

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"id": pipeline.id, "name": pipeline.name(), "deleted": true}),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!("Pipeline '{name}' deleted successfully")),
            None,
        ),
    }
    Ok(())
}
