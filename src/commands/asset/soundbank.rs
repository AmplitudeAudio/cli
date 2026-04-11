//! Soundbank asset commands.
//!
//! Implements CRUD operations for Soundbank assets in Amplitude projects.
//! Soundbanks package multiple assets together for efficient runtime loading.

use std::env;
use std::fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Subcommand;
use colored::Colorize;
use serde_json::json;

use crate::common::utils::generate_unique_id;
use crate::{
    assets::{Asset, AssetType, ProjectContext, ProjectValidator, Soundbank, SoundbankBuilder},
    common::{
        errors::{CliError, asset_already_exists, asset_not_found, codes},
        files::atomic_write,
        utils::read_amproject_file,
    },
    database::Database,
    input::{Input, select_index},
    presentation::{Output, OutputMode},
};

/// The name of the current asset.
const ASSET_NAME: &str = "Soundbank";

/// Soundbank asset subcommands.
#[derive(Subcommand, Debug)]
pub enum SoundbankCommands {
    /// Create a new soundbank asset
    Create {
        /// Name of the soundbank asset
        name: String,

        /// Include assets in format "type:name[,name,...]" (repeatable)
        /// Example: --include sound:footstep,explosion --include collection:ambience
        #[arg(short, long)]
        include: Vec<String>,
    },

    /// List all soundbank assets in the project
    List {},

    /// Update an existing soundbank asset
    Update {
        /// Name of the soundbank asset to update
        name: String,

        /// Add assets in format "type:name[,name,...]" (repeatable)
        #[arg(short, long)]
        add: Vec<String>,

        /// Remove assets in format "type:name[,name,...]" (repeatable)
        #[arg(short, long)]
        remove: Vec<String>,
    },

    /// Delete a soundbank asset
    Delete {
        /// Name of the soundbank asset to delete
        name: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
}

/// Handle soundbank commands by routing to the appropriate handler.
pub async fn handler(
    command: &SoundbankCommands,
    _database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    match command {
        SoundbankCommands::Create { name, include } => {
            create_soundbank(name, include.clone(), input, output).await
        }
        SoundbankCommands::List {} => list_soundbanks(output).await,
        SoundbankCommands::Update { name, add, remove } => {
            update_soundbank(name, add.clone(), remove.clone(), input, output).await
        }
        SoundbankCommands::Delete { name, yes } => {
            delete_soundbank(name, *yes, input, output).await
        }
    }
}

/// Parse include specification string in format "type:name[,name,...]"
/// Returns (asset_type_key, Vec<asset_paths>)
fn parse_include_spec(spec: &str, sources_dir: &std::path::Path) -> Result<Vec<(String, String)>> {
    let parts: Vec<&str> = spec.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Invalid include format: '{}'", spec),
            "Include must be in format 'type:name' or 'type:name1,name2'",
        )
        .with_suggestion(
            "Example: --include sound:footstep,explosion --include collection:ambience",
        )
        .into());
    }

    let type_key = parts[0].to_lowercase();

    // Validate the type key
    let dir_name = match type_key.as_str() {
        "sound" => "sounds",
        "collection" => "collections",
        "event" => "events",
        "switch" => "switches",
        "switch_container" => "switch_containers",
        "effect" => "effects",
        "attenuator" => "attenuators",
        "rtpc" => "rtpc",
        _ => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                format!("Invalid asset type: '{}'", type_key),
                "Asset type must be one of: sound, collection, event, switch, switch_container, effect, attenuator, rtpc",
            )
            .into());
        }
    };

    let names: Vec<&str> = parts[1].split(',').map(|s| s.trim()).collect();
    let mut results = Vec::new();

    for name in names {
        if name.is_empty() {
            continue;
        }
        // Build the relative path: type_dir/name.json
        let relative_path = format!("{}/{}.json", dir_name, name);
        let full_path = sources_dir.join(&relative_path);

        if !full_path.exists() {
            return Err(CliError::new(
                codes::ERR_VALIDATION_REFERENCE,
                format!("{} '{}' not found", type_key, name),
                format!(
                    "Expected file at {}",
                    full_path.display()
                ),
            )
            .with_suggestion(format!(
                "Create the {} first with 'am asset {} create {}'",
                type_key, type_key, name
            ))
            .into());
        }

        results.push((type_key.clone(), relative_path));
    }

    Ok(results)
}

