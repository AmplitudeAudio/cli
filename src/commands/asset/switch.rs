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

//! Switch asset commands.
//!
//! Implements CRUD operations for Switch assets in Amplitude projects.

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
    assets::{
        Asset, AssetType, ProjectContext, ProjectValidator, Switch,
        generated::SwitchStateDefinition,
    },
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        utils::read_amproject_file,
    },
    database::Database,
    input::{Input, select_index},
    presentation::{Output, OutputMode},
};

/// Recursively find all .json files in a directory.
fn find_json_files_recursive(dir: &std::path::Path) -> anyhow::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    
    if !dir.exists() {
        return Ok(files);
    }
    
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "json").unwrap_or(false) {
            files.push(path.to_path_buf());
        }
    }
    
    Ok(files)
}

/// The name of the current asset.
const ASSET_NAME: &str = "Switch";

/// Maximum number of ID generation retries before giving up.
const MAX_ID_RETRIES: u32 = 3;

/// Switch asset subcommands.
#[derive(Subcommand, Debug)]
pub enum SwitchCommands {
    /// Create a new switch asset
    #[command(
        after_help = "Examples:\n  am asset switch create surface_type\n  am asset switch create surface_type --states wood,stone,metal\n"
    )]
    Create {
        /// Name of the switch asset
        name: String,

        /// State names (comma-separated for non-interactive mode)
        #[arg(long, value_delimiter = ',')]
        states: Option<Vec<String>>,
    },

    /// List all switch assets in the project
    #[command(after_help = "Examples:\n  am asset switch list\n")]
    List {},

    /// Update an existing switch asset
    #[command(
        after_help = "Examples:\n  am asset switch update surface_type\n  am asset switch update surface_type --states wood,stone,grass\n"
    )]
    Update {
        /// Name of the switch asset to update
        name: String,

        /// New state names (comma-separated, replaces existing states)
        #[arg(long, value_delimiter = ',')]
        states: Option<Vec<String>>,
    },

    /// Delete a switch asset
    #[command(
        after_help = "Examples:\n  am asset switch delete surface_type\n  am asset switch delete surface_type --force\n"
    )]
    Delete {
        /// Name of the switch asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle switch commands by routing to the appropriate handler.
pub async fn handler(
    command: &SwitchCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        SwitchCommands::Create { name, states } => {
            create_switch(name, states.clone(), input, output).await
        }
        SwitchCommands::List {} => list_switches(output).await,
        SwitchCommands::Update { name, states } => {
            update_switch(name, states.clone(), input, output).await
        }
        SwitchCommands::Delete { name, force } => delete_switch(name, *force, input, output).await,
    }
}

// =============================================================================
// Create
// =============================================================================

