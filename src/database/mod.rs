mod connection;
pub mod entities;
mod migrations;

pub use connection::Database;

use crate::database::entities::{Project, ProjectConfiguration, Template};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;

/// Initialize the database system
pub async fn initialize() -> Result<Database> {
    let db_path = get_database_path()?;

    // Ensure the .amplitude directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut database = Database::new(&db_path)?;
    database.run_migrations().await?;

    Ok(database)
}

/// Get the path to the database file. The database file is stored in the user's directory, in
/// an `.amplitude` folder.
pub fn get_database_path() -> Result<PathBuf> {
    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    Ok(home_dir.join(".amplitude").join("am.db"))
}

/// Cleanup function to be called on application exit. Gracefully closes the database.
pub fn cleanup(database: Option<Database>) {
    if let Some(db) = database {
        db.close();
    }
}

/// Cleanup the given database on application panic
pub fn setup_crash_db_cleanup(db: Option<Arc<Database>>) {
    let default_hook = std::panic::take_hook();
    let db_clone = db.clone();
    std::panic::set_hook(Box::new(move |panic_info| {
        eprintln!("Application panicked: {}", panic_info);

        if let Some(db) = &db_clone {
            if let Ok(db) = Arc::try_unwrap(db.clone()) {
                cleanup(Some(db));
            }
        }

        default_hook(panic_info);
    }));
}

/// Get all templates from the database
pub fn db_get_templates(database: Option<Arc<Database>>) -> Result<Vec<entities::Template>> {
    let query = database
        .as_ref()
        .unwrap()
        .prepare("SELECT * FROM templates")?;

    query.query_map([], |row| {
        Ok(Template {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
        })
    })
}

/// Get a template by name from the database. Returns an error if the template is not found.
pub fn db_get_template_by_name(
    name: &str,
    database: Option<Arc<Database>>,
) -> Result<Option<entities::Template>> {
    let query = database
        .as_ref()
        .unwrap()
        .prepare("SELECT * FROM templates WHERE name = $1")?;

    let results = query.query_map([name], |row| {
        Ok(Template {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
        })
    })?;

    results
        .first()
        .ok_or_else(|| anyhow::anyhow!("Could not find template with name {}", name))
        .map(|template| Some(template.clone()))
}

/// Inserts a new project into the database.
pub fn db_create_project(project: &Project, database: Option<Arc<Database>>) -> Result<bool> {
    let query = database
        .as_ref()
        .unwrap()
        .prepare("INSERT INTO projects (name, path, template) VALUES ($1, $2, $3)")?;

    query
        .execute([
            project.name.clone(),
            project.path.clone(),
            project.template.clone(),
        ])
        .map(|_| true)
}

pub fn db_get_project_by_name(
    name: &str,
    database: Option<Arc<Database>>,
) -> Result<Option<entities::Project>> {
    let query = database
        .as_ref()
        .unwrap()
        .prepare("SELECT * FROM projects WHERE name = $1")?;

    let results = query.query_map([name], |row| {
        Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            template: row.get(3)?,
        })
    })?;

    results
        .first()
        .ok_or_else(|| anyhow::anyhow!("Could not find project with name {}", name))
        .map(|template| Some(template.clone()))
}

pub fn db_forget_project(id: i32, database: Option<Arc<Database>>) -> Result<bool> {
    let query = database
        .as_ref()
        .unwrap()
        .prepare("DELETE FROM projects WHERE id = $1")?;

    query.execute([id]).map(|_| true)
}
