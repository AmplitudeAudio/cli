//! Collection asset commands.
//!
//! Implements CRUD operations for Collection assets in Amplitude projects.

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
        Asset, Collection, CollectionPlayMode, ProjectContext, ProjectValidator,
        RtpcCompatibleValue, SoundSchedulerMode, Spatialization,
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

use super::parse_spatialization;

/// The name of the current asset.
const ASSET_NAME: &str = "Collection";

/// Maximum number of ID generation retries before giving up.
const MAX_ID_RETRIES: u32 = 3;

/// Collection asset subcommands.
#[derive(Subcommand, Debug)]
pub enum CollectionCommands {
    /// Create a new collection asset
    #[command(after_help = "Examples:
  am asset collection create footsteps
  am asset collection create ambience --play-mode PlayAll --gain 0.7
")]
    Create {
        /// Name of the collection asset
        name: String,

        /// Play mode: PlayOne or PlayAll (default: PlayOne)
        #[arg(long)]
        play_mode: Option<String>,

        /// Scheduler mode: Random or Sequence (default: Random)
        #[arg(long)]
        scheduler_mode: Option<String>,

        /// Volume gain (0.0-1.0, default: 1.0)
        #[arg(short, long)]
        gain: Option<f32>,

        /// Bus ID for audio routing (default: 0 = master)
        #[arg(short, long)]
        bus: Option<u64>,

        /// Playback priority (0-255, default: 128)
        #[arg(short, long)]
        priority: Option<u8>,

        /// Spatialization mode: none, position, position_orientation, hrtf (default: none)
        #[arg(short, long)]
        spatialization: Option<String>,
    },

    /// List all collection assets in the project
    #[command(after_help = "Examples:
  am asset collection list
  am asset collection list --json
")]
    List {},

    /// Update an existing collection asset
    #[command(after_help = "Examples:
  am asset collection update footsteps --play-mode PlayAll
  am asset collection update ambience --gain 0.5
")]
    Update {
        /// Name of the collection asset to update
        name: String,

        /// New play mode: PlayOne or PlayAll
        #[arg(long)]
        play_mode: Option<String>,

        /// New scheduler mode: Random or Sequence
        #[arg(long)]
        scheduler_mode: Option<String>,

        /// New volume gain (0.0-1.0)
        #[arg(short, long)]
        gain: Option<f32>,

        /// New bus ID for audio routing
        #[arg(short, long)]
        bus: Option<u64>,

        /// New playback priority (0-255)
        #[arg(short, long)]
        priority: Option<u8>,

        /// New spatialization mode: none, position, position_orientation, hrtf
        #[arg(short, long)]
        spatialization: Option<String>,
    },

    /// Delete a collection asset
    #[command(after_help = "Examples:
  am asset collection delete footsteps
  am asset collection delete footsteps --force
")]
    Delete {
        /// Name of the collection asset to delete
        name: String,

        /// Skip confirmation prompt (required in non-interactive mode)
        #[arg(long)]
        force: bool,
    },
}

/// Handle collection commands by routing to the appropriate handler.
pub async fn handler(
    command: &CollectionCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        CollectionCommands::Create {
            name,
            play_mode,
            scheduler_mode,
            gain,
            bus,
            priority,
            spatialization,
        } => {
            create_collection(
                name,
                play_mode.clone(),
                scheduler_mode.clone(),
                *gain,
                *bus,
                *priority,
                spatialization.clone(),
                input,
                output,
            )
            .await
        }
        CollectionCommands::List {} => list_collections(output).await,
        CollectionCommands::Update {
            name,
            play_mode,
            scheduler_mode,
            gain,
            bus,
            priority,
            spatialization,
        } => {
            update_collection(
                name,
                play_mode.clone(),
                scheduler_mode.clone(),
                *gain,
                *bus,
                *priority,
                spatialization.clone(),
                input,
                output,
            )
            .await
        }
        CollectionCommands::Delete { name, force } => {
            delete_collection(name, *force, input, output).await
        }
    }
}

// =============================================================================
// Create
// =============================================================================

