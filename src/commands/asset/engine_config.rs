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

//! Engine config commands: `am asset engine-config create|list|update|delete`.
//!
//! An engine config is a `<name>.config.json` file in the sources dir (the project root is read too); a
//! project may keep several (for example one per platform). The handlers only
//! gather values (flags, `--from-json`, then prompts when the session can ask);
//! building, merging and validation live in `services::engine_config`, and the
//! whole new state is validated before the file is written atomically.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::{Value, json};

use super::bus_shared::{can_prompt, confirm_deletion, project_root, validator};
use super::engine_config_args::{EngineConfigArgs, Override, as_pairs};
use super::engine_config_prompts::{Choices, complete_create, prompt_update};
use crate::assets::AssetType;
use crate::assets::bus::scan_bus_files;
use crate::assets::engine_config::{
    EngineConfigFile, EngineConfigScan, engine_config_path, normalize_file_name,
    scan_engine_configs, validate_engine_config,
};
use crate::common::errors::{CliError, asset_already_exists, asset_not_found, codes};
use crate::common::files::atomic_write;
use crate::database::Database;
use crate::forms::references::RefTarget;
use crate::input::Input;
use crate::presentation::{Output, OutputMode};
use crate::services::engine_config::{
    apply_update, build_config, check_references, config_warnings, summarize,
};

const ASSET_NAME: &str = "Engine config";

/// Engine config subcommands.
#[derive(Subcommand, Debug)]
pub enum EngineConfigCommands {
    /// Create an engine config file in the sources dir
    #[command(after_help = "Examples:
  am asset engine-config create pc --mixer-pipeline pc --mixer-active-channels 32 --buses-file main
  am asset engine-config create mobile --from-json template.json --output-buffer-size 512
  am asset engine-config create pc --mixer-pipeline pc --mixer-active-channels 32 \\
      --buses-file main --mixer-panning-mode BinauralLowQuality --hrtf-amir-file sphere.amir

Every scalar or enum field of the schema has a flag named --<section>-<field>
(for example --output-frequency, --game-track-rooms, --hrtf-hrir-sampling);
--buses-file and --driver sit at the top level. Required, with no default:
--mixer-pipeline and --buses-file (and --hrtf-amir-file once HRTF is used).
Everything else defaults to the schema default; note that --mixer-active-channels
defaults to 0, which gives the engine no audible channels (a warning is printed).

--mixer-pipeline and --buses-file take the NAME of an existing pipeline asset or
bus file and are stored the way the SDK loads them ('<name>.ampipeline',
'<name>.buses.ambus').

The obstruction/occlusion curves (game.obstruction, game.occlusion) have no flags:
give them with --from-json (a whole config); they default to the template curves.
Flags override values read from --from-json.
")]
    Create {
        /// Name of the file, without '.config.json'
        file_name: String,

        #[command(flatten)]
        args: EngineConfigArgs,
    },

    /// List the engine configs of the project with their key fields
    #[command(after_help = "Examples:
  am asset engine-config list
  am asset engine-config list --json
")]
    List {},

    /// Update an engine config (only the flags you pass change)
    #[command(after_help = "Examples:
  am asset engine-config update pc --output-buffer-size 512
  am asset engine-config update pc --mixer-pipeline mobile --driver ''
  am asset engine-config update pc --from-json curves.json

Keys the CLI does not model keep their value and position. Pass '' to
--driver, --mixer-resampler to remove them. With --from-json, each top-level
section of the file (output, mixer, game, ...) replaces the same section as a
whole, then flags apply on top.
")]
    Update {
        /// Name of the file, without '.config.json'
        file_name: String,

        #[command(flatten)]
        args: EngineConfigArgs,
    },

