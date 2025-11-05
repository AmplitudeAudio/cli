mod app;
mod commands;

use clap::Parser;

use crate::{
    app::{App, Commands},
    commands::project::handler as handle_project_command,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = App::parse();

    match &cli.command {
        Commands::Project { command } => {
            if let Err(result) = handle_project_command(command).await {
                Err(result)
            } else {
                println!("Project created successfully!");
                Ok(())
            }
        }
    }
}
