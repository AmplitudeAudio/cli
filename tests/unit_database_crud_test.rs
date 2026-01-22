//! Unit tests for database CRUD operations.

use am::database::{
    Database, db_create_project, db_forget_project, db_get_all_projects,
    db_get_project_by_name, db_get_template_by_name, db_get_templates, entities::Project,
};
use std::sync::Arc;
use tempfile::tempdir;

/// Helper to create a test database with migrations applied.
async fn setup_test_database() -> (Arc<Database>, tempfile::TempDir) {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let mut db = Database::new(&db_path).expect("Failed to create database");
    db.run_migrations().await.expect("Failed to run migrations");
    (Arc::new(db), temp_dir)
}

// =============================================================================
// db_create_project Tests
// =============================================================================

#[tokio::test]
async fn test_p0_db_create_project_inserts_new_project() {
    let (db, _temp_dir) = setup_test_database().await;

    let project = Project {
        id: None,
        name: "test_project".to_string(),
        path: "/path/to/project".to_string(),
        registered_at: None,
    };

    let result = db_create_project(&project, Some(db.clone()));

    assert!(result.is_ok(), "Project creation should succeed");
    assert!(result.unwrap(), "Should return true on success");
}

#[tokio::test]
async fn test_p0_db_create_project_fails_on_duplicate_name() {
    let (db, _temp_dir) = setup_test_database().await;

    let project1 = Project {
        id: None,
        name: "duplicate_project".to_string(),
        path: "/path/one".to_string(),
        registered_at: None,
    };
    db_create_project(&project1, Some(db.clone())).expect("First insert should succeed");

    let project2 = Project {
        id: None,
        name: "duplicate_project".to_string(),
        path: "/path/two".to_string(),
        registered_at: None,
    };

    let result = db_create_project(&project2, Some(db.clone()));

    assert!(result.is_err(), "Duplicate name should fail");
}

#[tokio::test]
async fn test_p1_db_create_project_allows_same_path_different_name() {
    let (db, _temp_dir) = setup_test_database().await;

    let project1 = Project {
        id: None,
        name: "project_a".to_string(),
        path: "/shared/path".to_string(),
        registered_at: None,
    };
    db_create_project(&project1, Some(db.clone())).expect("First insert should succeed");

    let project2 = Project {
        id: None,
        name: "project_b".to_string(),
        path: "/shared/path".to_string(),
        registered_at: None,
    };

    let result = db_create_project(&project2, Some(db.clone()));

    assert!(
        result.is_ok(),
        "Same path with different name should succeed"
    );
}

// =============================================================================
// db_get_project_by_name Tests
// =============================================================================

#[tokio::test]
async fn test_p0_db_get_project_by_name_returns_existing_project() {
    let (db, _temp_dir) = setup_test_database().await;

    let project = Project {
        id: None,
        name: "findable_project".to_string(),
        path: "/path/to/findable".to_string(),
        registered_at: None,
    };
    db_create_project(&project, Some(db.clone())).expect("Insert should succeed");

    let result = db_get_project_by_name("findable_project", Some(db.clone()));

    assert!(result.is_ok(), "Query should succeed");
    let found = result.unwrap();
    assert!(found.is_some(), "Project should be found");

    let found_project = found.unwrap();
    assert_eq!(found_project.name, "findable_project");
    assert_eq!(found_project.path, "/path/to/findable");
    assert!(found_project.id.is_some(), "Project should have an ID");
}

#[tokio::test]
async fn test_p0_db_get_project_by_name_returns_error_for_nonexistent() {
    let (db, _temp_dir) = setup_test_database().await;

    let result = db_get_project_by_name("nonexistent_project", Some(db.clone()));

    assert!(result.is_err(), "Non-existent project should return error");
}

#[tokio::test]
async fn test_p1_db_get_project_by_name_is_case_sensitive() {
    let (db, _temp_dir) = setup_test_database().await;

    let project = Project {
        id: None,
        name: "CaseSensitiveProject".to_string(),
        path: "/path/case".to_string(),
        registered_at: None,
    };
    db_create_project(&project, Some(db.clone())).expect("Insert should succeed");

    let result = db_get_project_by_name("casesensitiveproject", Some(db.clone()));

    assert!(result.is_err(), "Case-different name should not match");
}

// =============================================================================
// db_forget_project Tests
// =============================================================================

#[tokio::test]
async fn test_p0_db_forget_project_removes_project() {
    let (db, _temp_dir) = setup_test_database().await;

    let project = Project {
        id: None,
        name: "to_be_forgotten".to_string(),
        path: "/path/to/forget".to_string(),
        registered_at: None,
    };
    db_create_project(&project, Some(db.clone())).expect("Insert should succeed");

    let found = db_get_project_by_name("to_be_forgotten", Some(db.clone()))
        .expect("Query should succeed")
        .expect("Project should exist");

    let result = db_forget_project(found.id.unwrap(), Some(db.clone()));

    assert!(result.is_ok(), "Forget should succeed");

    let check = db_get_project_by_name("to_be_forgotten", Some(db.clone()));
    assert!(check.is_err(), "Project should no longer exist");
}

#[tokio::test]
async fn test_p1_db_forget_project_with_invalid_id_succeeds() {
    let (db, _temp_dir) = setup_test_database().await;

    let result = db_forget_project(99999, Some(db.clone()));

    assert!(result.is_ok(), "Forget with invalid ID should succeed");
}

// =============================================================================
// db_get_templates Tests
// =============================================================================