/// Create a new collection asset.
#[allow(clippy::too_many_arguments)]
async fn create_collection(
    name: &str,
    play_mode: Option<String>,
    scheduler_mode: Option<String>,
    gain: Option<f32>,
    bus: Option<u64>,
    priority: Option<u8>,
    spatialization: Option<String>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Validate name is not empty
    if name.trim().is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Collection name is required",
            "A non-empty name must be provided",
        )
        .with_suggestion("Provide a name: 'am asset collection create <name>'")
        .into());
    }

    // Step 2: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating collection '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 3: Validate collection name doesn't already exist (filesystem + registry)
    let collections_dir = current_dir.join("sources").join("collections");
    let collection_file_path = collections_dir.join(format!("{}.json", name));

    if collection_file_path.exists() {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset collection update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Build populated ProjectContext for validation
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Check name uniqueness via ProjectContext registry
    if context.has_name(crate::assets::AssetType::Collection, name) {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset collection update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Step 3: Get play mode
    let play_mode_value = if let Some(pm) = play_mode {
        CollectionPlayMode::from_str(&pm)
            .map_err(|e| CliError::new(codes::ERR_VALIDATION_FIELD, e, "Invalid play mode value"))?
    } else {
        prompt_play_mode(input)?
    };

    // Step 4: Get scheduler mode
    let scheduler_mode_value = if let Some(sm) = scheduler_mode {
        SoundSchedulerMode::from_str(&sm).map_err(|e| {
            CliError::new(
                codes::ERR_VALIDATION_FIELD,
                e,
                "Invalid scheduler mode value",
            )
        })?
    } else {
        prompt_scheduler_mode(input)?
    };

    // Step 5: Get gain value
    let gain_value = if let Some(g) = gain {
        validate_gain(g)?;
        g
    } else {
        prompt_gain(input)?
    };

    // Step 6: Get priority value
    let priority_value = if let Some(p) = priority {
        p
    } else {
        prompt_priority(input)?
    };

    // Step 7: Get spatialization mode
    let spatialization_mode = if let Some(s) = spatialization {
        parse_spatialization(&s)?
    } else {
        prompt_spatialization(input)?
    };

    // Step 8: Get bus ID
    let bus_id = bus.unwrap_or(0);

    // Step 9: Generate unique ID with collision check
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

    // Step 10: Build the Collection asset
    let collection = Collection::builder(id, name)
        .play_mode(play_mode_value)
        .scheduler_mode(scheduler_mode_value)
        .gain(gain_value)
        .priority(priority_value)
        .spatialization(spatialization_mode)
        .bus(bus_id)
        .build();

    // Step 11: Validate
    collection.validate_rules(&context)?;

    // Step 12: Serialize to JSON
    let json_content = serde_json::to_string_pretty(&collection)
        .context("Failed to serialize collection to JSON")?;

    // Step 13: Ensure directory exists and write atomically
    fs::create_dir_all(&collections_dir)?;
    atomic_write(&collection_file_path, json_content.as_bytes())?;

    // Step 14: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": collection.id,
                    "name": collection.name(),
                    "path": collection_file_path.to_string_lossy(),
                    "play_mode": play_mode_value.to_string(),
                    "scheduler_mode": scheduler_mode_value.to_string(),
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Collection '{}' created successfully at {}",
                    name,
                    collection_file_path.display()
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

/// Format gain value for JSON output.
fn gain_to_json(gain: &Option<RtpcCompatibleValue>) -> serde_json::Value {
    match gain {
        Some(g) => match g.as_static() {
            Some(v) => json!(v),
            None => json!("RTPC"),
        },
        None => serde_json::Value::Null,
    }
}

/// Format spatialization mode as a stable string for JSON output.
fn spatialization_to_string(spatialization: &Spatialization) -> &'static str {
    match spatialization {
        Spatialization::None => "None",
        Spatialization::Position => "Position",
        Spatialization::PositionOrientation => "PositionOrientation",
        Spatialization::HRTF => "HRTF",
    }
}

/// List all collection assets in the current project.
async fn list_collections(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Scan collections directory
    let collections_dir = current_dir.join("sources").join("collections");

    // Step 3: Handle missing directory
    if !collections_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "collections": [],
                        "count": 0,
                        "warnings": ["No collections directory found. Create collections with 'am asset collection create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No collections directory found.");
                output.progress(&format!(
                    "Create collections with '{}'.",
                    "am asset collection create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 4: Read and parse all .json files
    let mut collections: Vec<Collection> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let entries = match fs::read_dir(&collections_dir) {
        Ok(entries) => entries,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read collections directory",
                format!("Permission denied on {}", collections_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    let canonical_collections_dir = collections_dir
        .canonicalize()
        .unwrap_or_else(|_| collections_dir.clone());

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Skip symlinks that resolve outside the collections directory
        if path.is_symlink()
            && path
                .canonicalize()
                .is_ok_and(|resolved| !resolved.starts_with(&canonical_collections_dir))
        {
            log::warn!(
                "Skipping symlink outside collections directory: {}",
                path.display()
            );
            continue;
        }

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Collection>(&content) {
                    Ok(collection) => {
                        collections.push(collection);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        log::warn!("Skipping invalid collection file: {}", path.display());
                        warnings.push(format!("Invalid JSON in {}: {}", filename, e));
                    }
                },
                Err(e) => {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    log::warn!("Failed to read collection file: {}", path.display());
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 5: Sort by name
    collections.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 6: Handle empty
    if collections.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "collections": [],
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
                output.progress("No collections found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset collection create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 7: Output
    match output.mode() {
        OutputMode::Json => {
            let collection_data: Vec<serde_json::Value> = collections
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "name": c.name(),
                        "play_mode": c.play_mode.to_string(),
                        "scheduler_mode": c.scheduler.as_ref().map(|s| s.mode.to_string()).unwrap_or_default(),
                        "gain": gain_to_json(&c.gain),
                        "spatialization": spatialization_to_string(&c.spatialization)
                    })
                })
                .collect();

            output.success(
                json!({
                    "collections": collection_data,
                    "count": collections.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = collections
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "name": c.name(),
                        "play_mode": c.play_mode.to_string(),
                        "scheduler": c.scheduler.as_ref().map(|s| s.mode.to_string()).unwrap_or_default()
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} collection(s) found", collections.len()));
        }
    }

    Ok(())
}

// =============================================================================
// Update
// =============================================================================

/// Update an existing collection asset.
#[allow(clippy::too_many_arguments)]
async fn update_collection(
    name: &str,
    play_mode: Option<String>,
    scheduler_mode: Option<String>,
    gain: Option<f32>,
    bus: Option<u64>,
    priority: Option<u8>,
    spatialization: Option<String>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating collection '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Locate existing collection file
    let collections_dir = current_dir.join("sources").join("collections");
    let collection_file_path = collections_dir.join(format!("{}.json", name));

    if !collection_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset collection list' to see available collections, or 'am asset collection create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing collection
    let content = fs::read_to_string(&collection_file_path).context(format!(
        "Failed to read collection file: {}",
        collection_file_path.display()
    ))?;
    let mut collection: Collection = serde_json::from_str(&content).context(format!(
        "Failed to parse collection file: {}",
        collection_file_path.display()
    ))?;

    // Step 4: Determine if we have any flag values (non-interactive mode)
    let has_any_flag = play_mode.is_some()
        || scheduler_mode.is_some()
        || gain.is_some()
        || bus.is_some()
        || priority.is_some()
        || spatialization.is_some();

    // Step 5: Apply updates
    let updated_fields: Vec<String> = if has_any_flag {
        apply_flag_updates(
            &mut collection,
            play_mode,
            scheduler_mode,
            gain,
            bus,
            priority,
            spatialization,
        )?
    } else {
        prompt_collection_updates(&mut collection, input)?
    };

    // Step 6: Validate
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);
    collection.validate_rules(&context)?;

    // Step 7: Serialize and write atomically
    let json_content = serde_json::to_string_pretty(&collection)
        .context("Failed to serialize collection to JSON")?;
    atomic_write(&collection_file_path, json_content.as_bytes())?;

    // Step 8: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": collection.id,
                    "name": collection.name(),
                    "path": collection_file_path.to_string_lossy(),
                    "updated_fields": updated_fields,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Collection '{}' updated successfully at {}",
                    name,
                    collection_file_path.display()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Apply flag updates to a collection (non-interactive mode).
fn apply_flag_updates(
    collection: &mut Collection,
    play_mode: Option<String>,
    scheduler_mode: Option<String>,
    gain: Option<f32>,
    bus: Option<u64>,
    priority: Option<u8>,
    spatialization: Option<String>,
) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    if let Some(pm) = play_mode {
        collection.play_mode = CollectionPlayMode::from_str(&pm).map_err(|e| {
            CliError::new(codes::ERR_VALIDATION_FIELD, e, "Invalid play mode value")
        })?;
        updated_fields.push("play_mode".to_string());
    }

    if let Some(sm) = scheduler_mode {
        let mode = SoundSchedulerMode::from_str(&sm).map_err(|e| {
            CliError::new(
                codes::ERR_VALIDATION_FIELD,
                e,
                "Invalid scheduler mode value",
            )
        })?;
        collection.scheduler = Some(crate::assets::SoundSchedulerSettings { mode });
        updated_fields.push("scheduler_mode".to_string());
    }

    if let Some(g) = gain {
        validate_gain(g)?;
        collection.gain = Some(RtpcCompatibleValue::static_value(g));
        updated_fields.push("gain".to_string());
    }

    if let Some(b) = bus {
        collection.bus = b;
        updated_fields.push("bus".to_string());
    }

    if let Some(p) = priority {
        collection.priority = Some(RtpcCompatibleValue::static_value(p as f32));
        updated_fields.push("priority".to_string());
    }

    if let Some(s) = spatialization {
        collection.spatialization = parse_spatialization(&s)?;
        updated_fields.push("spatialization".to_string());
    }

    Ok(updated_fields)
}

/// Prompt for collection updates in interactive mode.
fn prompt_collection_updates(
    collection: &mut Collection,
    input: &dyn Input,
) -> Result<Vec<String>> {
    let mut updated_fields = Vec::new();

    // Prompt for play mode
    if let Some(new_pm) = prompt_update_play_mode(input, &collection.play_mode)? {
        collection.play_mode = new_pm;
        updated_fields.push("play_mode".to_string());
    }

    // Prompt for scheduler mode
    let current_scheduler = collection
        .scheduler
        .as_ref()
        .map(|s| s.mode)
        .unwrap_or(SoundSchedulerMode::Random);
    if let Some(new_sm) = prompt_update_scheduler_mode(input, &current_scheduler)? {
        collection.scheduler = Some(crate::assets::SoundSchedulerSettings { mode: new_sm });
        updated_fields.push("scheduler_mode".to_string());
    }

    // Prompt for gain
    let current_gain = collection
        .gain
        .as_ref()
        .and_then(|g| g.as_static())
        .unwrap_or(1.0);
    if let Some(new_gain) = prompt_update_number(
        input,
        "Volume gain [0.0-1.0]",
        current_gain,
        |v| (0.0..=1.0).contains(&v),
        "Gain must be between 0.0 and 1.0",
    )? {
        collection.gain = Some(RtpcCompatibleValue::static_value(new_gain));
        updated_fields.push("gain".to_string());
    }

    // Prompt for priority
    let current_priority = collection
        .priority
        .as_ref()
        .and_then(|p| p.as_static())
        .unwrap_or(128.0);
    if let Some(new_priority) = prompt_update_number(
        input,
        "Playback priority [0-255]",
        current_priority,
        |v| (0.0..=255.0).contains(&v) && v == v.floor(),
        "Priority must be an integer between 0 and 255",
    )? {
        collection.priority = Some(RtpcCompatibleValue::static_value(new_priority));
        updated_fields.push("priority".to_string());
    }

    // Prompt for spatialization
    if let Some(new_spat) = prompt_update_spatialization(input, &collection.spatialization)? {
        collection.spatialization = new_spat;
        updated_fields.push("spatialization".to_string());
    }

    // Prompt for bus
    let current_bus = collection.bus;
    if let Some(new_bus) = prompt_update_number(
        input,
        "Bus ID",
        current_bus as f32,
        |v| v >= 0.0 && v == v.floor(),
        "Bus ID must be a non-negative integer",
    )? {
        collection.bus = new_bus as u64;
        updated_fields.push("bus".to_string());
    }

    Ok(updated_fields)
}

// =============================================================================
// Delete
// =============================================================================

/// Delete a collection asset.
async fn delete_collection(
    name: &str,
    force: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Locate collection file
    let collections_dir = current_dir.join("sources").join("collections");
    let collection_file_path = collections_dir.join(format!("{}.json", name));

    if !collection_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset collection list' to see available collections")
            .into());
    }

    // Step 3: Read collection for response data
    let content = fs::read_to_string(&collection_file_path).context(format!(
        "Failed to read collection file: {}",
        collection_file_path.display()
    ))?;
    let collection: Collection = serde_json::from_str(&content).context(format!(
        "Failed to parse collection file: {}",
        collection_file_path.display()
    ))?;

    // Step 4: Confirm deletion
    let confirmed = if force {
        true
    } else {
        match input.confirm(
            &format!("Delete collection '{}'? This cannot be undone.", name),
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
                    "Use 'am asset collection delete {} --force' to delete without prompting",
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
    fs::remove_file(&collection_file_path).context(format!(
        "Failed to delete collection file: {}",
        collection_file_path.display()
    ))?;

    // Step 6: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": collection.id,
                    "name": collection.name(),
                    "deleted": true,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Collection '{}' deleted successfully", name)),
                None,
            );
        }
    }

    Ok(())
}

// =============================================================================
// Shared prompt/validation helpers
// =============================================================================

/// Validate gain is in valid range.
fn validate_gain(gain: f32) -> Result<()> {
    if !(0.0..=1.0).contains(&gain) {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid gain value: {}", gain),
            "Gain must be between 0.0 and 1.0",
        )
        .with_suggestion("Set gain to a value between 0.0 (silent) and 1.0 (full volume)")
        .into());
    }
    Ok(())
}

