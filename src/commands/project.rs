use anyhow::Result;
use colored::*;
use log::{info, warn};
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    app::Resource,
    common::errors::{CliError, codes, project_already_exists, project_not_initialized},
    database::{
        Database, db_create_project, db_forget_project, db_get_project_by_name,
        db_get_template_by_name, db_get_templates,
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

const PROJECT_DIR_ATTENUATORS: &str = "attenuators";
const PROJECT_DIR_COLLECTIONS: &str = "collections";
const PROJECT_DIR_EFFECTS: &str = "effects";
const PROJECT_DIR_EVENTS: &str = "events";
const PROJECT_DIR_PIPELINES: &str = "pipelines";
const PROJECT_DIR_RTPC: &str = "rtpc";
const PROJECT_DIR_SOUND_BANKS: &str = "soundbanks";
const PROJECT_DIR_SOUNDS: &str = "sounds";
const PROJECT_DIR_SWITCH_CONTAINERS: &str = "switch_containers";
const PROJECT_DIR_SWITCHES: &str = "switches";
const DEFAULT_TEMPLATE: &str = "default";

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

    if !no_register {
        if let Some(Some(p)) = db_get_project_by_name(project_name.as_str(), database.clone()).ok()
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

    fs::create_dir_all(&project_path)?;

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

            fs::copy(&template_path, &project_path).map_err(|e| {
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

        // Create project 'sources' directories
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_ATTENUATORS))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_COLLECTIONS))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_EFFECTS))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_EVENTS))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_PIPELINES))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_RTPC))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_SOUND_BANKS))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_SOUNDS))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_SWITCH_CONTAINERS))?;
        fs::create_dir_all(sources_dir.join(PROJECT_DIR_SWITCHES))?;

        if let Some(file) = Resource::get("default.config.json") {
            fs::write(sources_dir.join("pc.config.json"), file.data)?;
        }

        if let Some(file) = Resource::get("default.buses.json") {
            fs::write(sources_dir.join("pc.buses.json"), file.data)?;
        }

        if let Some(file) = Resource::get("default.pipeline.json") {
            fs::write(
                sources_dir
                    .join(PROJECT_DIR_PIPELINES)
                    .join("pc.pipeline.json"),
                file.data,
            )?;
        }

        // Create the project's 'build' directory
        fs::create_dir_all(project_path.join("build"))?;

        // Create the project's 'data' directory
        fs::create_dir_all(project_path.join("data"))?;

        // Create the project's 'plugins' directory
        fs::create_dir_all(project_path.join("plugins"))?;

        // Create the project's file
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
    path: &PathBuf,
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

    if let Some(Some(p)) = db_get_project_by_name(project_name.as_str(), database.clone()).ok() {
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
    if let Some(Some(p)) = db_get_project_by_name(name, database.clone()).ok() {
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
    name.to_lowercase().replace(' ', "_").replace('-', "_")
}

fn register_project(
    config: &ProjectConfiguration,
    path: &PathBuf,
    database: Option<Arc<Database>>,
) -> Result<bool> {
    info!("Registering project {}...", config.name.cyan());

    db_create_project(&config.to_project(path.to_str().unwrap()), database.clone())
}
