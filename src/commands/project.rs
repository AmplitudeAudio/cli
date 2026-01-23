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
    common::{
        errors::{CliError, codes, project_already_exists, project_not_initialized},
        utils::{
            count_assets_by_type, read_amproject_file,
            ASSET_DIR_ATTENUATORS, ASSET_DIR_COLLECTIONS, ASSET_DIR_EFFECTS,
            ASSET_DIR_EVENTS, ASSET_DIR_PIPELINES, ASSET_DIR_RTPC,
            ASSET_DIR_SOUNDBANKS, ASSET_DIR_SOUNDS, ASSET_DIR_SWITCH_CONTAINERS,
            ASSET_DIR_SWITCHES,
        },
    },
    database::{
        Database, db_create_project, db_forget_project, db_get_all_projects,
        db_get_project_by_name, db_get_project_by_path, db_get_template_by_name, db_get_templates,
        entities::{ProjectConfiguration, Template},
    },
    input::Input,
    presentation::Output,
};
use clap::{Subcommand, value_parser};
use inquire::{
    CustomUserError, required,
    validator::{StringValidator, Validation},
};
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

            display_project_info(&project.name, &project_path, true, project.registered_at.as_deref(), &asset_counts, output);

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
        .with_suggestion("Create a new project with 'am project init <name>' or provide a project name")
        .into());
    }

    let config = read_amproject_file(cwd)?;
    let asset_counts = count_assets_by_type(cwd).unwrap_or_default();
    let cwd_str = cwd.to_str().unwrap_or_default();
    let registered_project = db_get_project_by_path(cwd_str, database.clone())?;

    match registered_project {
        Some(project) => {
            display_project_info(&config.name, cwd, true, project.registered_at.as_deref(), &asset_counts, output);
        }
        None => {
            match output.mode() {
                crate::presentation::OutputMode::Json => {
                    display_project_info(&config.name, cwd, false, None, &asset_counts, output);
                }
                crate::presentation::OutputMode::Interactive => {
                    display_project_info_interactive(&config.name, cwd, false, None, &asset_counts, output);

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
            }
        }
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
            let json_data = build_project_info_json(name, path, registered, registered_at, asset_counts);
            output.success(json_data, None);
        }
        crate::presentation::OutputMode::Interactive => {
            display_project_info_interactive(name, path, registered, registered_at, asset_counts, output);
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
        json_value["notice"] = json!("This project is not registered in the database. Run 'am project register' to register it.");
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
    if name
        .trim()
        .chars()
        .any(|c| !c.is_alphanumeric() && c != '_' && c != '-' && c != ' ')
    {
        Ok(Validation::Invalid(
            "The project name must only contain alphanumeric characters, underscores, and hyphens."
                .into(),
        ))
    } else {
        required!("This project name is required").validate(name)
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