    /// Delete an engine config (refused for the last one)
    #[command(after_help = "Examples:
  am asset engine-config delete mobile
  am asset engine-config delete mobile --force
")]
    Delete {
        /// Name of the file, without '.config.json'
        file_name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle engine config commands by routing to the appropriate handler.
pub async fn handler(
    command: &EngineConfigCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        EngineConfigCommands::Create { file_name, args } => {
            create_config(file_name, args, input, output)
        }
        EngineConfigCommands::List {} => list_configs(output),
        EngineConfigCommands::Update { file_name, args } => {
            update_config(file_name, args, input, output)
        }
        EngineConfigCommands::Delete { file_name, force } => {
            delete_config(file_name, *force, input, output)
        }
    }
}

/// Prints the non-blocking warnings of `config` and returns them for the JSON result.
fn report_warnings(config: &Value, output: &dyn Output) -> Vec<String> {
    let warnings = config_warnings(config);
    for warning in &warnings {
        output.warning(warning);
    }
    warnings
}

/// Adds a `warnings` array to a JSON result when there are warnings.
fn with_warnings(mut data: Value, warnings: &[String]) -> Value {
    if let (false, Some(map)) = (warnings.is_empty(), data.as_object_mut()) {
        map.insert("warnings".to_string(), json!(warnings));
    }
    data
}

fn write_config(path: &Path, value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value).context("Failed to serialize engine config")?;
    atomic_write(path, text.as_bytes())
}

/// Names of the pipelines and bus files a reference can point at.
struct Existing {
    pipelines: Vec<String>,
    bus_files: Vec<String>,
}

impl Existing {
    fn load(root: &Path, output: &dyn Output) -> Result<Self> {
        let pipelines = validator(root, output)?
            .list(RefTarget::Asset(AssetType::Pipeline))
            .into_iter()
            .map(|p| p.name)
            .collect();
        let bus_files = scan_bus_files(root)?
            .files
            .into_iter()
            .map(|f| f.name)
            .collect();
        Ok(Self {
            pipelines,
            bus_files,
        })
    }

    fn choices(&self) -> Choices<'_> {
        Choices {
            pipelines: &self.pipelines,
            bus_files: &self.bus_files,
        }
    }
}

fn find_config<'a>(scan: &'a EngineConfigScan, name: &str) -> Result<&'a EngineConfigFile> {
    scan.files.iter().find(|f| f.name == name).ok_or_else(|| {
        asset_not_found(ASSET_NAME, name)
            .with_suggestion(
                "Use 'am asset engine-config list' to see the engine configs, or 'am asset engine-config create <file-name>'",
            )
            .into()
    })
}

fn create_config(
    file_name: &str,
    args: &EngineConfigArgs,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let root = project_root()?;
    let name = normalize_file_name(file_name)?;
    let path = engine_config_path(&root, &name);
    if path.exists() {
        return Err(asset_already_exists(ASSET_NAME, &name)
            .with_suggestion(format!(
                "Use 'am asset engine-config update {name}' to modify it, or choose a different name"
            ))
            .into());
    }
    output.progress(&format!("Creating engine config '{name}'..."));

    let base = args.json_document()?;
    let mut set: Vec<Override> = args.overrides()?;
    let existing = Existing::load(&root, output)?;
    if can_prompt(input, output) {
        set = complete_create(input, base.as_ref(), set, &existing.choices())?;
    }

    let config = build_config(base.as_ref(), &as_pairs(&set))?;
    check_references(&config, &existing.pipelines, &existing.bus_files)?;
    write_config(&path, &config)?;

    let warnings = report_warnings(&config, output);
    match output.mode() {
        OutputMode::Json => output.success(
            with_warnings(
                json!({
                    "file": name,
                    "path": path.to_string_lossy(),
                    "pipeline": config["mixer"]["pipeline"],
                    "buses_file": config["buses_file"],
                }),
                &warnings,
            ),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Engine config '{name}' created successfully at {}",
                path.display()
            )),
            None,
        ),
    }
    Ok(())
}