/// Collect all available assets from project sources directory, grouped by type.
fn collect_available_assets(
    sources_dir: &std::path::Path,
) -> Vec<(String, String, String)> {
    // Returns (type_label, asset_name, relative_path)
    let type_dirs = [
        ("sound", "sounds"),
        ("collection", "collections"),
        ("event", "events"),
        ("switch", "switches"),
        ("switch_container", "switch_containers"),
        ("effect", "effects"),
        ("attenuator", "attenuators"),
        ("rtpc", "rtpc"),
    ];

    let mut assets = Vec::new();

    for (type_label, dir_name) in &type_dirs {
        let dir = sources_dir.join(dir_name);
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "json") {
                    let filename = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let relative = format!("{}/{}.json", dir_name, filename);
                    assets.push((type_label.to_string(), filename, relative));
                }
            }
        }
    }

    assets.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    assets
}

/// Add an asset path to the appropriate soundbank field.
fn add_asset_to_builder(builder: SoundbankBuilder, type_key: &str, path: String) -> SoundbankBuilder {
    match type_key {
        "sound" => builder.sound(path),
        "collection" => builder.collection(path),
        "event" => builder.event(path),
        "switch" => builder.switch(path),
        "switch_container" => builder.switch_container(path),
        "effect" => builder.effect(path),
        "attenuator" => builder.attenuator(path),
        "rtpc" => builder.rtpc(path),
        _ => builder,
    }
}

/// Add an asset path to an existing Soundbank's appropriate field.
fn add_asset_to_soundbank(soundbank: &mut Soundbank, type_key: &str, path: String) {
    match type_key {
        "sound" => {
            soundbank
                .sounds
                .get_or_insert_with(Vec::new)
                .push(path);
        }
        "collection" => {
            soundbank
                .collections
                .get_or_insert_with(Vec::new)
                .push(path);
        }
        "event" => {
            soundbank
                .events
                .get_or_insert_with(Vec::new)
                .push(path);
        }
        "switch" => {
            soundbank
                .switches
                .get_or_insert_with(Vec::new)
                .push(path);
        }
        "switch_container" => {
            soundbank
                .switch_containers
                .get_or_insert_with(Vec::new)
                .push(path);
        }
        "effect" => {
            soundbank
                .effects
                .get_or_insert_with(Vec::new)
                .push(path);
        }
        "attenuator" => {
            soundbank
                .attenuators
                .get_or_insert_with(Vec::new)
                .push(path);
        }
        "rtpc" => {
            soundbank.rtpc.get_or_insert_with(Vec::new).push(path);
        }
        _ => {}
    }
}

/// Remove an asset path from a Soundbank. Returns true if removed.
fn remove_asset_from_soundbank(soundbank: &mut Soundbank, type_key: &str, path: &str) -> bool {
    let list = match type_key {
        "sound" => &mut soundbank.sounds,
        "collection" => &mut soundbank.collections,
        "event" => &mut soundbank.events,
        "switch" => &mut soundbank.switches,
        "switch_container" => &mut soundbank.switch_containers,
        "effect" => &mut soundbank.effects,
        "attenuator" => &mut soundbank.attenuators,
        "rtpc" => &mut soundbank.rtpc,
        _ => return false,
    };

    if let Some(vec) = list {
        if let Some(pos) = vec.iter().position(|p| p == path) {
            vec.remove(pos);
            return true;
        }
    }
    false
}

