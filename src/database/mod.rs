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

mod connection;
pub mod entities;
mod migrations;

pub use connection::Database;

use crate::database::entities::{Project, Template};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;

/// Error message for when database is required but not available.
const ERR_DATABASE_NOT_AVAILABLE: &str =
    "Database is not available. This operation requires a database connection.";

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
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let query =
        db.prepare("SELECT id, name, path, engine, description FROM templates ORDER BY name ASC")?;

    query.query_map([], |row| {
        Ok(Template {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            engine: row.get(3)?,
            description: row.get(4)?,
            source: entities::TemplateSource::Custom,
        })
    })
}

/// Get a template by name from the database. Returns `Ok(None)` if the template is not found.
pub fn db_get_template_by_name(
    name: &str,
    database: Option<Arc<Database>>,
) -> Result<Option<entities::Template>> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let query =
        db.prepare("SELECT id, name, path, engine, description FROM templates WHERE name = $1")?;

    let results = query.query_map([name], |row| {
        Ok(Template {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            engine: row.get(3)?,
            description: row.get(4)?,
            source: entities::TemplateSource::Custom,
        })
    })?;

    Ok(results.first().cloned())
}

/// Inserts a new project into the database.
pub fn db_create_project(project: &Project, database: Option<Arc<Database>>) -> Result<bool> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let query = db.prepare("INSERT INTO projects (name, path) VALUES ($1, $2)")?;

    query
        .execute([project.name.clone(), project.path.clone()])
        .map(|_| true)
}

/// Get a project by name from the database.
pub fn db_get_project_by_name(
    name: &str,
    database: Option<Arc<Database>>,
) -> Result<Option<entities::Project>> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let query = db.prepare(
        "SELECT id, name, path, date(created_at) as registered_at, is_favorite FROM projects WHERE name = $1",
    )?;

    let results = query.query_map([name], |row| {
        Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            registered_at: row.get(3)?,
            is_favorite: row.get::<_, i32>(4)? != 0,
        })
    })?;

    Ok(results.first().cloned())
}

/// Get all registered projects from the database. Favorites are pinned to the
/// top, then sorted alphabetically by name within each group.
pub fn db_get_all_projects(database: Option<Arc<Database>>) -> Result<Vec<entities::Project>> {
    db_get_projects_filtered(None, database)
}

/// Get registered projects, optionally filtered by favorite status.
///
/// `favorite_only`:
/// - `None` returns every project.
/// - `Some(true)` returns only favorites.
/// - `Some(false)` returns only non-favorites.
///
/// Ordering is always favorites-first, then alphabetical by name.
pub fn db_get_projects_filtered(
    favorite_only: Option<bool>,
    database: Option<Arc<Database>>,
) -> Result<Vec<entities::Project>> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let (where_clause, order_clause) = match favorite_only {
        None => ("", "is_favorite DESC, name ASC"),
        Some(true) => ("WHERE is_favorite = 1", "name ASC"),
        Some(false) => ("WHERE is_favorite = 0", "name ASC"),
    };

    let sql = format!(
        "SELECT id, name, path, date(created_at) as registered_at, is_favorite \
         FROM projects {where_clause} ORDER BY {order_clause}"
    );

    let query = db.prepare(&sql)?;

    query.query_map([], |row| {
        Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            registered_at: row.get(3)?,
            is_favorite: row.get::<_, i32>(4)? != 0,
        })
    })
}

pub fn db_forget_project(id: i32, database: Option<Arc<Database>>) -> Result<bool> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let query = db.prepare("DELETE FROM projects WHERE id = $1")?;

    query.execute([id]).map(|_| true)
}

/// Update the `is_favorite` flag for a project. Returns whether a row was affected.
pub fn db_set_project_favorite(
    id: i32,
    value: bool,
    database: Option<Arc<Database>>,
) -> Result<bool> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let query = db.prepare("UPDATE projects SET is_favorite = ?1 WHERE id = ?2")?;

    let rows = query.execute(rusqlite::params![if value { 1 } else { 0 }, id])?;
    Ok(rows > 0)
}

/// Get a project by its filesystem path from the database.
pub fn db_get_project_by_path(
    path: &str,
    database: Option<Arc<Database>>,
) -> Result<Option<entities::Project>> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let query = db.prepare(
        "SELECT id, name, path, date(created_at) as registered_at, is_favorite FROM projects WHERE path = $1",
    )?;

    let results = query.query_map([path], |row| {
        Ok(Project {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            registered_at: row.get(3)?,
            is_favorite: row.get::<_, i32>(4)? != 0,
        })
    })?;

    Ok(results.first().cloned())
}

/// Inserts a new template into the database.
///
/// # Arguments
/// * `template` - The template to insert
/// * `database` - Database connection
///
/// # Returns
/// * `Ok(true)` - Template was inserted successfully
/// * `Err` - Database error occurred
pub fn db_create_template(template: &Template, database: Option<Arc<Database>>) -> Result<bool> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let conn = db.get_connection();
    let conn = conn
        .lock()
        .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

    conn.execute(
        "INSERT INTO templates (name, path, engine, description) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            template.name,
            template.path,
            template.engine,
            template.description,
        ],
    )?;

    Ok(true)
}

/// Delete a template by name from the database.
///
/// # Arguments
/// * `name` - Name of the template to delete
/// * `database` - Database connection
///
/// # Returns
/// * `Ok(true)` - Template was deleted (row existed)
/// * `Ok(false)` - No template with that name existed
/// * `Err` - Database error occurred
pub fn db_delete_template_by_name(name: &str, database: Option<Arc<Database>>) -> Result<bool> {
    let db = database.as_ref().context(ERR_DATABASE_NOT_AVAILABLE)?;

    let conn = db.get_connection();
    let conn = conn
        .lock()
        .map_err(|e| anyhow::anyhow!("Failed to acquire database lock: {}", e))?;

    let rows_affected = conn.execute("DELETE FROM templates WHERE name = ?1", [name])?;

    Ok(rows_affected > 0)
}
