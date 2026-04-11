use anyhow::Result;
use colored::*;
use log::{info, warn};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    app::Resource,
    assets::{
        Asset, AssetType, Collection, Effect, Event, ProjectContext, ProjectValidator, Sound,
        Soundbank, Switch, SwitchContainer,
    },
    common::{
        errors::{CliError, codes, project_already_exists, project_not_initialized},
        utils::{
            ASSET_DIR_ATTENUATORS, ASSET_DIR_COLLECTIONS, ASSET_DIR_EFFECTS, ASSET_DIR_EVENTS,
            ASSET_DIR_PIPELINES, ASSET_DIR_RTPC, ASSET_DIR_SOUNDBANKS, ASSET_DIR_SOUNDS,
            ASSET_DIR_SWITCH_CONTAINERS, ASSET_DIR_SWITCHES, count_assets_by_type,
            read_amproject_file, validate_project_name,
        },
    },
    config::sdk::discover_sdk,
    database::{
        Database, db_create_project, db_forget_project, db_get_all_projects,
        db_get_project_by_name, db_get_project_by_path, db_get_template_by_name, db_get_templates,
        entities::{ProjectConfiguration, Template},
    },
    input::Input,
    presentation::{Output, OutputMode},
    schema::loader::load_schemas,
};
use clap::{Subcommand, value_parser};
use inquire::{CustomUserError, validator::Validation};
use serde_json::json;

const DEFAULT_TEMPLATE: &str = "default";

/// Width of the separator line in project info display.
const PROJECT_INFO_SEPARATOR_WIDTH: usize = 40;

#[derive(Subcommand, Debug)]
pub enum ProjectCommands {
    /// Create a new project
    Init {
        /// The name of the project to create
        name: Option<String>,

        /// The project template. Must be registered
        #[arg(short, long)]
        template: Option<String>,

        /// Create a new project without registering it
        #[arg(long, value_parser = value_parser!(bool))]
        no_register: bool,
    },

    /// Register an existing project
    Register {
        #[arg(value_parser = value_parser!(PathBuf))]
        path: Option<PathBuf>,
    },

    /// Unregister a project
    Unregister {
        /// The name of the project to unregister
        name: String,

        /// Delete the project files as well
        #[arg(long, value_parser = value_parser!(bool))]
        delete_files: bool,
    },

    /// List all registered projects
    List {},

    /// Show details of a project
    Info {
        /// The name of the project (uses current directory if not provided)
        name: Option<String>,
    },

    /// Validate all assets in a project
    Validate {
        /// Validate only sounds
        #[arg(long)]
        sounds_only: bool,

        /// Validate only collections
        #[arg(long)]
        collections_only: bool,

        /// Validate only effects
        #[arg(long)]
        effects_only: bool,

        /// Validate only switches
        #[arg(long)]
        switches_only: bool,

        /// Validate only switch containers
        #[arg(long)]
        switch_containers_only: bool,

        /// Validate only events
        #[arg(long)]
        events_only: bool,

        /// Validate only soundbanks
        #[arg(long)]
        soundbanks_only: bool,
    },

    /// Build project assets for runtime consumption
    Build {
        /// Output directory (defaults to project's build directory)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Remove existing build output before generating new files
        #[arg(long)]
        clean: bool,

        /// Stop on first error instead of continuing
        #[arg(long)]
        fail_fast: bool,
    },
}

pub async fn handler(
    command: &ProjectCommands,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> anyhow::Result<()> {
    match command {
        ProjectCommands::Init {
            name,
            template,
            no_register,
        } => {
            let mut templates = db_get_templates(database.clone())?;

            templates.insert(
                0,
                Template {
                    id: Some(0),
                    name: DEFAULT_TEMPLATE.into(),
                    path: "bundled".to_string(),
                    engine: Some("generic".to_string()),
                    description: Some("Default project template for any engine".to_string()),
                    source: crate::database::entities::TemplateSource::Embedded,
                },
            );

            let mut project_name = name.clone();
            let mut project_template = template.clone();

            if project_template.is_some()
                && !templates
                    .iter()
                    .any(|t| t.name == *project_template.as_ref().unwrap())
            {
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Invalid project template",
                    "The specified template does not exist",
                )
                .with_suggestion("Use 'am template list' to see available templates")
                .into());
            }

            if project_name.is_none() {
                let ret = input.prompt_text(
                    "Project Name",
                    Some("my_project"),
                    Some(&transform_name),
                    Some(&validate_name),
                )?;

                project_name = Some(ret);
            }

            if project_template.is_none() {
                let selected_idx =
                    crate::input::select_index(input, "Project Template", &templates)?;
                project_template = Some(templates[selected_idx].name.clone());
            }

            handle_init_project_command(
                project_name.as_deref().unwrap(),
                project_template.as_deref().unwrap_or(""),
                no_register,
                database,
                input,
                output,
            )
            .await
        }
        ProjectCommands::Register { path } => {
            let cwd = env::current_dir()?;
            let project_path = match path {
                Some(path) => path,
                None => &cwd,
            };

            handle_register_project_command(project_path, database, input, output).await
        }
        ProjectCommands::Unregister {
            name,
            delete_files: delete,
        } => {
            handle_unregister_project_command(name.as_str(), delete, database, input, output).await
        }
        ProjectCommands::List {} => handle_list_projects_command(database, output).await,
        ProjectCommands::Info { name } => {
            handle_info_project_command(name.clone(), database, input, output).await
        }
        ProjectCommands::Validate {
            sounds_only,
            collections_only,
            effects_only,
            switches_only,
            switch_containers_only,
            events_only,
            soundbanks_only,
        } => {
            let filter = resolve_type_filter(
                *sounds_only,
                *collections_only,
                *effects_only,
                *switches_only,
                *switch_containers_only,
                *events_only,
                *soundbanks_only,
            );
            handle_validate_project_command(filter, output).await
        }
        ProjectCommands::Build {
            output: output_dir,
            clean,
            fail_fast,
        } => handle_build_project_command(output_dir.clone(), *clean, *fail_fast, output).await,
    }
}