/// Create a new switch asset.
async fn create_switch(
    name: &str,
    states: Option<Vec<String>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Validate name is not empty
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Switch name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset switch create <name>'")
        .into());
    }

    // Step 2: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating switch '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 3: Validate switch name doesn't already exist
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let switches_dir = sources_base.join("switches");
    let switch_file_path = switches_dir.join(format!("{}.json", name));

    if switch_file_path.exists() {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset switch update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Build populated ProjectContext for validation
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Check name uniqueness via ProjectContext registry
    if context.has_name(AssetType::Switch, name) {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset switch update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Step 4: Get states
    let state_list = if let Some(s) = states {
        // Non-interactive mode: parse comma-separated states
        parse_state_names(&s)?
    } else {
        // Interactive mode: prompt for states one by one
        prompt_states(input)?
    };

    // Validate at least one state
    if state_list.is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Switch must have at least one state",
            "At least one state is required for a switch",
        )
        .with_suggestion("Provide states via --states flag or use interactive mode")
        .into());
    }

    // Validate no duplicate state names
    let mut seen_names = std::collections::HashSet::new();
    for state_name in &state_list {
        if !seen_names.insert(state_name.as_str()) {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("Duplicate state name: '{}'", state_name),
                "Each state name must be unique within the switch",
            )
            .with_suggestion("Remove duplicate state names")
            .into());
        }
    }

    // Step 5: Generate unique ID for switch
    let mut switch_id = generate_unique_id(name);
    let mut retries = 0;
    while context.has_id(switch_id) && retries < MAX_ID_RETRIES {
        switch_id = generate_unique_id(&format!("{}{}", name, retries));
        retries += 1;
    }
    if context.has_id(switch_id) {
        return Err(CliError::new(
            codes::ERR_ASSET_ALREADY_EXISTS,
            format!("Generated ID {} collides with an existing asset", switch_id),
            "All generated ID attempts collided with existing assets in the project",
        )
        .with_suggestion("Try a different name or wait a moment and retry")
        .into());
    }

    // Step 6: Generate unique IDs for each state
    let mut state_definitions = Vec::new();
    for state_name in &state_list {
        let mut state_id = generate_unique_id(&format!("{}_{}", name, state_name));
        let mut retries = 0;
        while (context.has_id(state_id)
            || state_definitions
                .iter()
                .any(|s: &SwitchStateDefinition| s.id == state_id))
            && retries < MAX_ID_RETRIES
        {
            state_id = generate_unique_id(&format!("{}_{}_{}", name, state_name, retries));
            retries += 1;
        }
        state_definitions.push(SwitchStateDefinition {
            id: state_id,
            name: Some(state_name.clone()),
        });
    }

    // Step 7: Build the Switch asset
    let switch = Switch::builder(switch_id, name)
        .states(state_definitions)
        .build();

    // Step 8: Validate
    switch.validate_rules(&context)?;

    // Step 9: Serialize to JSON
    let json_content =
        serde_json::to_string_pretty(&switch).context("Failed to serialize switch to JSON")?;

    // Step 10: Ensure directory exists and write atomically
    fs::create_dir_all(&switches_dir)?;
    atomic_write(&switch_file_path, json_content.as_bytes())?;

    // Step 11: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": switch.id,
                    "name": switch.name(),
                    "path": switch_file_path.to_string_lossy(),
                    "state_count": state_list.len(),
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Switch '{}' created successfully at {}",
                    name,
                    switch_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Parse state names from comma-separated list.
fn parse_state_names(states: &[String]) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for state in states {
        let trimmed = state.trim();
        if !trimmed.is_empty() {
            result.push(trimmed.to_string());
        }
    }
    Ok(result)
}

/// Prompt for states in interactive mode.
fn prompt_states(input: &dyn Input) -> Result<Vec<String>> {
    let mut states = Vec::new();

    loop {
        let prompt = if states.is_empty() {
            "Enter state name (e.g., 'wood', 'stone'):".to_string()
        } else {
            format!("Enter state name (current: {}):", states.join(", "))
        };

        let state_name = input.prompt_text(
            &prompt,
            None,
            None,
            Some(&|value: &str| {
                if value.trim().is_empty() && states.is_empty() {
                    return Ok(Validation::Invalid("At least one state is required".into()));
                }
                Ok(Validation::Valid)
            }),
        )?;

        let trimmed = state_name.trim();
        if trimmed.is_empty() {
            if states.is_empty() {
                continue;
            } else {
                break;
            }
        }

        // Check for duplicates
        if states.contains(&trimmed.to_string()) {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("Duplicate state name: '{}'", trimmed),
                "Each state name must be unique within the switch",
            )
            .with_suggestion("Choose a different name for this state")
            .into());
        }

        states.push(trimmed.to_string());

        // Ask if user wants to add another state
        match input.confirm("Add another state?", Some(true)) {
            Ok(true) => continue,
            Ok(false) => break,
            Err(_) => {
                // Non-interactive mode - stop after first state if no more input
                if states.len() >= 1 {
                    break;
                }
                continue;
            }
        }
    }

    Ok(states)
}

// =============================================================================
// List
// =============================================================================