/// Prompt for play mode selection.
///
/// In non-interactive mode, defaults to PlayOne (per AC2: "all fields use defaults or provided flags").
fn prompt_play_mode(input: &dyn Input) -> Result<CollectionPlayMode> {
    let options = crate::assets::extensions::COLLECTION_PLAY_MODE_NAMES;
    let modes = [CollectionPlayMode::PlayOne, CollectionPlayMode::PlayAll];

    match select_index(input, "Play mode:", options) {
        Ok(idx) => Ok(modes[idx]),
        Err(_) => {
            log::debug!("Non-interactive mode: using default play mode PlayOne");
            Ok(CollectionPlayMode::PlayOne)
        }
    }
}

/// Prompt for scheduler mode selection.
///
/// In non-interactive mode, defaults to Random (per AC2: "all fields use defaults or provided flags").
fn prompt_scheduler_mode(input: &dyn Input) -> Result<SoundSchedulerMode> {
    let options = crate::assets::extensions::SOUND_SCHEDULER_MODE_NAMES;
    let modes = [SoundSchedulerMode::Random, SoundSchedulerMode::Sequence];

    match select_index(input, "Scheduler mode:", options) {
        Ok(idx) => Ok(modes[idx]),
        Err(_) => {
            log::debug!("Non-interactive mode: using default scheduler mode Random");
            Ok(SoundSchedulerMode::Random)
        }
    }
}