async fn handle_init_project_command(
    name: &str,
    template: &str,
    no_register: &bool,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> anyhow::Result<()> {
    let project_name = transform_name(name);

    if !no_register
        && let Ok(Some(p)) = db_get_project_by_name(project_name.as_str(), database.clone())
    {
        warn!(
            "A project with the name {} is already registered at path {}",
            project_name.cyan(),
            p.path.cyan()
        );

        if input.confirm(
            "Do you want to forget that project and create this new one?",
            None,
        )? {
            info!("Unregistering previous project...");
            db_forget_project(p.id.unwrap(), database.clone())?;
        } else {
            return Err(project_already_exists(&project_name)
                .with_suggestion("Use the --no-register flag to create without registering, or choose a different name")
                .into());
        }
    }

    output.progress(
        format!(
            "Initializing project {} using template {}...",
            project_name.cyan(),
            template.cyan()
        )
        .as_str(),
    );

    let cwd = env::current_dir()?;
    let project_path = &cwd.join(&project_name);

    if project_path.exists() && project_path.read_dir()?.next().is_some() {
        warn!(
            "The project path {} already exists and is not empty",
            project_path.to_str().unwrap_or_default().cyan()
        );

        if input.confirm(
            "Do you want to overwrite the directory? All existing content will be deleted!",
            None,
        )? {
            fs::remove_dir_all(project_path)?;
        } else {
            return Err(CliError::new(
                codes::ERR_PROJECT_ALREADY_EXISTS,
                "Cannot create project",
                "The project directory already exists and is not empty",
            )
            .with_context(project_path.display().to_string())
            .into());
        }
    }

    fs::create_dir_all(project_path)?;

    if template != DEFAULT_TEMPLATE {
        if let Some(t) = db_get_template_by_name(template, database.clone())? {
            let template_path = PathBuf::from(t.path);
            if !template_path.exists() {
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Template directory does not exist",
                    "The registered template path is invalid or has been moved",
                )
                .with_context(template_path.display().to_string())
                .into());
            }

            fs::copy(&template_path, project_path).map_err(|e| {
                CliError::new(
                    codes::ERR_TEMPLATE_COPY_FAILED,
                    format!("Failed to copy template from {}", template_path.display()),
                    format!("Underlying OS error: {}", e),
                )
            })?;
        } else {
            return Err(CliError::new(
                codes::ERR_VALIDATION_FIELD,
                "Template not found",
                "The selected template is not registered",
            )
            .with_context(template)
            .with_suggestion("Use 'am template list' to see available templates")
            .into());
        }
    } else {
        let sources_dir = project_path.join("sources");

        fs::create_dir_all(sources_dir.join(ASSET_DIR_ATTENUATORS))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_COLLECTIONS))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_EFFECTS))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_EVENTS))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_PIPELINES))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_RTPC))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_SOUNDBANKS))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_SOUNDS))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_SWITCH_CONTAINERS))?;
        fs::create_dir_all(sources_dir.join(ASSET_DIR_SWITCHES))?;

        if let Some(file) = Resource::get("default.config.json") {
            fs::write(sources_dir.join("pc.config.json"), file.data)?;
        }

        if let Some(file) = Resource::get("default.buses.json") {
            fs::write(sources_dir.join("pc.buses.json"), file.data)?;
        }

        if let Some(file) = Resource::get("default.pipeline.json") {
            fs::write(
                sources_dir
                    .join(ASSET_DIR_PIPELINES)
                    .join("pc.pipeline.json"),
                file.data,
            )?;
        }

        fs::create_dir_all(project_path.join("build"))?;
        fs::create_dir_all(project_path.join("data"))?;
        fs::create_dir_all(project_path.join("plugins"))?;

        let mut amproject = fs::File::create(project_path.join(".amproject"))?;

        let project = &ProjectConfiguration {
            name: project_name,
            default_configuration: "pc.config.amconfig".to_string(),
            build_dir: "build".to_string(),
            data_dir: "data".to_string(),
            sources_dir: "sources".to_string(),
            version: 1,
        };

        if !no_register {
            register_project(project, project_path, database)?;
        }

        amproject.write_all(serde_json::to_string(project)?.as_bytes())?;
    }

    output.success(
        json!(format!("Project {} created successfully", name)),
        None,
    );

    Ok(())
}