/// Get all assets currently in a soundbank as (type_key, path) pairs.
fn get_soundbank_assets(soundbank: &Soundbank) -> Vec<(String, String)> {
    let mut assets = Vec::new();

    if let Some(ref sounds) = soundbank.sounds {
        for p in sounds {
            assets.push(("sound".to_string(), p.clone()));
        }
    }
    if let Some(ref collections) = soundbank.collections {
        for p in collections {
            assets.push(("collection".to_string(), p.clone()));
        }
    }
    if let Some(ref events) = soundbank.events {
        for p in events {
            assets.push(("event".to_string(), p.clone()));
        }
    }
    if let Some(ref switches) = soundbank.switches {
        for p in switches {
            assets.push(("switch".to_string(), p.clone()));
        }
    }
    if let Some(ref containers) = soundbank.switch_containers {
        for p in containers {
            assets.push(("switch_container".to_string(), p.clone()));
        }
    }
    if let Some(ref effects) = soundbank.effects {
        for p in effects {
            assets.push(("effect".to_string(), p.clone()));
        }
    }
    if let Some(ref attenuators) = soundbank.attenuators {
        for p in attenuators {
            assets.push(("attenuator".to_string(), p.clone()));
        }
    }
    if let Some(ref rtpcs) = soundbank.rtpc {
        for p in rtpcs {
            assets.push(("rtpc".to_string(), p.clone()));
        }
    }

    assets
}

/// Count assets by type in a soundbank.
fn count_assets_by_type(soundbank: &Soundbank) -> Vec<(String, usize)> {
    let mut counts = Vec::new();

    let check = |name: &str, opt: &Option<Vec<String>>| -> Option<(String, usize)> {
        opt.as_ref()
            .filter(|v| !v.is_empty())
            .map(|v| (name.to_string(), v.len()))
    };

    if let Some(c) = check("sounds", &soundbank.sounds) {
        counts.push(c);
    }
    if let Some(c) = check("collections", &soundbank.collections) {
        counts.push(c);
    }
    if let Some(c) = check("events", &soundbank.events) {
        counts.push(c);
    }
    if let Some(c) = check("switches", &soundbank.switches) {
        counts.push(c);
    }
    if let Some(c) = check("switch_containers", &soundbank.switch_containers) {
        counts.push(c);
    }
    if let Some(c) = check("effects", &soundbank.effects) {
        counts.push(c);
    }
    if let Some(c) = check("attenuators", &soundbank.attenuators) {
        counts.push(c);
    }
    if let Some(c) = check("rtpc", &soundbank.rtpc) {
        counts.push(c);
    }

    counts
}

/// Simple output helper for interactive prompts.
fn output_simple(message: &str) {
    println!("{}", message);
}

// =============================================================================
// Create
// =============================================================================