#[tokio::test]
async fn test_p1_db_get_templates_returns_empty_for_fresh_db() {
    let (db, _temp_dir) = setup_test_database().await;

    let result = db_get_templates(Some(db.clone()));

    assert!(result.is_ok(), "Query should succeed");
    let templates = result.unwrap();
    assert!(
        templates.is_empty(),
        "Fresh database should have no templates"
    );
}

#[tokio::test]
async fn test_p1_db_get_templates_returns_inserted_templates() {
    let (db, _temp_dir) = setup_test_database().await;

    db.execute(
        "INSERT INTO templates (name, path) VALUES ('template1', '/path/t1')",
        [],
    )
    .expect("Insert should succeed");
    db.execute(
        "INSERT INTO templates (name, path) VALUES ('template2', '/path/t2')",
        [],
    )
    .expect("Insert should succeed");

    let result = db_get_templates(Some(db.clone()));

    assert!(result.is_ok(), "Query should succeed");
    let templates = result.unwrap();
    assert_eq!(templates.len(), 2, "Should have 2 templates");
}

// =============================================================================
// db_get_template_by_name Tests
// =============================================================================

#[tokio::test]
async fn test_p1_db_get_template_by_name_returns_existing_template() {
    let (db, _temp_dir) = setup_test_database().await;

    db.execute(
        "INSERT INTO templates (name, path) VALUES ('my_template', '/templates/my')",
        [],
    )
    .expect("Insert should succeed");

    let result = db_get_template_by_name("my_template", Some(db.clone()));

    assert!(result.is_ok(), "Query should succeed");
    let found = result.unwrap();
    assert!(found.is_some(), "Template should be found");

    let template = found.unwrap();
    assert_eq!(template.name, "my_template");
    assert_eq!(template.path, "/templates/my");
}

#[tokio::test]
async fn test_p1_db_get_template_by_name_returns_error_for_nonexistent() {
    let (db, _temp_dir) = setup_test_database().await;

    let result = db_get_template_by_name("nonexistent", Some(db.clone()));

    assert!(result.is_err(), "Non-existent template should return error");
}

// =============================================================================
// db_get_all_projects Tests
// =============================================================================

#[tokio::test]
async fn test_p0_db_get_all_projects_returns_empty_for_fresh_db() {
    let (db, _temp_dir) = setup_test_database().await;

    let result = db_get_all_projects(Some(db.clone()));

    assert!(result.is_ok(), "Query should succeed");
    let projects = result.unwrap();
    assert!(
        projects.is_empty(),
        "Fresh database should have no projects"
    );
}

#[tokio::test]
async fn test_p0_db_get_all_projects_returns_all_projects() {
    let (db, _temp_dir) = setup_test_database().await;

    let project1 = Project {
        id: None,
        name: "project_one".to_string(),
        path: "/path/one".to_string(),
        registered_at: None,
    };
    let project2 = Project {
        id: None,
        name: "project_two".to_string(),
        path: "/path/two".to_string(),
        registered_at: None,
    };
    db_create_project(&project1, Some(db.clone())).expect("First insert should succeed");
    db_create_project(&project2, Some(db.clone())).expect("Second insert should succeed");

    let result = db_get_all_projects(Some(db.clone()));

    assert!(result.is_ok(), "Query should succeed");
    let projects = result.unwrap();
    assert_eq!(projects.len(), 2, "Should have 2 projects");
}

#[tokio::test]
async fn test_p1_db_get_all_projects_returns_sorted_by_name() {
    let (db, _temp_dir) = setup_test_database().await;

    // Insert projects in non-alphabetical order
    let project_z = Project {
        id: None,
        name: "zebra_project".to_string(),
        path: "/path/zebra".to_string(),
        registered_at: None,
    };
    let project_a = Project {
        id: None,
        name: "alpha_project".to_string(),
        path: "/path/alpha".to_string(),
        registered_at: None,
    };
    let project_m = Project {
        id: None,
        name: "middle_project".to_string(),
        path: "/path/middle".to_string(),
        registered_at: None,
    };
    db_create_project(&project_z, Some(db.clone())).expect("Insert should succeed");
    db_create_project(&project_a, Some(db.clone())).expect("Insert should succeed");
    db_create_project(&project_m, Some(db.clone())).expect("Insert should succeed");

    let result = db_get_all_projects(Some(db.clone()));

    assert!(result.is_ok(), "Query should succeed");
    let projects = result.unwrap();
    assert_eq!(projects.len(), 3, "Should have 3 projects");
    assert_eq!(
        projects[0].name, "alpha_project",
        "First project should be alpha"
    );
    assert_eq!(
        projects[1].name, "middle_project",
        "Second project should be middle"
    );
    assert_eq!(
        projects[2].name, "zebra_project",
        "Third project should be zebra"
    );
}

#[tokio::test]
async fn test_p1_db_get_all_projects_includes_registered_at() {
    let (db, _temp_dir) = setup_test_database().await;

    let project = Project {
        id: None,
        name: "dated_project".to_string(),
        path: "/path/dated".to_string(),
        registered_at: None,
    };
    db_create_project(&project, Some(db.clone())).expect("Insert should succeed");

    let result = db_get_all_projects(Some(db.clone()));

    assert!(result.is_ok(), "Query should succeed");
    let projects = result.unwrap();
    assert_eq!(projects.len(), 1, "Should have 1 project");
    assert!(
        projects[0].registered_at.is_some(),
        "Project should have a registered_at date"
    );
    // The date should be in ISO 8601 format (YYYY-MM-DD)
    let date = projects[0].registered_at.as_ref().unwrap();
    assert!(
        date.len() == 10 && date.contains('-'),
        "Date should be in YYYY-MM-DD format"
    );
}
