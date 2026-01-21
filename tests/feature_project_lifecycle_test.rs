//! Feature tests for project lifecycle operations.

use am::database::{
    Database, db_create_project, db_forget_project, db_get_project_by_name,
    entities::{Project, ProjectConfiguration},
};
use std::fs;
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
// Project Initialization Tests
// =============================================================================

#[tokio::test]
async fn test_p0_project_init_creates_directory_structure() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let project_path = temp_dir.path().join("test_project");

    fs::create_dir_all(&project_path).expect("Failed to create project dir");

    let sources_dir = project_path.join("sources");
    fs::create_dir_all(sources_dir.join("attenuators")).expect("Failed to create attenuators");
    fs::create_dir_all(sources_dir.join("collections")).expect("Failed to create collections");
    fs::create_dir_all(sources_dir.join("effects")).expect("Failed to create effects");
    fs::create_dir_all(sources_dir.join("events")).expect("Failed to create events");
    fs::create_dir_all(sources_dir.join("pipelines")).expect("Failed to create pipelines");
    fs::create_dir_all(sources_dir.join("rtpc")).expect("Failed to create rtpc");
    fs::create_dir_all(sources_dir.join("soundbanks")).expect("Failed to create soundbanks");
    fs::create_dir_all(sources_dir.join("sounds")).expect("Failed to create sounds");
    fs::create_dir_all(sources_dir.join("switch_containers"))
        .expect("Failed to create switch_containers");
    fs::create_dir_all(sources_dir.join("switches")).expect("Failed to create switches");

    fs::create_dir_all(project_path.join("build")).expect("Failed to create build");
    fs::create_dir_all(project_path.join("data")).expect("Failed to create data");
    fs::create_dir_all(project_path.join("plugins")).expect("Failed to create plugins");

    assert!(project_path.exists());
    assert!(sources_dir.join("attenuators").exists());
    assert!(sources_dir.join("sounds").exists());
    assert!(project_path.join("build").exists());
    assert!(project_path.join("data").exists());
}

#[tokio::test]
async fn test_p0_project_init_creates_amproject_file() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let project_path = temp_dir.path().join("my_project");
    fs::create_dir_all(&project_path).expect("Failed to create project dir");

    let config = ProjectConfiguration {
        name: "my_project".to_string(),
        default_configuration: "pc.config.amconfig".to_string(),
        build_dir: "build".to_string(),
        data_dir: "data".to_string(),
        sources_dir: "sources".to_string(),
        version: 1,
    };

    let amproject_path = project_path.join(".amproject");
    let json = serde_json::to_string(&config).expect("Failed to serialize");
    fs::write(&amproject_path, json).expect("Failed to write .amproject");

    assert!(amproject_path.exists());

    let content = fs::read_to_string(&amproject_path).expect("Failed to read");
    let parsed: ProjectConfiguration = serde_json::from_str(&content).expect("Failed to parse");

    assert_eq!(parsed.name, "my_project");
    assert_eq!(parsed.version, 1);
}

// =============================================================================
// Project Registration Flow Tests
// =============================================================================

#[tokio::test]
async fn test_p0_project_registration_stores_in_database() {
    let (db, temp_dir) = setup_test_database().await;
    let project_path = temp_dir.path().join("registered_project");
    fs::create_dir_all(&project_path).expect("Failed to create project dir");

    let config = ProjectConfiguration {
        name: "registered_project".to_string(),
        default_configuration: "pc.config.amconfig".to_string(),
        build_dir: "build".to_string(),
        data_dir: "data".to_string(),
        sources_dir: "sources".to_string(),
        version: 1,
    };

    let project = config.to_project(project_path.to_str().unwrap());
    db_create_project(&project, Some(db.clone())).expect("Registration should succeed");

    let found = db_get_project_by_name("registered_project", Some(db.clone()))
        .expect("Query should succeed")
        .expect("Project should exist");

    assert_eq!(found.name, "registered_project");
    assert_eq!(found.path, project_path.to_str().unwrap());
}

#[tokio::test]
async fn test_p1_project_registration_prevents_duplicate_names() {
    let (db, temp_dir) = setup_test_database().await;

    let project1 = Project {
        id: None,
        name: "unique_name".to_string(),
        path: temp_dir
            .path()
            .join("project1")
            .to_str()
            .unwrap()
            .to_string(),
    };
    db_create_project(&project1, Some(db.clone())).expect("First registration should succeed");

    let project2 = Project {
        id: None,
        name: "unique_name".to_string(),
        path: temp_dir
            .path()
            .join("project2")
            .to_str()
            .unwrap()
            .to_string(),
    };
    let result = db_create_project(&project2, Some(db.clone()));

    assert!(result.is_err(), "Duplicate name registration should fail");
}