/// Create a new soundbank asset.
async fn create_soundbank(
    name: &str,
    includes: Vec<String>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Creating soundbank '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Validate name doesn't already exist
    let soundbanks_dir = current_dir.join("sources").join("soundbanks");
    let soundbank_file_path = soundbanks_dir.join(format!("{}.json", name));

    if soundbank_file_path.exists() {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset soundbank update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    // Build context for validation
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    if context.has_name(AssetType::Soundbank, name) {
        return Err(asset_already_exists(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset soundbank update {}' to modify it, or choose a different name",
                name
            ))
            .into());
    }

    let sources_dir = current_dir.join("sources");

    // Step 3: Get assets to include
    let asset_refs: Vec<(String, String)> = if includes.is_empty() {
        // Interactive mode: prompt for assets
        prompt_select_assets(input, &sources_dir)?
    } else {
        // Non-interactive: parse --include flags
        let mut refs = Vec::new();
        for spec in &includes {
            let parsed = parse_include_spec(spec, &sources_dir)?;
            refs.extend(parsed);
        }
        refs
    };

    // Validate at least one asset
    if asset_refs.is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Soundbank has no assets",
            "Soundbanks must contain at least one asset",
        )
        .with_suggestion(
            "Add at least one asset using --include flag or in interactive mode",
        )
        .into());
    }

    // Step 4: Generate unique ID
    let mut id = generate_unique_id(name);
    let mut retries = 0;
    while context.has_id(id) && retries < 3 {
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

    // Step 5: Build the Soundbank
    let mut builder = Soundbank::builder(id, name);
    for (type_key, path) in &asset_refs {
        builder = add_asset_to_builder(builder, type_key, path.clone());
    }
    let soundbank = builder.build();

    // Step 6: Show summary in interactive mode
    if output.mode() == OutputMode::Interactive {
        output_simple(&format!("\nSoundbank '{}' will include:", name));
        let counts = count_assets_by_type(&soundbank);
        for (type_name, count) in &counts {
            output_simple(&format!("  {}: {}", type_name, count));
        }
        output_simple(&format!("  Total: {} asset(s)\n", soundbank.asset_count()));
    }

    // Step 7: Serialize to JSON
    let json_content = serde_json::to_string_pretty(&soundbank)
        .context("Failed to serialize soundbank to JSON")?;

    // Step 8: Write atomically
    fs::create_dir_all(&soundbanks_dir)?;
    atomic_write(&soundbank_file_path, json_content.as_bytes())?;

    // Step 9: Output success
    match output.mode() {
        OutputMode::Json => {
            let counts = count_assets_by_type(&soundbank);
            let type_counts: serde_json::Value = counts
                .iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into();

            output.success(
                json!({
                    "id": soundbank.id,
                    "name": soundbank.name(),
                    "path": soundbank_file_path.to_string_lossy(),
                    "asset_count": soundbank.asset_count(),
                    "assets_by_type": type_counts,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Soundbank '{}' created successfully at {}\n  Assets: {} total",
                    name,
                    soundbank_file_path.display(),
                    soundbank.asset_count()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Prompt user to select assets interactively, grouped by type.
fn prompt_select_assets(
    input: &dyn Input,
    sources_dir: &std::path::Path,
) -> Result<Vec<(String, String)>> {
    let available = collect_available_assets(sources_dir);

    if available.is_empty() {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "No assets found in project",
            "The project has no assets to include in a soundbank",
        )
        .with_suggestion("Create some assets first (sounds, collections, events, etc.)")
        .into());
    }

    let mut selected: Vec<(String, String)> = Vec::new();

    // Group by type for display
    output_simple("\nAvailable assets by type:");
    let mut current_type = String::new();
    for (type_label, name, _path) in &available {
        if *type_label != current_type {
            current_type = type_label.clone();
            output_simple(&format!("  {}:", type_label));
        }
        output_simple(&format!("    - {}", name));
    }
    output_simple("");

    loop {
        if !selected.is_empty() {
            output_simple(&format!("\nSelected ({}):", selected.len()));
            for (t, p) in &selected {
                output_simple(&format!("  {} : {}", t, p));
            }
            output_simple("");
        }

        let should_add = match input.confirm("Add an asset to this soundbank?", Some(true)) {
            Ok(val) => val,
            Err(_) => break, // Non-interactive mode
        };

        if !should_add {
            break;
        }

        // Build options list from available assets not already selected
        let options: Vec<String> = available
            .iter()
            .filter(|(t, _n, p)| !selected.iter().any(|(st, sp)| st == t && sp == p))
            .map(|(t, n, _p)| format!("[{}] {}", t, n))
            .collect();

        if options.is_empty() {
            output_simple("All available assets have been selected.");
            break;
        }

        let remaining: Vec<&(String, String, String)> = available
            .iter()
            .filter(|(t, _n, p)| !selected.iter().any(|(st, sp)| st == t && sp == p))
            .collect();

        match select_index(input, "Select asset:", &options) {
            Ok(idx) => {
                let (type_key, _name, path) = &remaining[idx];
                selected.push((type_key.clone(), path.clone()));
            }
            Err(_) => break,
        }
    }

    Ok(selected)
}

// =============================================================================
// List
// =============================================================================

/// List all soundbank assets in the current project.
async fn list_soundbanks(output: &dyn Output) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Scan soundbanks directory
    let soundbanks_dir = current_dir.join("sources").join("soundbanks");

    if !soundbanks_dir.exists() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "soundbanks": [],
                        "count": 0,
                        "warnings": ["No soundbanks directory found. Create soundbanks with 'am asset soundbank create'."]
                    }),
                    None,
                );
            }
            OutputMode::Interactive => {
                output.progress("No soundbanks directory found.");
                output.progress(&format!(
                    "Create soundbanks with '{}'.",
                    "am asset soundbank create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 3: Read and parse all .json files
    let mut soundbanks: Vec<Soundbank> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let entries = match fs::read_dir(&soundbanks_dir) {
        Ok(entries) => entries,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Cannot read soundbanks directory",
                format!("Permission denied on {}", soundbanks_dir.display()),
            )
            .with_suggestion("Check directory permissions")
            .with_context(format!("I/O error: {}", e))
            .into());
        }
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|e| e == "json") {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<Soundbank>(&content) {
                    Ok(soundbank) => {
                        soundbanks.push(soundbank);
                    }
                    Err(e) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        log::warn!("Skipping invalid soundbank file: {}", path.display());
                        warnings.push(format!("Invalid JSON in {}: {}", filename, e));
                    }
                },
                Err(e) => {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    log::warn!("Failed to read soundbank file: {}", path.display());
                    warnings.push(format!("Failed to read {}: {}", filename, e));
                }
            }
        }
    }

    // Step 4: Sort by name
    soundbanks.sort_by(|a, b| a.name().cmp(b.name()));

    // Step 5: Handle empty directory
    if soundbanks.is_empty() {
        match output.mode() {
            OutputMode::Json => {
                output.success(
                    json!({
                        "soundbanks": [],
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
                output.progress("No soundbanks found in this project.");
                output.progress(&format!(
                    "Use '{}' to add one.",
                    "am asset soundbank create <name>".green()
                ));
            }
        }
        return Ok(());
    }

    // Step 6: Output based on mode
    match output.mode() {
        OutputMode::Json => {
            let soundbank_data: Vec<serde_json::Value> = soundbanks
                .iter()
                .map(|sb| {
                    let counts = count_assets_by_type(sb);
                    let type_counts: serde_json::Value = counts
                        .iter()
                        .map(|(k, v)| (k.clone(), json!(v)))
                        .collect::<serde_json::Map<String, serde_json::Value>>()
                        .into();

                    json!({
                        "id": sb.id,
                        "name": sb.name(),
                        "asset_count": sb.asset_count(),
                        "assets_by_type": type_counts,
                    })
                })
                .collect();

            output.success(
                json!({
                    "soundbanks": soundbank_data,
                    "count": soundbanks.len(),
                    "warnings": warnings
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            for warning in &warnings {
                output.progress(&format!("{} {}", "Warning:".yellow(), warning));
            }

            let table_data: Vec<serde_json::Value> = soundbanks
                .iter()
                .map(|sb| {
                    let counts = count_assets_by_type(sb);
                    let type_summary = counts
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k, v))
                        .collect::<Vec<_>>()
                        .join(", ");

                    json!({
                        "id": sb.id,
                        "name": sb.name(),
                        "total": sb.asset_count(),
                        "breakdown": type_summary,
                    })
                })
                .collect();

            output.table(None, json!(table_data));
            output.progress("");
            output.progress(&format!("{} soundbank(s) found", soundbanks.len()));
        }
    }

    Ok(())
}

// =============================================================================
// Update
// =============================================================================

/// Update an existing soundbank asset.
async fn update_soundbank(
    name: &str,
    add_specs: Vec<String>,
    remove_specs: Vec<String>,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Updating soundbank '{}' in project '{}'...",
        name, project_config.name
    ));

    // Step 2: Locate existing soundbank file
    let soundbanks_dir = current_dir.join("sources").join("soundbanks");
    let soundbank_file_path = soundbanks_dir.join(format!("{}.json", name));

    if !soundbank_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion(format!(
                "Use 'am asset soundbank list' to see available soundbanks, or 'am asset soundbank create {}' to create it",
                name
            ))
            .into());
    }

    // Step 3: Parse existing soundbank
    let content = fs::read_to_string(&soundbank_file_path).context(format!(
        "Failed to read soundbank file: {}",
        soundbank_file_path.display()
    ))?;
    let mut soundbank: Soundbank = serde_json::from_str(&content).context(format!(
        "Failed to parse soundbank file: {}",
        soundbank_file_path.display()
    ))?;

    let sources_dir = current_dir.join("sources");
    let mut updated_fields: Vec<String> = Vec::new();

    let has_any_flag = !add_specs.is_empty() || !remove_specs.is_empty();

    if has_any_flag {
        // Non-interactive: apply flags

        // Process additions
        for spec in &add_specs {
            let parsed = parse_include_spec(spec, &sources_dir)?;
            for (type_key, path) in parsed {
                add_asset_to_soundbank(&mut soundbank, &type_key, path);
            }
            updated_fields.push("added_assets".to_string());
        }

        // Process removals
        for spec in &remove_specs {
            let parsed = parse_include_spec(spec, &sources_dir)?;
            for (type_key, path) in parsed {
                if !remove_asset_from_soundbank(&mut soundbank, &type_key, &path) {
                    return Err(CliError::new(
                        codes::ERR_VALIDATION_FIELD,
                        format!("Asset '{}' not found in soundbank", path),
                        "The specified asset is not currently in this soundbank",
                    )
                    .with_suggestion("Use 'am asset soundbank list' to see current contents")
                    .into());
                }
            }
            updated_fields.push("removed_assets".to_string());
        }
    } else {
        // Interactive mode: prompt for modifications
        let modified = prompt_modify_soundbank(input, &mut soundbank, &sources_dir)?;
        if modified {
            updated_fields.push("assets".to_string());
        }
    }

    // Validate at least one asset remains
    if soundbank.asset_count() == 0 {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            "Soundbank has no assets",
            "Soundbanks must contain at least one asset",
        )
        .with_suggestion("Add at least one asset before saving")
        .into());
    }

    // Serialize and write atomically
    let json_content = serde_json::to_string_pretty(&soundbank)
        .context("Failed to serialize soundbank to JSON")?;
    atomic_write(&soundbank_file_path, json_content.as_bytes())?;

    // Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": soundbank.id,
                    "name": soundbank.name(),
                    "path": soundbank_file_path.to_string_lossy(),
                    "asset_count": soundbank.asset_count(),
                    "updated_fields": updated_fields,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!(
                    "Soundbank '{}' updated successfully at {}\n  Assets: {} total",
                    name,
                    soundbank_file_path.display(),
                    soundbank.asset_count()
                )),
                None,
            );
        }
    }

    Ok(())
}