fn list_configs(output: &dyn Output) -> Result<()> {
    let root = project_root()?;
    let scan = scan_engine_configs(&root)?;
    let mut problems = scan.problems.clone();
    let rows: Vec<Value> = scan
        .files
        .iter()
        .map(|f| {
            if let Err(e) = validate_engine_config(&f.value) {
                problems.push(format!("{}.config.json: {e}", f.name));
            }
            let mut row = summarize(&f.value);
            if let Some(map) = row.as_object_mut() {
                map.insert("file".to_string(), json!(f.name));
                map.insert("path".to_string(), json!(f.path.to_string_lossy()));
            }
            row
        })
        .collect();

    match output.mode() {
        OutputMode::Json => output.success(
            json!({"files": rows, "count": rows.len(), "warnings": problems}),
            None,
        ),
        OutputMode::Interactive => {
            for problem in &problems {
                output.progress(&format!("{} {problem}", "Warning:".yellow()));
            }
            if rows.is_empty() {
                output.progress("No engine configs found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset engine-config create <file-name>".green()
                ));
                return Ok(());
            }
            output.table(None, json!(rows));
            output.progress("");
            output.progress(&format!("{} engine config(s) found", rows.len()));
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
        "Pass at least one flag, e.g. 'am asset engine-config update {name} --output-buffer-size 512'"
    ))
}

fn update_config(
    file_name: &str,
    args: &EngineConfigArgs,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let root = project_root()?;
    let name = normalize_file_name(file_name)?;
    let scan = scan_engine_configs(&root)?;
    let file = find_config(&scan, &name)?;
    output.progress(&format!("Updating engine config '{name}'..."));

    let sections = args.json_document()?;
    let mut set = args.overrides()?;
    let existing = Existing::load(&root, output)?;
    if set.is_empty() && sections.is_none() {
        if !can_prompt(input, output) {
            return Err(no_changes_error(&name).into());
        }
        set = prompt_update(input, &file.value, &existing.choices())?;
        if set.is_empty() {
            return Err(no_changes_error(&name).into());
        }
    }

    let updated = apply_update(&file.value, sections.as_ref(), &as_pairs(&set))?;
    if references_changed(&file.value, &updated) {
        check_references(&updated, &existing.pipelines, &existing.bus_files)?;
    }
    write_config(&file.path, &updated)?;

    let mut changed: Vec<String> = sections
        .iter()
        .filter_map(Value::as_object)
        .flat_map(|o| o.keys().cloned())
        .collect();
    changed.extend(set.iter().map(|(key, _)| key.clone()));
    let warnings = report_warnings(&updated, output);
    match output.mode() {
        OutputMode::Json => output.success(
            with_warnings(
                json!({
                    "file": name,
                    "path": file.path.to_string_lossy(),
                    "updated_fields": changed,
                }),
                &warnings,
            ),
            None,
        ),
        OutputMode::Interactive => output.success(
            json!(format!(
                "Engine config '{name}' updated successfully in {}",
                file.path.display()
            )),
            None,
        ),
    }
    Ok(())
}

/// Whether the pipeline or bus file reference differs between two configs.
fn references_changed(before: &Value, after: &Value) -> bool {
    before["mixer"]["pipeline"] != after["mixer"]["pipeline"]
        || before["buses_file"] != after["buses_file"]
}

fn delete_config(
    file_name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    let root = project_root()?;
    let name = normalize_file_name(file_name)?;
    let scan = scan_engine_configs(&root)?;
    let file = find_config(&scan, &name)?;

    if scan.files.len() <= 1 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Cannot delete '{name}': it is the last engine config of the project"),
            "The SDK engine needs an engine config to start; a project must keep at least one",
        )
        .with_suggestion(
            "Create another one first with 'am asset engine-config create <file-name>'",
        )
        .into());
    }

    let confirmed = confirm_deletion(
        input,
        &format!("Delete engine config '{name}'? This cannot be undone."),
        force,
        &format!("am asset engine-config delete {name}"),
    )?;
    if !confirmed {
        output.progress("Deletion cancelled.");
        return Ok(());
    }

    fs::remove_file(&file.path)
        .with_context(|| format!("Failed to delete engine config: {}", file.path.display()))?;

    match output.mode() {
        OutputMode::Json => output.success(json!({"file": name, "deleted": true}), None),
        OutputMode::Interactive => output.success(
            json!(format!("Engine config '{name}' deleted successfully")),
            None,
        ),
    }
    Ok(())
}