async fn handle_register_project_command(
    path: &std::path::Path,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> anyhow::Result<()> {
    output.progress(&format!("Registering project '{}'...", path.display()));
    let amproject = path.join(".amproject");

    if !amproject.exists() {
        return Err(project_not_initialized(path.to_str().unwrap_or_default()).into());
    }

    let amproject_content = fs::read_to_string(&amproject)?;
    let project_config: ProjectConfiguration = serde_json::from_str(&amproject_content)?;
    let project_name = project_config.name.clone();

    if let Ok(Some(p)) = db_get_project_by_name(project_name.as_str(), database.clone()) {
        warn!(
            "A project with the name {} is already registered at path {}",
            project_name.cyan(),
            p.path.cyan()
        );

        if input.confirm(
            "Do you want to forget that project and register this one?",
            None,
        )? {
            info!("Unregistering previous project...");
            db_forget_project(p.id.unwrap(), database.clone())?;
        } else {
            return Err(project_already_exists(&project_name)
                .with_suggestion("Unregister the existing project first, or use a different name")
                .into());
        }
    }

    register_project(&project_config, path, database)?;

    output.success(
        json!(format!(
            "Project {} registered successfully",
            project_config.name
        )),
        None,
    );

    Ok(())
}

async fn handle_unregister_project_command(
    name: &str,
    delete: &bool,
    database: Option<Arc<Database>>,
    _input: &dyn Input,
    output: &dyn Output,
) -> anyhow::Result<()> {
    if let Ok(Some(p)) = db_get_project_by_name(name, database.clone()) {
        output.progress("Unregistering project...");
        db_forget_project(p.id.unwrap(), database.clone())?;

        if *delete && fs::exists(p.path.clone())? {
            output.progress("Deleting project directory...");
            fs::remove_dir_all(p.path)?;
        }
    }

    output.success(
        json!(format!("Project {} unregistered successfully", name)),
        None,
    );

    Ok(())
}

async fn handle_list_projects_command(
    database: Option<Arc<Database>>,
    output: &dyn Output,
) -> anyhow::Result<()> {
    let projects = db_get_all_projects(database)?;

    if projects.is_empty() {
        output.table(Some("Registered Projects"), json!([]));

        output.progress("No projects registered.");
        output.progress("");
        output.progress("To get started:");
        output.progress(&format!(
            "  {} Create a new project: {} {}",
            "•".cyan(),
            "am project init".green(),
            "<name>".white()
        ));
        output.progress(&format!(
            "  {} Register an existing project: {} {}",
            "•".cyan(),
            "am project register".green(),
            "<path>".white()
        ));
    } else {
        let display_data: Vec<serde_json::Value> = projects
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "path": p.path,
                    "registered_at": p.registered_at.clone().unwrap_or_else(|| "-".to_string())
                })
            })
            .collect();

        output.table(Some("Registered Projects"), json!(display_data));
    }

    Ok(())
}