/// Prompt to modify soundbank contents interactively. Returns true if modified.
fn prompt_modify_soundbank(
    input: &dyn Input,
    soundbank: &mut Soundbank,
    sources_dir: &std::path::Path,
) -> Result<bool> {
    let mut modified = false;

    loop {
        // Show current assets
        let current = get_soundbank_assets(soundbank);
        output_simple("\nCurrent soundbank contents:");
        if current.is_empty() {
            output_simple("  (empty)");
        } else {
            for (type_key, path) in &current {
                output_simple(&format!("  [{}] {}", type_key, path));
            }
        }
        output_simple(&format!("  Total: {} asset(s)", soundbank.asset_count()));

        let options = vec![
            "Add asset".to_string(),
            "Remove asset".to_string(),
            "Done".to_string(),
        ];

        let choice = match select_index(input, "\nWhat would you like to do?", &options) {
            Ok(idx) => idx,
            Err(_) => break,
        };

        match choice {
            0 => {
                // Add asset
                let available = collect_available_assets(sources_dir);
                let current = get_soundbank_assets(soundbank);

                let add_options: Vec<String> = available
                    .iter()
                    .filter(|(t, _n, p)| !current.iter().any(|(ct, cp)| ct == t && cp == p))
                    .map(|(t, n, _p)| format!("[{}] {}", t, n))
                    .collect();

                if add_options.is_empty() {
                    output_simple("All available assets are already included.");
                    continue;
                }

                let remaining: Vec<&(String, String, String)> = available
                    .iter()
                    .filter(|(t, _n, p)| !current.iter().any(|(ct, cp)| ct == t && cp == p))
                    .collect();

                match select_index(input, "Select asset to add:", &add_options) {
                    Ok(idx) => {
                        let (type_key, _name, path) = &remaining[idx];
                        add_asset_to_soundbank(soundbank, type_key, path.clone());
                        modified = true;
                    }
                    Err(_) => continue,
                }
            }
            1 => {
                // Remove asset
                let current = get_soundbank_assets(soundbank);
                if current.is_empty() {
                    output_simple("No assets to remove.");
                    continue;
                }

                let remove_options: Vec<String> = current
                    .iter()
                    .map(|(t, p)| format!("[{}] {}", t, p))
                    .collect();

                match select_index(input, "Select asset to remove:", &remove_options) {
                    Ok(idx) => {
                        let (type_key, path) = &current[idx];
                        remove_asset_from_soundbank(soundbank, type_key, path);
                        modified = true;
                    }
                    Err(_) => continue,
                }
            }
            _ => break, // Done
        }
    }

    Ok(modified)
}