/// Prompt for gain value.
///
/// In non-interactive mode, defaults to 1.0 (per AC2: "all fields use defaults or provided flags").
fn prompt_gain(input: &dyn Input) -> Result<f32> {
    let result = input.prompt_text(
        "Volume gain [0.0-1.0]",
        Some("1.0"),
        None,
        Some(&|value: &str| match value.trim().parse::<f32>() {
            Ok(g) if (0.0..=1.0).contains(&g) => Ok(Validation::Valid),
            Ok(g) => Ok(Validation::Invalid(
                format!("Gain must be between 0.0 and 1.0, got {}", g).into(),
            )),
            Err(_) => Ok(Validation::Invalid("Must be a number".into())),
        }),
    );

    match result {
        Ok(value) => Ok(value.trim().parse().unwrap_or(1.0)),
        Err(_) => Ok(1.0), // Default in non-interactive mode
    }
}

/// Prompt for priority value.
fn prompt_priority(input: &dyn Input) -> Result<u8> {
    let result = input.prompt_text(
        "Playback priority [0-255]",
        Some("128"),
        None,
        Some(&|value: &str| match value.trim().parse::<u8>() {
            Ok(_) => Ok(Validation::Valid),
            Err(_) => Ok(Validation::Invalid(
                "Must be a number between 0 and 255".into(),
            )),
        }),
    );

    match result {
        Ok(value) => Ok(value.trim().parse().unwrap_or(128)),
        Err(_) => Ok(128), // Default in non-interactive mode
    }
}