async fn handle_info_project_command(
    name: Option<String>,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> anyhow::Result<()> {
    if let Some(project_name) = name {
        return handle_info_by_name(&project_name, database, output).await;
    }

    let cwd = env::current_dir()?;
    handle_info_current_dir(&cwd, database, input, output).await
}

async fn handle_info_by_name(
    name: &str,
    database: Option<Arc<Database>>,
    output: &dyn Output,
) -> anyhow::Result<()> {
    let lookup_result = db_get_project_by_name(name, database.clone())?;

    match lookup_result {
        Some(project) => {
            let project_path = PathBuf::from(&project.path);
            let asset_counts = count_assets_by_type(&project_path).unwrap_or_default();

            display_project_info(
                &project.name,
                &project_path,
                true,
                project.registered_at.as_deref(),
                &asset_counts,
                output,
            );

            Ok(())
        }
        None => Err(CliError::new(
            codes::ERR_PROJECT_NOT_REGISTERED,
            format!("Project '{}' not found", name),
            "The project is not registered in the database",
        )
        .with_suggestion("Use 'am project list' to see registered projects")
        .into()),
    }
}

async fn handle_info_current_dir(
    cwd: &std::path::Path,
    database: Option<Arc<Database>>,
    input: &dyn Input,
    output: &dyn Output,
) -> anyhow::Result<()> {
    let amproject_path = cwd.join(".amproject");
    if !amproject_path.exists() {
        return Err(CliError::new(
            codes::ERR_PROJECT_NOT_INITIALIZED,
            "No project found in current directory",
            "The current directory does not contain a .amproject file",
        )
        .with_suggestion(
            "Create a new project with 'am project init <name>' or provide a project name",
        )
        .into());
    }

    let config = read_amproject_file(cwd)?;
    let asset_counts = count_assets_by_type(cwd).unwrap_or_default();
    let cwd_str = cwd.to_str().unwrap_or_default();
    let registered_project = db_get_project_by_path(cwd_str, database.clone())?;

    match registered_project {
        Some(project) => {
            display_project_info(
                &config.name,
                cwd,
                true,
                project.registered_at.as_deref(),
                &asset_counts,
                output,
            );
        }
        None => match output.mode() {
            crate::presentation::OutputMode::Json => {
                display_project_info(&config.name, cwd, false, None, &asset_counts, output);
            }
            crate::presentation::OutputMode::Interactive => {
                display_project_info_interactive(
                    &config.name,
                    cwd,
                    false,
                    None,
                    &asset_counts,
                    output,
                );

                output.progress("");
                output.progress("This project is not registered in the database.");

                match input.confirm("Would you like to register it now?", Some(false)) {
                    Ok(true) => {
                        let project = config.to_project(cwd_str);
                        db_create_project(&project, database)?;
                        output.success(json!("Project registered successfully!"), None);
                    }
                    Ok(false) | Err(_) => {
                        output.progress(&format!(
                            "  Run {} to register this project",
                            "am project register".green()
                        ));
                    }
                }
            }
        },
    }

    Ok(())
}

fn display_project_info(
    name: &str,
    path: &std::path::Path,
    registered: bool,
    registered_at: Option<&str>,
    asset_counts: &HashMap<String, usize>,
    output: &dyn Output,
) {
    match output.mode() {
        crate::presentation::OutputMode::Json => {
            let json_data =
                build_project_info_json(name, path, registered, registered_at, asset_counts);
            output.success(json_data, None);
        }
        crate::presentation::OutputMode::Interactive => {
            display_project_info_interactive(
                name,
                path,
                registered,
                registered_at,
                asset_counts,
                output,
            );
        }
    }
}

fn build_project_info_json(
    name: &str,
    path: &std::path::Path,
    registered: bool,
    registered_at: Option<&str>,
    asset_counts: &HashMap<String, usize>,
) -> serde_json::Value {
    let path_str = path.to_str().unwrap_or_default();

    let mut json_value = json!({
        "name": name,
        "path": path_str,
        "registered": registered,
        "paths": {
            "sources": format!("{}/sources", path_str),
            "data": format!("{}/data", path_str),
            "build": format!("{}/build", path_str),
        },
        "assets": {
            "sounds": asset_counts.get("sounds").unwrap_or(&0),
            "collections": asset_counts.get("collections").unwrap_or(&0),
            "events": asset_counts.get("events").unwrap_or(&0),
            "effects": asset_counts.get("effects").unwrap_or(&0),
            "switches": asset_counts.get("switches").unwrap_or(&0),
            "switch_containers": asset_counts.get("switch_containers").unwrap_or(&0),
            "soundbanks": asset_counts.get("soundbanks").unwrap_or(&0),
            "attenuators": asset_counts.get("attenuators").unwrap_or(&0),
            "rtpc": asset_counts.get("rtpc").unwrap_or(&0),
            "pipelines": asset_counts.get("pipelines").unwrap_or(&0),
        }
    });

    if registered {
        if let Some(date) = registered_at {
            json_value["registered_at"] = json!(date);
        }
    } else {
        json_value["notice"] = json!(
            "This project is not registered in the database. Run 'am project register' to register it."
        );
    }

    json_value
}

fn display_project_info_interactive(
    name: &str,
    path: &std::path::Path,
    registered: bool,
    registered_at: Option<&str>,
    asset_counts: &HashMap<String, usize>,
    output: &dyn Output,
) {
    let path_str = path.to_str().unwrap_or_default();

    output.progress(&format!("Project: {}", name.cyan().bold()));
    output.progress(&"─".repeat(PROJECT_INFO_SEPARATOR_WIDTH));
    output.progress("");
    output.progress("Details:");
    output.progress(&format!("  Root Path:      {}", path_str));
    if registered {
        output.progress(&format!(
            "  Registered:     {} ({})",
            "Yes".green(),
            registered_at.unwrap_or("-")
        ));
    } else {
        output.progress(&format!("  Registered:     {}", "No".yellow()));
    }
    output.progress("");
    output.progress("Paths:");
    output.progress(&format!("  Sources:        {}/sources", path_str));
    output.progress(&format!("  Data:           {}/data", path_str));
    output.progress(&format!("  Build:          {}/build", path_str));

    let has_assets = asset_counts.values().any(|&v| v > 0);
    if has_assets {
        output.progress("");
        output.progress("Assets:");
        if let Some(&count) = asset_counts.get("sounds") {
            if count > 0 {
                output.progress(&format!("  Sounds:         {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("collections") {
            if count > 0 {
                output.progress(&format!("  Collections:    {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("events") {
            if count > 0 {
                output.progress(&format!("  Events:         {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("effects") {
            if count > 0 {
                output.progress(&format!("  Effects:        {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("switches") {
            if count > 0 {
                output.progress(&format!("  Switches:       {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("switch_containers") {
            if count > 0 {
                output.progress(&format!("  Switch Cont.:   {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("soundbanks") {
            if count > 0 {
                output.progress(&format!("  Soundbanks:     {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("attenuators") {
            if count > 0 {
                output.progress(&format!("  Attenuators:    {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("rtpc") {
            if count > 0 {
                output.progress(&format!("  RTPC:           {}", count));
            }
        }
        if let Some(&count) = asset_counts.get("pipelines") {
            if count > 0 {
                output.progress(&format!("  Pipelines:      {}", count));
            }
        }
    }
}

fn validate_name(name: &str) -> Result<Validation, CustomUserError> {
    match validate_project_name(name) {
        Ok(()) => Ok(Validation::Valid),
        Err(msg) => Ok(Validation::Invalid(msg.into())),
    }
}

fn transform_name(name: &str) -> String {
    name.to_lowercase().replace([' ', '-'], "_")
}

fn register_project(
    config: &ProjectConfiguration,
    path: &std::path::Path,
    database: Option<Arc<Database>>,
) -> Result<bool> {
    info!("Registering project {}...", config.name.cyan());

    db_create_project(&config.to_project(path.to_str().unwrap()), database.clone())
}

// =============================================================================
// Validate Command
// =============================================================================

/// Resolve which asset types to validate based on filter flags.
/// If no flags are set, returns None (validate all types).
fn resolve_type_filter(
    sounds_only: bool,
    collections_only: bool,
    effects_only: bool,
    switches_only: bool,
    switch_containers_only: bool,
    events_only: bool,
    soundbanks_only: bool,
) -> Option<Vec<AssetType>> {
    let mut types = Vec::new();

    if sounds_only {
        types.push(AssetType::Sound);
    }
    if collections_only {
        types.push(AssetType::Collection);
    }
    if effects_only {
        types.push(AssetType::Effect);
    }
    if switches_only {
        types.push(AssetType::Switch);
    }
    if switch_containers_only {
        types.push(AssetType::SwitchContainer);
    }
    if events_only {
        types.push(AssetType::Event);
    }
    if soundbanks_only {
        types.push(AssetType::Soundbank);
    }

    if types.is_empty() { None } else { Some(types) }
}

/// A single validation error with file context.
#[derive(Debug)]
struct ValidationResult {
    file: String,
    asset_type: AssetType,
    error: String,
    why: String,
    suggestion: String,
    field: Option<String>,
}

/// Validate all assets in the current project.
async fn handle_validate_project_command(
    type_filter: Option<Vec<AssetType>>,
    output: &dyn Output,
) -> Result<()> {
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Validating project '{}'...",
        project_config.name
    ));

    // Try to discover SDK for schema validation
    let sdk_available = match discover_sdk() {
        Ok(sdk) => {
            match load_schemas(&sdk) {
                Ok(registry) => {
                    output.progress(&format!(
                        "SDK found: loaded {} schema(s) from {}",
                        registry.schema_count(),
                        sdk.schemas_dir().display()
                    ));
                    for (path, err) in registry.failed_files() {
                        output.progress(&format!(
                            "{} Failed to load schema {}: {}",
                            "⚠".yellow(),
                            path.display(),
                            err
                        ));
                    }
                    true
                }
                Err(e) => {
                    output.progress(&format!(
                        "{} Schema loading failed: {}. Schema validation will be skipped.",
                        "⚠".yellow(),
                        e.what
                    ));
                    false
                }
            }
        }
        Err(_) => {
            output.progress(&format!(
                "{} SDK not found. Schema validation will be skipped.",
                "⚠".yellow()
            ));
            output.progress("  Set AM_SDK_PATH for full validation.");
            false
        }
    };

    // Build project context with validator for cross-reference checking
    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    // Determine which types to validate
    let types_to_validate: Vec<AssetType> = type_filter.unwrap_or_else(|| {
        vec![
            AssetType::Sound,
            AssetType::Collection,
            AssetType::Effect,
            AssetType::Switch,
            AssetType::SwitchContainer,
            AssetType::Event,
            AssetType::Soundbank,
        ]
    });

    let sources_dir = current_dir.join("sources");
    let mut errors: Vec<ValidationResult> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut asset_summary: HashMap<String, usize> = HashMap::new();
    let mut total_validated: usize = 0;

    for asset_type in &types_to_validate {
        let dir = sources_dir.join(asset_type.directory_name());
        if !dir.exists() {
            continue;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                warnings.push(format!(
                    "Cannot read {} directory: {}",
                    asset_type.directory_name(),
                    e
                ));
                continue;
            }
        };

        let mut type_count: usize = 0;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }

            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let relative_path = format!("sources/{}/{}", asset_type.directory_name(), filename);

            output.progress(&format!("  Validating {}...", relative_path));

            // Read and parse the file
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(ValidationResult {
                        file: relative_path,
                        asset_type: *asset_type,
                        error: format!("Failed to read file: {}", e),
                        why: "The file could not be read".to_string(),
                        suggestion: "Check file permissions".to_string(),
                        field: None,
                    });
                    continue;
                }
            };

            type_count += 1;

            // Validate based on asset type
            let validation_errors =
                validate_asset_file(*asset_type, &content, &relative_path, &context);
            errors.extend(validation_errors);
        }

        asset_summary.insert(asset_type.directory_name().to_string(), type_count);
        total_validated += type_count;
    }

    // Output results
    let is_valid = errors.is_empty();

    match output.mode() {
        OutputMode::Json => {
            let error_data: Vec<serde_json::Value> = errors
                .iter()
                .map(|e| {
                    let mut obj = json!({
                        "file": e.file,
                        "type": format!("{}", e.asset_type),
                        "error": e.error,
                        "why": e.why,
                        "fix": e.suggestion,
                    });
                    if let Some(ref field) = e.field {
                        obj["field"] = json!(field);
                    }
                    obj
                })
                .collect();

            let result = json!({
                "valid": is_valid,
                "errors": error_data,
                "warnings": warnings,
                "summary": asset_summary,
                "total_validated": total_validated,
                "sdk_available": sdk_available,
            });

            // In JSON mode, always output the structured result
            output.success(result, None);
        }
        OutputMode::Interactive => {
            if !warnings.is_empty() {
                for w in &warnings {
                    output.progress(&format!("{} {}", "⚠".yellow(), w));
                }
                output.progress("");
            }

            if is_valid {
                output.progress("");
                output.success(
                    json!(format!(
                        "All {} asset(s) validated successfully!",
                        total_validated
                    )),
                    None,
                );

                // Print summary
                output.progress("");
                output.progress("Summary:");
                for (type_name, count) in &asset_summary {
                    if *count > 0 {
                        output.progress(&format!("  {}: {} {}", type_name, count, "✓".green()));
                    }
                }
                if !sdk_available {
                    output.progress(&format!(
                        "\n{} Schema validation was skipped (SDK not available)",
                        "ℹ".blue()
                    ));
                }
            } else {
                output.progress("");
                output.progress(&format!(
                    "{} Validation failed: {} error(s) found\n",
                    "✗".red(),
                    errors.len()
                ));

                for err in &errors {
                    output.progress(&format!("  {} {}", "Error:".red().bold(), err.error));
                    output.progress(&format!("    File:  {}", err.file));
                    if let Some(ref field) = err.field {
                        output.progress(&format!("    Field: {}", field));
                    }
                    output.progress(&format!("    Why:   {}", err.why));
                    output.progress(&format!("    Fix:   {}", err.suggestion));
                    output.progress("");
                }
            }
        }
    }

    if !is_valid {
        return Err(CliError::new(
            codes::ERR_VALIDATION_SCHEMA,
            format!(
                "Project validation failed: {} error(s) in {} asset(s)",
                errors.len(),
                total_validated
            ),
            "Fix the reported errors and run validation again",
        )
        .into());
    }

    Ok(())
}

/// Validate a single asset file by deserializing and running type rules.
fn validate_asset_file(
    asset_type: AssetType,
    content: &str,
    file_path: &str,
    context: &ProjectContext,
) -> Vec<ValidationResult> {
    match asset_type {
        AssetType::Sound => validate_typed_asset::<Sound>(asset_type, content, file_path, context),
        AssetType::Collection => {
            validate_typed_asset::<Collection>(asset_type, content, file_path, context)
        }
        AssetType::Effect => {
            validate_typed_asset::<Effect>(asset_type, content, file_path, context)
        }
        AssetType::Switch => {
            validate_typed_asset::<Switch>(asset_type, content, file_path, context)
        }
        AssetType::SwitchContainer => {
            validate_typed_asset::<SwitchContainer>(asset_type, content, file_path, context)
        }
        AssetType::Event => {
            validate_typed_asset::<Event>(asset_type, content, file_path, context)
        }
        AssetType::Soundbank => {
            validate_typed_asset::<Soundbank>(asset_type, content, file_path, context)
        }
    }
}

/// Generic validation for any Asset type.
fn validate_typed_asset<T: Asset>(
    asset_type: AssetType,
    content: &str,
    file_path: &str,
    context: &ProjectContext,
) -> Vec<ValidationResult> {
    let mut results = Vec::new();

    // Step 1: Deserialize
    let asset: T = match serde_json::from_str(content) {
        Ok(a) => a,
        Err(e) => {
            results.push(ValidationResult {
                file: file_path.to_string(),
                asset_type,
                error: format!("Invalid JSON structure: {}", e),
                why: "The file does not match the expected schema for this asset type".to_string(),
                suggestion: "Check JSON syntax and ensure all required fields are present"
                    .to_string(),
                field: None,
            });
            return results;
        }
    };

    // Step 2: Validate business rules
    if let Err(validation_err) = asset.validate_rules(context) {
        results.push(ValidationResult {
            file: file_path.to_string(),
            asset_type,
            error: validation_err.what().to_string(),
            why: validation_err.why().to_string(),
            suggestion: validation_err.suggestion().to_string(),
            field: validation_err.field.clone(),
        });
    }

    results
}

// =============================================================================
// Build Command
// =============================================================================

/// Build project assets for runtime consumption.
///
/// Steps:
/// 1. Require SDK availability
/// 2. Validate all assets (fail-fast on errors)
/// 3. Copy asset JSON files to build output directory
/// 4. Copy audio data files to build output directory
/// 5. Report summary
async fn handle_build_project_command(
    output_dir: Option<PathBuf>,
    clean: bool,
    fail_fast: bool,
    output: &dyn Output,
) -> Result<()> {
    let current_dir = env::current_dir()?;
    let project_config = read_amproject_file(&current_dir)?;

    output.progress(&format!(
        "Building project '{}'...",
        project_config.name
    ));

    // Step 1: Require SDK
    let sdk = match discover_sdk() {
        Ok(sdk) => sdk,
        Err(e) => {
            return Err(CliError::new(
                codes::ERR_SDK_NOT_FOUND,
                "SDK is required for build operations",
                e.what,
            )
            .with_suggestion(
                "Set the AM_SDK_PATH environment variable to your SDK installation path",
            )
            .into());
        }
    };

    output.progress(&format!("SDK found at {}", sdk.root().display()));

    // Step 2: Validate all assets first
    output.progress("Running validation...");

    let validator = ProjectValidator::new(current_dir.clone())?;
    let context = ProjectContext::new(current_dir.clone()).with_validator(validator);

    let sources_dir = current_dir.join("sources");
    let asset_types = vec![
        AssetType::Sound,
        AssetType::Collection,
        AssetType::Effect,
        AssetType::Switch,
        AssetType::SwitchContainer,
        AssetType::Event,
        AssetType::Soundbank,
    ];

    let mut validation_errors: Vec<ValidationResult> = Vec::new();

    for asset_type in &asset_types {
        let dir = sources_dir.join(asset_type.directory_name());
        if !dir.exists() {
            continue;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }

            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let relative_path = format!("sources/{}/{}", asset_type.directory_name(), filename);

            if let Ok(content) = fs::read_to_string(&path) {
                let errs = validate_asset_file(*asset_type, &content, &relative_path, &context);
                if !errs.is_empty() {
                    validation_errors.extend(errs);
                    if fail_fast {
                        break;
                    }
                }
            }
        }

        if fail_fast && !validation_errors.is_empty() {
            break;
        }
    }

    if !validation_errors.is_empty() {
        // Display validation errors and abort
        match output.mode() {
            OutputMode::Interactive => {
                output.progress(&format!(
                    "\n{} Validation failed: {} error(s). Build aborted.\n",
                    "✗".red(),
                    validation_errors.len()
                ));
                for err in &validation_errors {
                    output.progress(&format!("  {} {}", "Error:".red().bold(), err.error));
                    output.progress(&format!("    File: {}", err.file));
                    if let Some(ref field) = err.field {
                        output.progress(&format!("    Field: {}", field));
                    }
                    output.progress(&format!("    Fix:   {}", err.suggestion));
                    output.progress("");
                }
            }
            OutputMode::Json => {
                let error_data: Vec<serde_json::Value> = validation_errors
                    .iter()
                    .map(|e| {
                        json!({
                            "file": e.file,
                            "error": e.error,
                            "fix": e.suggestion,
                        })
                    })
                    .collect();

                output.success(
                    json!({
                        "built": false,
                        "validation_errors": error_data,
                    }),
                    None,
                );
            }
        }

        return Err(CliError::new(
            codes::ERR_VALIDATION_SCHEMA,
            format!(
                "Build aborted: {} validation error(s)",
                validation_errors.len()
            ),
            "Fix validation errors before building",
        )
        .into());
    }

    output.progress("Validation passed.");

    // Step 3: Determine output directory
    let build_dir = match output_dir {
        Some(dir) => dir,
        None => current_dir.join(&project_config.build_dir),
    };

    // Step 4: Clean if requested
    if clean && build_dir.exists() {
        output.progress(&format!("Cleaning build directory: {}...", build_dir.display()));
        fs::remove_dir_all(&build_dir)?;
    }

    fs::create_dir_all(&build_dir)?;

    // Step 5: Copy asset files to build directory
    output.progress("Copying assets...");

    let mut copied_assets: HashMap<String, usize> = HashMap::new();
    let mut copy_errors: Vec<(String, String)> = Vec::new();
    let mut total_size: u64 = 0;

    for asset_type in &asset_types {
        let src_dir = sources_dir.join(asset_type.directory_name());
        if !src_dir.exists() {
            continue;
        }

        let dest_dir = build_dir.join(asset_type.directory_name());
        fs::create_dir_all(&dest_dir)?;

        let entries = match fs::read_dir(&src_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut type_count: usize = 0;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }

            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let dest_path = dest_dir.join(&filename);
            let relative = format!("sources/{}/{}", asset_type.directory_name(), filename);

            match fs::copy(&path, &dest_path) {
                Ok(bytes) => {
                    total_size += bytes;
                    type_count += 1;
                    output.progress(&format!("  Copied {}", relative));
                }
                Err(e) => {
                    let err_msg = format!("Failed to copy {}: {}", relative, e);
                    copy_errors.push((relative, err_msg.clone()));
                    output.progress(&format!("  {} {}", "✗".red(), err_msg));

                    if fail_fast {
                        return Err(CliError::new(
                            codes::ERR_VALIDATION_FIELD,
                            format!("Build failed: could not copy {}", filename),
                            format!("I/O error: {}", e),
                        )
                        .into());
                    }
                }
            }
        }

        if type_count > 0 {
            copied_assets.insert(asset_type.directory_name().to_string(), type_count);
        }
    }

    // Step 6: Copy data files (audio files)
    let data_dir = current_dir.join(&project_config.data_dir);
    let mut data_files_copied: usize = 0;

    if data_dir.exists() {
        let dest_data_dir = build_dir.join("data");
        fs::create_dir_all(&dest_data_dir)?;

        output.progress("Copying data files...");
        match copy_dir_recursive(&data_dir, &dest_data_dir, fail_fast) {
            Ok((count, bytes, errors)) => {
                data_files_copied = count;
                total_size += bytes;
                for (file, err) in &errors {
                    copy_errors.push((file.clone(), err.clone()));
                    output.progress(&format!("  {} {}", "✗".red(), err));
                }
            }
            Err(e) => {
                return Err(CliError::new(
                    codes::ERR_VALIDATION_FIELD,
                    "Failed to copy data directory",
                    format!("I/O error: {}", e),
                )
                .into());
            }
        }
    }

    // Step 7: Copy config files (*.config.json, *.buses.json) from sources root
    let config_patterns = ["config.json", "buses.json"];
    if let Ok(entries) = fs::read_dir(&sources_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                if config_patterns.iter().any(|pat| filename.ends_with(pat)) {
                    let dest = build_dir.join(&filename);
                    match fs::copy(&path, &dest) {
                        Ok(bytes) => {
                            total_size += bytes;
                            output.progress(&format!("  Copied {}", filename));
                        }
                        Err(e) => {
                            copy_errors.push((filename.clone(), format!("Failed to copy {}: {}", filename, e)));
                        }
                    }
                }
            }
        }
    }

    // Step 8: Report results
    let total_assets: usize = copied_assets.values().sum();
    let has_errors = !copy_errors.is_empty();

    match output.mode() {
        OutputMode::Json => {
            output.success(
                json!({
                    "built": !has_errors,
                    "output_path": build_dir.to_string_lossy(),
                    "assets": copied_assets,
                    "data_files": data_files_copied,
                    "total_assets": total_assets,
                    "size_bytes": total_size,
                    "errors": copy_errors.iter().map(|(f, e)| json!({"file": f, "error": e})).collect::<Vec<_>>(),
                }),
                None,
            );
        }
        OutputMode::Interactive => {
            output.progress("");

            if has_errors {
                output.progress(&format!(
                    "{} Build completed with {} error(s):\n",
                    "⚠".yellow(),
                    copy_errors.len()
                ));
                for (_file, err) in &copy_errors {
                    output.progress(&format!("  {} {}", "✗".red(), err));
                }
                output.progress("");
            }

            output.success(
                json!(format!(
                    "Build complete: {} asset(s), {} data file(s) -> {}",
                    total_assets,
                    data_files_copied,
                    build_dir.display()
                )),
                None,
            );

            output.progress("");
            output.progress("Summary:");
            for (type_name, count) in &copied_assets {
                output.progress(&format!("  {}: {}", type_name, count));
            }
            if data_files_copied > 0 {
                output.progress(&format!("  data files: {}", data_files_copied));
            }
            output.progress(&format!("  Total size: {} bytes", total_size));
        }
    }

    if has_errors {
        return Err(CliError::new(
            codes::ERR_VALIDATION_FIELD,
            format!("Build completed with {} error(s)", copy_errors.len()),
            "Some files failed to copy",
        )
        .with_suggestion("Check file permissions and disk space")
        .into());
    }

    Ok(())
}

/// Recursively copy a directory's contents.
/// Returns (files_copied, total_bytes, errors).
fn copy_dir_recursive(
    src: &std::path::Path,
    dest: &std::path::Path,
    fail_fast: bool,
) -> Result<(usize, u64, Vec<(String, String)>)> {
    let mut count = 0;
    let mut bytes = 0u64;
    let mut errors = Vec::new();

    let entries = fs::read_dir(src)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let filename = path.file_name().unwrap_or_default();
        let dest_path = dest.join(filename);

        if path.is_dir() {
            fs::create_dir_all(&dest_path)?;
            let (sub_count, sub_bytes, sub_errors) =
                copy_dir_recursive(&path, &dest_path, fail_fast)?;
            count += sub_count;
            bytes += sub_bytes;
            errors.extend(sub_errors);

            if fail_fast && !errors.is_empty() {
                return Ok((count, bytes, errors));
            }
        } else {
            match fs::copy(&path, &dest_path) {
                Ok(b) => {
                    count += 1;
                    bytes += b;
                }
                Err(e) => {
                    let rel = path.to_string_lossy().to_string();
                    errors.push((rel.clone(), format!("Failed to copy {}: {}", rel, e)));

                    if fail_fast {
                        return Ok((count, bytes, errors));
                    }
                }
            }
        }
    }

    Ok((count, bytes, errors))
}