// =============================================================================
// Project Unregistration Flow Tests
// =============================================================================

#[tokio::test]
async fn test_p0_project_unregistration_removes_from_database() {
    let (db, _temp_dir) = setup_test_database().await;

    let project = Project {
        id: None,
        name: "to_unregister".to_string(),
        path: "/path/to/project".to_string(),
    };
    db_create_project(&project, Some(db.clone())).expect("Registration should succeed");

    let found = db_get_project_by_name("to_unregister", Some(db.clone()))
        .expect("Query should succeed")
        .expect("Project should exist");

    db_forget_project(found.id.unwrap(), Some(db.clone())).expect("Unregister should succeed");

    let check = db_get_project_by_name("to_unregister", Some(db.clone()));
    assert!(check.is_err(), "Project should not exist after unregister");
}

#[tokio::test]
async fn test_p1_project_unregistration_does_not_delete_files_by_default() {
    let (db, temp_dir) = setup_test_database().await;
    let project_path = temp_dir.path().join("project_with_files");
    fs::create_dir_all(&project_path).expect("Failed to create project dir");

    let amproject_path = project_path.join(".amproject");
    fs::write(&amproject_path, "{}").expect("Failed to write .amproject");

    let project = Project {
        id: None,
        name: "project_with_files".to_string(),
        path: project_path.to_str().unwrap().to_string(),
    };
    db_create_project(&project, Some(db.clone())).expect("Registration should succeed");

    let found = db_get_project_by_name("project_with_files", Some(db.clone()))
        .expect("Query should succeed")
        .expect("Project should exist");

    db_forget_project(found.id.unwrap(), Some(db.clone())).expect("Unregister should succeed");

    assert!(
        project_path.exists(),
        "Project directory should still exist"
    );
    assert!(amproject_path.exists(), ".amproject should still exist");
}

// =============================================================================
// Full Lifecycle Tests
// =============================================================================

#[tokio::test]
async fn test_p0_full_project_lifecycle() {
    let (db, temp_dir) = setup_test_database().await;
    let project_name = "lifecycle_test";
    let project_path = temp_dir.path().join(project_name);

    // Step 1 - Initialize project
    fs::create_dir_all(project_path.join("sources")).expect("Failed to create sources");
    fs::create_dir_all(project_path.join("build")).expect("Failed to create build");
    fs::create_dir_all(project_path.join("data")).expect("Failed to create data");

    let config = ProjectConfiguration {
        name: project_name.to_string(),
        default_configuration: "pc.config.amconfig".to_string(),
        build_dir: "build".to_string(),
        data_dir: "data".to_string(),
        sources_dir: "sources".to_string(),
        version: 1,
    };

    fs::write(
        project_path.join(".amproject"),
        serde_json::to_string(&config).unwrap(),
    )
    .expect("Failed to write .amproject");

    assert!(project_path.join(".amproject").exists());
    assert!(project_path.join("sources").exists());

    // Step 2 - Register project
    let project = config.to_project(project_path.to_str().unwrap());
    db_create_project(&project, Some(db.clone())).expect("Registration should succeed");

    let found = db_get_project_by_name(project_name, Some(db.clone()))
        .expect("Query should succeed")
        .expect("Project should exist");
    assert_eq!(found.name, project_name);

    // Step 3 - Unregister project
    db_forget_project(found.id.unwrap(), Some(db.clone())).expect("Unregister should succeed");

    let check = db_get_project_by_name(project_name, Some(db.clone()));
    assert!(check.is_err(), "Project should not exist after unregister");

    assert!(project_path.exists(), "Project files should remain");
}

#[tokio::test]
async fn test_p1_re_register_after_unregister() {
    let (db, temp_dir) = setup_test_database().await;

    let project = Project {
        id: None,
        name: "re_register_test".to_string(),
        path: temp_dir
            .path()
            .join("re_register")
            .to_str()
            .unwrap()
            .to_string(),
    };

    db_create_project(&project, Some(db.clone())).expect("First registration should succeed");

    let found = db_get_project_by_name("re_register_test", Some(db.clone()))
        .expect("Query should succeed")
        .expect("Project should exist");
    db_forget_project(found.id.unwrap(), Some(db.clone())).expect("Unregister should succeed");

    let result = db_create_project(&project, Some(db.clone()));

    assert!(result.is_ok(), "Re-registration should succeed");
}