/// Prompt for spatialization mode.
fn prompt_spatialization(input: &dyn Input) -> Result<Spatialization> {
    let options = vec![
        "None".to_string(),
        "Position".to_string(),
        "Position + Orientation".to_string(),
        "HRTF".to_string(),
    ];

    let modes = [
        Spatialization::None,
        Spatialization::Position,
        Spatialization::PositionOrientation,
        Spatialization::HRTF,
    ];

    match select_index(input, "Spatialization mode:", &options) {
        Ok(idx) => Ok(modes[idx]),
        Err(_) => Ok(Spatialization::None), // Default in non-interactive mode
    }
}

/// Prompt for an optional numeric field update.
fn prompt_update_number(
    input: &dyn Input,
    label: &str,
    current_value: f32,
    validator: impl Fn(f32) -> bool,
    error_msg: &str,
) -> Result<Option<f32>> {
    let current_str = format!("{}", current_value);
    let prompt = format!("{} (current: {}, Enter to keep)", label, current_value);

    let error_msg = error_msg.to_string();
    let result = input.prompt_text(
        &prompt,
        Some(&current_str),
        None,
        Some(&move |value: &str| match value.trim().parse::<f32>() {
            Ok(v) if validator(v) => Ok(Validation::Valid),
            Ok(_) => Ok(Validation::Invalid(error_msg.clone().into())),
            Err(_) => Ok(Validation::Invalid("Must be a number".into())),
        }),
    );

    match result {
        Ok(value) => {
            let parsed: f32 = value.trim().parse().unwrap_or(current_value);
            if (parsed - current_value).abs() < f32::EPSILON {
                Ok(None)
            } else {
                Ok(Some(parsed))
            }
        }
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}

/// Prompt for play mode update.
fn prompt_update_play_mode(
    input: &dyn Input,
    current: &CollectionPlayMode,
) -> Result<Option<CollectionPlayMode>> {
    let options = crate::assets::extensions::COLLECTION_PLAY_MODE_NAMES;
    let modes = [CollectionPlayMode::PlayOne, CollectionPlayMode::PlayAll];

    let current_idx = modes.iter().position(|m| m == current).unwrap_or(0);
    let prompt = format!("Play mode (current: {}):", options[current_idx]);

    match select_index(input, &prompt, options) {
        Ok(idx) if idx == current_idx => Ok(None),
        Ok(idx) => Ok(Some(modes[idx])),
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}

/// Prompt for scheduler mode update.
fn prompt_update_scheduler_mode(
    input: &dyn Input,
    current: &SoundSchedulerMode,
) -> Result<Option<SoundSchedulerMode>> {
    let options = crate::assets::extensions::SOUND_SCHEDULER_MODE_NAMES;
    let modes = [SoundSchedulerMode::Random, SoundSchedulerMode::Sequence];

    let current_idx = modes.iter().position(|m| m == current).unwrap_or(0);
    let prompt = format!("Scheduler mode (current: {}):", options[current_idx]);

    match select_index(input, &prompt, options) {
        Ok(idx) if idx == current_idx => Ok(None),
        Ok(idx) => Ok(Some(modes[idx])),
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}

/// Prompt for spatialization update.
fn prompt_update_spatialization(
    input: &dyn Input,
    current: &Spatialization,
) -> Result<Option<Spatialization>> {
    let options = vec![
        "None".to_string(),
        "Position".to_string(),
        "Position + Orientation".to_string(),
        "HRTF".to_string(),
    ];

    let modes = [
        Spatialization::None,
        Spatialization::Position,
        Spatialization::PositionOrientation,
        Spatialization::HRTF,
    ];

    let current_idx = modes.iter().position(|m| m == current).unwrap_or(0);
    let prompt = format!("Spatialization mode (current: {}):", options[current_idx]);

    match select_index(input, &prompt, &options) {
        Ok(idx) if idx == current_idx => Ok(None),
        Ok(idx) => Ok(Some(modes[idx])),
        Err(_) => Ok(None), // Non-interactive, keep current
    }
}