/// List all switch assets in the current project.
async fn list_switches(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Scan switches directory
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let switches_dir = sources_base.join("switches");

    // Step 3: Handle missing directory
    if !switches_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "switches": [],
                        "count": 0,
                        "warnings": ["No switches directory found. Create switches with 'am asset switch create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No switches directory found.");
                output.progress(&format!(
                    "Create switches with '{}'.",
                    "am asset switch create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 4: Read and parse all .json files recursively
    let mut switches: Vec<Switch> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let json_files = match find_json_files_recursive(&switches_dir) {
        Ok(files) => files,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read switches directory",
                format!("Permission denied on {}", switches_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for path in json_files {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Switch>(&content) {
                    Ok(switch) => {
                        switches.push(switch);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        output.warning(&format!("Skipping invalid switch file: {}", path.display()));
                        // Provide more context for JSON errors
                        let error_msg = if let Some(line) = content.lines().next() {
                            if e.to_string().contains("column") {
                                format!("Invalid JSON in {}: {}. First line: {}", filename, e, &line[..line.len().min(200)])
                            } else {
                                format!("Invalid JSON in {}: {}", filename, e)
                            }
                        } else {
                            format!("Invalid JSON in {}: {}", filename, e)
                        };
                        warnings.push(error_msg);
                    }
                },
                Err(e) => {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    output.warning(&format!("Failed to read switch file: {}", path.display()));
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 5: Sort by name
    switches.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 6: Handle empty
    if switches.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "switches": [],
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
                output.progress("No switches found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset switch create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 7: Output
    match output.mode() {
        OutputMode::Json => {
            let switch_data: Vec<serde_json::Value> = switches
                .iter()
                .map(|s| {
                    let state_count = s.states.as_ref().map(|v| v.len()).unwrap_or(0);
                    json!({
                        "id": s.id,
                        "name": s.name(),
                        "state_count": state_count,
                        "states": s.states.as_ref().map(|states| {
                            states.iter().map(|st| {
                                json!({
                                    "id": st.id,
                                    "name": st.name.as_deref().unwrap_or("")
                                })
                            }).collect::<Vec<_>>()
                        }).unwrap_or_default()
                    })
                })
                .collect();

            output.success(
                json!({
                    "switches": switch_data,
                    "count": switches.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = switches
                .iter()
                .map(|s| {
                    let state_count = s.states.as_ref().map(|v| v.len()).unwrap_or(0);
                    json!({
                        "id": s.id,
                        "name": s.name(),
                        "states": state_count,
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} switch(es) found", switches.len()));
        }
    }

    Ok(())
}

// =============================================================================
// Update
// =============================================================================

/// Update an existing switch asset.
async fn update_switch(
    name: &str,
    states: Option<Vec<String>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating switch '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Locate existing switch file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let switches_dir = sources_base.join("switches");
    let switch_file_path = switches_dir.join(format!("{}.json", name));

    if !switch_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset switch list' to see available switches, or 'am asset switch create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing switch
    let content = fs::read_to_string(&switch_file_path).context(format!(
        "Failed to read switch file: {}",
        switch_file_path.display()
    ))?;
    let mut switch: Switch = serde_json::from_str(&content).context(format!(
        "Failed to parse switch file: {}",
        switch_file_path.display()
    ))?;

    // Step 4: Determine if we have any flag values (non-interactive mode)
    let has_any_flag = states.is_some();

    // Step 5: Apply updates
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    let updated_fields: Vec<String> = if has_any_flag {
        // Non-interactive mode: only update fields provided via flags
        apply_flag_updates(&mut switch, states, &context)?
    } else {
        // Interactive mode: prompt for updates
        prompt_switch_updates(&mut switch, input)?
    };

    // Step 6: Validate
    switch.validate_rules(&context)?;

    // Step 7: Serialize and write atomically
    let json_content =
        serde_json::to_string_pretty(&switch).context("Failed to serialize switch to JSON")?;
    atomic_write(&switch_file_path, json_content.as_bytes())?;

    // Step 8: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": switch.id,
                    "name": switch.name(),
                    "path": switch_file_path.to_string_lossy(),
                    "updated_fields": updated_fields,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Switch '{}' updated successfully at {}",
                    name,
                    switch_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Apply flag updates to a switch (non-interactive mode).
fn apply_flag_updates(
    switch: &mut Switch,
    states: Option<Vec<String>>,
    context: &ProjectContext,
) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    if let Some(new_states) = states {
        let state_list = parse_state_names(&new_states)?;

        // Validate at least one state
        if state_list.is_empty() {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Switch must have at least one state",
                "Cannot remove all states from a switch",
            )
            .with_suggestion("Provide at least one state name")
            .into());
        }

        // Validate no duplicates
        let mut seen_names = std::collections::HashSet::new();
        for state_name in &state_list {
            if !seen_names.insert(state_name.as_str()) {
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    format!("Duplicate state name: '{}'", state_name),
                    "Each state name must be unique within the switch",
                )
                .into());
            }
        }

        // Generate new state definitions with collision check
        let mut state_definitions = Vec::new();
        let mut used_ids = std::collections::HashSet::new();
        for state_name in &state_list {
            let mut state_id = generate_unique_id(&format!("{}_{}", switch.name(), state_name));
            let mut retries = 0;
            while (used_ids.contains(&state_id) || context.has_id(state_id))
                && retries < MAX_ID_RETRIES
            {
                state_id =
                    generate_unique_id(&format!("{}_{}_{}", switch.name(), state_name, retries));
                retries += 1;
            }
            if used_ids.contains(&state_id) || context.has_id(state_id) {
                return Err(CliError::new(
                    codes::ERR_ASSET_ALREADY_EXISTS,
                    format!(
                        "Generated state ID {} collides with existing asset",
                        state_id
                    ),
                    "All generated ID attempts collided with existing assets",
                )
                .with_suggestion("Try a different state name or wait and retry")
                .into());
            }
            used_ids.insert(state_id);
            state_definitions.push(SwitchStateDefinition {
                id: state_id,
                name: Some(state_name.clone()),
            });
        }

        switch.states = Some(state_definitions);
        updated_fields.push("states".to_string());
    }

    Ok(updated_fields)
}

/// Prompt for switch updates in interactive mode.
fn prompt_switch_updates(switch: &mut Switch, input: &dyn Input) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    // Get current state names
    let current_states: Vec<String> = switch
        .states
        .as_ref()
        .map(|s| s.iter().filter_map(|st| st.name.clone()).collect())
        .unwrap_or_default();

    // Prompt to modify states
    match input.confirm(
        &format!("Modify states? (current: {})", current_states.join(", ")),
        Some(false),
    ) {
        Ok(true) => {
            let new_states = prompt_states_for_update(input, &current_states)?;
            if !new_states.is_empty() && new_states != current_states {
                // Generate new state definitions with collision check
                let mut state_definitions = Vec::new();
                let mut used_ids = std::collections::HashSet::new();
                for state_name in &new_states {
                    let mut state_id =
                        generate_unique_id(&format!("{}_{}", switch.name(), state_name));
                    let mut retries = 0;
                    while used_ids.contains(&state_id) && retries < MAX_ID_RETRIES {
                        state_id = generate_unique_id(&format!(
                            "{}_{}_{}",
                            switch.name(),
                            state_name,
                            retries
                        ));
                        retries += 1;
                    }
                    if used_ids.contains(&state_id) {
                        return Err(CliError::new(
                            codes::ERR_ASSET_ALREADY_EXISTS,
                            format!(
                                "Generated state ID {} collides within this switch",
                                state_id
                            ),
                            "Duplicate state ID generated",
                        )
                        .into());
                    }
                    used_ids.insert(state_id);
                    state_definitions.push(SwitchStateDefinition {
                        id: state_id,
                        name: Some(state_name.clone()),
                    });
                }
                switch.states = Some(state_definitions);
                updated_fields.push("states".to_string());
            }
        }
        _ => {}
    }

    Ok(updated_fields)
}

/// Prompt for state updates in interactive mode.
fn prompt_states_for_update(input: &dyn Input, current_states: &[String]) -> Result<Vec<String>> {
    let mut states = current_states.to_vec();

    loop {
        let prompt = format!(
            "Current states: {}\nOptions: (a)dd, (r)emove, (d)one",
            states.join(", ")
        );

        let choice = input.prompt_text(
            &prompt,
            Some("d"),
            None,
            Some(&|value: &str| {
                let trimmed = value.trim().to_lowercase();
                if trimmed.is_empty()
                    || ["a", "add", "r", "remove", "d", "done"].contains(&trimmed.as_str())
                {
                    Ok(Validation::Valid)
                } else {
                    Ok(Validation::Invalid(
                        "Enter 'a' to add, 'r' to remove, or 'd' to done".into(),
                    ))
                }
            }),
        )?;

        match choice.trim().to_lowercase().as_str() {
            "a" | "add" => {
                let new_state = input.prompt_text(
                    "Enter new state name:",
                    None,
                    None,
                    Some(&|value: &str| {
                        if value.trim().is_empty() {
                            return Ok(Validation::Invalid("State name cannot be empty".into()));
                        }
                        Ok(Validation::Valid)
                    }),
                )?;
                let trimmed = new_state.trim();
                if states.contains(&trimmed.to_string()) {
                    return Err(CliError::new(
                        codes::ERR_VALIDATION_FIELD,
                        format!("Duplicate state name: '{}'", trimmed),
                        "Each state name must be unique within the switch",
                    )
                    .with_suggestion("Choose a different name for this state")
                    .into());
                }
                states.push(trimmed.to_string());
            }
            "r" | "remove" => {
                if states.len() <= 1 {
                    return Err(CliError::new(
                        codes::ERR_VALIDATION_FIELD,
                        "Cannot remove the last state",
                        "A switch must have at least one state",
                    )
                    .into());
                }
                // In non-interactive mode, select_index will fail - we should not silently ignore
                let idx = select_index(input, "Select state to remove:", &states)?;
                states.remove(idx);
            }
            _ => break,
        }
    }

    Ok(states)
}

// =============================================================================
// Delete
// =============================================================================

/// Delete a switch asset.
async fn delete_switch(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    // Step 2: Locate switch file
    let sources_base = if project_config.sources_dir.is_empty() {
        current_dir.clone()
    } else {
        current_dir.join(&project_config.sources_dir)
    };
    let switches_dir = sources_base.join("switches");
    let switch_file_path = switches_dir.join(format!("{}.json", name));

    if !switch_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset switch list' to see available switches")
            .into());
    }

    // Step 3: Read switch for response data
    let content = fs::read_to_string(&switch_file_path).context(format!(
        "Failed to read switch file: {}",
        switch_file_path.display()
    ))?;
    let switch: Switch = serde_json::from_str(&content).context(format!(
        "Failed to parse switch file: {}",
        switch_file_path.display()
    ))?;

    // Step 4: Check for switch container references
    let validator = ProjectValidator::new(current_dir.clone(), output)?;
    let has_references = check_switch_references(&validator, switch.id);

    // Step 5: Confirm deletion
    let confirmed = if force {
        true
    } else {
        let mut prompt = format!("Delete switch '{}'? This cannot be undone.", name);
        if has_references {
            prompt.push_str("\n⚠️  Warning: This switch is referenced by switch containers.");
        }

        match input.confirm(&prompt, Some(false)) {
            Ok(value) => value,
            Err(_) => {
                // Non-interactive mode without --force
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Deletion requires confirmation",
                    "In non-interactive mode, use --force to confirm deletion",
                )
                .with_suggestion(format!(
                    "Use 'am asset switch delete {} --force' to delete without prompting",
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

    // Step 6: Remove file
    fs::remove_file(&switch_file_path).context(format!(
        "Failed to delete switch file: {}",
        switch_file_path.display()
    ))?;

    // Step 7: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": switch.id,
                    "name": switch.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Switch '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}

/// Check if a switch is referenced by any switch containers.
fn check_switch_references(_validator: &ProjectValidator, _switch_id: u64) -> bool {
    // For now, return false as switch container implementation is in Story 4.3
    // This will be enhanced when switch containers are implemented
    false
}
