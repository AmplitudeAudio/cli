use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::database::db_forget_project;
use crate::database::db_get_project_by_name;
use crate::database::{
    Database, db_create_project, db_get_template_by_name, db_get_templates,
    entities::{ProjectConfiguration, Template},
};
use clap::{Subcommand, value_parser};
use inquire::Confirm;
use inquire::{
    CustomUserError, Select, Text, required,
    validator::{StringValidator, Validation},
};

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
        name: Option<String>,

        #[arg(short, long)]
        template: Option<String>,

        #[arg(long, value_parser = value_parser!(bool))]
        no_register: bool,
    },

    /// Register an existing project
    Register {
        #[arg(value_parser = value_parser!(PathBuf))]
        path: Option<PathBuf>,
    },
}

pub async fn handler(
    command: &ProjectCommands,
    database: Option<Arc<Database>>,
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
                    path: String::new(),
                },
            );

            let mut project_name = name.clone();
            let mut project_template = template.clone();

            if !project_template.is_none()
                && templates
                    .iter()
                    .find(|t| t.name == *project_template.as_ref().unwrap())
                    .is_none()
            {
                Err(anyhow::Error::msg("Invalid project template"))?;
            }

            if project_name.is_none() {
                let ret = Text::new("Project Name")
                    .with_formatter(&transform_name)
                    .with_validator(&validate_name)
                    .with_placeholder("my_project")
                    .prompt()?;

                project_name = Some(ret);
            }

            if project_template.is_none() {
                let ret = Select::new("Project Template", templates).prompt()?;

                project_template = Some(ret.name);
            }

            handle_init_project_command(
                project_name.as_deref().unwrap(),
                project_template.as_deref().unwrap_or(""),
                no_register,
                database,
            )
            .await
        }
        ProjectCommands::Register { path } => {
            handle_register_project_command(path.as_deref().unwrap(), database).await
        }
    }
}

async fn handle_init_project_command(
    name: &str,
    template: &str,
    no_register: &bool,
    database: Option<Arc<Database>>,
) -> anyhow::Result<()> {
    let project_name = transform_name(name);

    if !no_register {
        if let Some(Some(p)) = db_get_project_by_name(project_name.as_str(), database.clone()).ok()
        {
            println!(
                "A project with the name '{}' is already registered.",
                project_name
            );
            println!("  • Project path: {}", p.path);

            if Confirm::new("Do you want to forget this project and create a new one?").prompt()? {
                println!("Unregistering previous project...");
                db_forget_project(p.id.unwrap(), database.clone())?;
            } else {
                return Err(anyhow::Error::msg(
                    "Cannot create project, a project with the same name is already registered. You can use the --no-register flag to just create a project without registering it.",
                ));
            }
        }
    }

    println!("Initializing project '{name}' using template '{template}'...");

    let cwd = env::current_dir()?;
    let project_path = &cwd.join(project_name.clone());

    if project_path.exists() {
        println!(
            "The project path '{}' already exists.",
            project_path.display()
        );

        if Confirm::new(
            "Do you want to overwrite the directory? All existing content will be deleted!",
        )
        .prompt()?
        {
            fs::remove_dir_all(project_path)?;
        } else {
            return Err(anyhow::Error::msg(
                "Cannot create project, The project directory already exist.",
            ));
        }
    }

    fs::create_dir_all(&project_path)?;

    if template != DEFAULT_TEMPLATE {
        if let Some(t) = db_get_template_by_name(template, database.clone())? {
            let template_path = PathBuf::from(t.path);
            if !template_path.exists() {
                eprintln!(
                    "Template directory '{}' does not exist",
                    template_path.display()
                );
                return Err(anyhow::Error::msg("Invalid template path"));
            }

            fs::copy(template_path, &project_path)?;
        } else {
            return Err(anyhow::Error::msg("The selected template was not found"));
        }
    } else {
        // Create project 'sources' directories
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_ATTENUATORS))?;
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_COLLECTIONS))?;
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_EFFECTS))?;
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_EVENTS))?;
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_PIPELINES))?;
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_RTPC))?;
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_SOUND_BANKS))?;
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_SOUNDS))?;
        fs::create_dir_all(
            project_path
                .join("sources")
                .join(PROJECT_DIR_SWITCH_CONTAINERS),
        )?;
        fs::create_dir_all(project_path.join("sources").join(PROJECT_DIR_SWITCHES))?;

        // TODO: Create default config file
        // TODO: Create default buses file
        // TODO: Create default pipeline file

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
            template: template.to_string(),
            default_configuration: "pc.config.amconfig".to_string(),
            build_dir: "build".to_string(),
            data_dir: "data".to_string(),
            sources_dir: "sources".to_string(),
            version: 1,
        };

        if !no_register {
            println!("Registering project '{name}'...");

            db_create_project(
                &project.to_project(project_path.to_str().unwrap()),
                database.clone(),
            )?;
        }

        amproject.write_all(serde_json::to_string(project)?.as_bytes())?;
    }

    println!("Project '{}' created successfully", name);

    Ok(())
}

async fn handle_register_project_command(
    path: &Path,
    database: Option<Arc<Database>>,
) -> anyhow::Result<()> {
    println!("Registering project '{}'...", path.display());

    if !path.join(".amproject").exists() {
        Err(anyhow::Error::msg(
            "Invalid project path. No '.amproject' file detected in the specified path.",
        ))?;
    }

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