// =============================================================================
// Delete
// =============================================================================

/// Delete a soundbank asset.
async fn delete_soundbank(
    name: &str,
    yes: bool,
    input: &dyn Input,
    output: &dyn Output,
) -> Result<()> {
    // Step 1: Detect project
    let current_dir = env::current_dir()?;
    read_amproject_file(&current_dir)?;

    // Step 2: Locate soundbank file
    let soundbanks_dir = current_dir.join("sources").join("soundbanks");
    let soundbank_file_path = soundbanks_dir.join(format!("{}.json", name));

    if !soundbank_file_path.exists() {
        return Err(asset_not_found(ASSET_NAME, name)
            .with_suggestion("Use 'am asset soundbank list' to see available soundbanks")
            .into());
    }

    // Step 3: Parse soundbank for details
    let content = fs::read_to_string(&soundbank_file_path)?;
    let soundbank: Soundbank = serde_json::from_str(&content)?;

    // Step 4: Check for orphaned assets (informational only)
    // An asset is "orphaned" if this is the only soundbank containing it.
    // This is informational, not a blocker.
    let orphan_info = check_orphaned_assets(&soundbank, &current_dir);

    // Step 5: Confirmation prompt
    if !yes {
        output_simple(&format!(
            "\n{} You are about to delete the following soundbank:",
            "⚠".yellow()
        ));
        output_simple(&format!("  Name: {}", soundbank.name()));
        output_simple(&format!("  ID: {}", soundbank.id));
        output_simple(&format!("  Assets: {}", soundbank.asset_count()));
        output_simple(&format!("  File: {}\n", soundbank_file_path.display()));

        if !orphan_info.is_empty() {
            output_simple(&format!(
                "{} The following assets will no longer be in any soundbank after deletion:",
                "ℹ".blue()
            ));
            for orphan in &orphan_info {
                output_simple(&format!("  - {}", orphan));
            }
            output_simple("");
        }

        let confirmed = match input.confirm(
            "Are you sure you want to delete this soundbank?",
            Some(false),
        ) {
            Ok(val) => val,
            Err(_) => {
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Deletion requires confirmation",
                    "The --yes flag is required in non-interactive mode",
                )
                .with_suggestion("Use --yes to confirm deletion")
                .into());
            }
        };

        if !confirmed {
            output.progress("Deletion cancelled.");
            return Ok(());
        }
    }

    // Step 6: Delete the file
    fs::remove_file(&soundbank_file_path)?;

    // Step 7: Output success
    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "id": soundbank.id,
                    "name": soundbank.name(),
                    "deleted": true,
                    "orphaned_assets": orphan_info,
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.success(
                json!(format!("Soundbank '{}' deleted successfully", name)),
                None,
            );
            if !orphan_info.is_empty() {
                output.progress(&format!(
                    "{} {} asset(s) are no longer in any soundbank",
                    "ℹ".blue(),
                    orphan_info.len()
                ));
            }
        }
    }

    Ok(())
}

/// Check which assets would become orphaned (not in any other soundbank) after deletion.
fn check_orphaned_assets(soundbank: &Soundbank, project_root: &std::path::Path) -> Vec<String> {
    let soundbanks_dir = project_root.join("sources").join("soundbanks");

    // Load all other soundbanks
    let mut other_assets: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Ok(entries) = fs::read_dir(&soundbanks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(other) = serde_json::from_str::<Soundbank>(&content) {
                        // Skip self
                        if other.id == soundbank.id {
                            continue;
                        }
                        // Collect all asset paths from other soundbanks
                        for (_, asset_path) in get_soundbank_assets(&other) {
                            other_assets.insert(asset_path);
                        }
                    }
                }
            }
        }
    }

    // Find assets in this soundbank that aren't in any other
    let our_assets = get_soundbank_assets(soundbank);
    our_assets
        .into_iter()
        .filter(|(_, path)| !other_assets.contains(path))
        .map(|(type_key, path)| format!("[{}] {}", type_key, path))
        .collect()
}
