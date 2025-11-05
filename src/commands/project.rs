use clap::Subcommand;
use promkit::{Prompt, preset::readline::Readline, suggest::Suggest};
use tokio::io::stdout;

#[derive(Subcommand)]
pub enum ProjectCommands {
    /// Create a new project
    Init {
        #[arg(value_parser = validate_name)]
        name: Option<String>,

        #[arg(short, long)]
        template: Option<String>,
    },
}

pub async fn handler(command: &ProjectCommands) -> anyhow::Result<()> {
    match command {
        ProjectCommands::Init { name, template } => {
            let mut project_name = name.clone();

            if let None = project_name {
                let ret = Readline::default()
                    .title("Enter the project name")
                    .prefix("? ")
                    .validator(
                        |text| !text.chars().any(|c| !c.is_alphanumeric() && c != '_' && c != '-'),
                        |_| format!("The project name must only contain alphanumeric characters, underscores, and hyphens."),
                    )
                    .run()
                    .await?;

                project_name = Some(transform_name(ret.as_ref()));
            }

            return handle_init_project_command(
                project_name.as_deref().unwrap(),
                template.as_deref().unwrap_or("empty"),
            )
            .await;
        }
    }
}

async fn handle_init_project_command(name: &str, template: &str) -> anyhow::Result<()> {
    println!("Initializing project: {name}");
    Err(anyhow::Error::msg("Failed to create project"))
}

fn validate_name(name: &str) -> Result<String, String> {
    if name
        .chars()
        .any(|c| !c.is_alphanumeric() && c != '_' && c != '-')
    {
        Err(format!(
            "The project name must only contain alphanumeric characters, underscores, and hyphens."
        ))
    } else {
        Ok(transform_name(name))
    }
}

fn transform_name(name: &str) -> String {
    name.to_lowercase().replace(' ', "_").replace('-', "_")
}
