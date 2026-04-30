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

//! Runtime schema loading for Amplitude SDK validation.
//!
//! Loads compiled FlatBuffer schema files (`.bfbs`) from the SDK installation
//! and provides a queryable `SchemaRegistry` for validating asset JSON files
//! against the official SDK schemas at runtime.
//!
//! This is complementary to the build-time code generation in `build.rs`:
//! - **Build time**: Generates Rust structs from schemas (compile-time safety)
//! - **Runtime**: Validates user-edited JSON files against schemas (runtime safety)

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::presentation::Output;

use crate::assets::AssetType;
use crate::common::errors::{CliError, codes};
use crate::config::sdk::SdkLocation;

/// A field definition extracted from a FlatBuffer schema.
#[derive(Debug, Clone)]
pub struct SchemaField {
    /// Field name as it appears in JSON.
    pub name: String,
    /// Type description (e.g., "u64", "String", "Vec<String>", "SoundLoopConfig").
    pub type_desc: String,
    /// Whether this field is required (non-optional).
    pub required: bool,
}

/// Schema definition for a single asset type.
#[derive(Debug, Clone)]
pub struct AssetSchema {
    /// The asset type this schema describes.
    pub asset_type: AssetType,
    /// The FlatBuffer table name (e.g., "SoundDefinition").
    pub table_name: String,
    /// Fields defined in the schema.
    pub fields: Vec<SchemaField>,
    /// Source file this schema was loaded from.
    pub source_file: PathBuf,
}

impl AssetSchema {
    /// Returns true if the schema defines a field with the given name.
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f.name == name)
    }

    /// Returns the field definition for the given name, if it exists.
    pub fn get_field(&self, name: &str) -> Option<&SchemaField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Returns all required field names.
    pub fn required_fields(&self) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|f| f.required)
            .map(|f| f.name.as_str())
            .collect()
    }
}

/// Registry of all loaded SDK schemas, queryable by asset type.
///
/// Created by [`load_schemas`] from an SDK installation's schema directory.
pub struct SchemaRegistry {
    /// Schemas indexed by asset type.
    schemas: HashMap<AssetType, AssetSchema>,
    /// Schema files that were successfully loaded.
    loaded_files: Vec<PathBuf>,
    /// Schema files that failed to load (path, error message).
    failed_files: Vec<(PathBuf, String)>,
}

impl SchemaRegistry {
    /// Returns the schema for the given asset type, if loaded.
    pub fn get(&self, asset_type: AssetType) -> Option<&AssetSchema> {
        self.schemas.get(&asset_type)
    }

    /// Returns true if a schema exists for the given asset type.
    pub fn has(&self, asset_type: AssetType) -> bool {
        self.schemas.contains_key(&asset_type)
    }

    /// Returns the number of loaded schemas.
    pub fn schema_count(&self) -> usize {
        self.schemas.len()
    }

    /// Returns the number of successfully loaded schema files.
    pub fn loaded_file_count(&self) -> usize {
        self.loaded_files.len()
    }

    /// Returns details about schema files that failed to load.
    pub fn failed_files(&self) -> &[(PathBuf, String)] {
        &self.failed_files
    }

    /// Returns all loaded asset types.
    pub fn asset_types(&self) -> Vec<AssetType> {
        self.schemas.keys().copied().collect()
    }
}

/// Maps a FlatBuffer table name to an `AssetType`.
///
/// Returns `None` for tables that don't correspond to a top-level asset type
/// (e.g., helper structs like `CurveDefinition`).
fn table_name_to_asset_type(table_name: &str) -> Option<AssetType> {
    match table_name {
        "SoundDefinition" => Some(AssetType::Sound),
        "CollectionDefinition" => Some(AssetType::Collection),
        "SwitchDefinition" => Some(AssetType::Switch),
        "SwitchContainerDefinition" => Some(AssetType::SwitchContainer),
        "SoundBankDefinition" => Some(AssetType::Soundbank),
        "EventDefinition" => Some(AssetType::Event),
        "EffectDefinition" => Some(AssetType::Effect),
        _ => None,
    }
}

/// Extracts the leaf name from a fully qualified FlatBuffer name.
fn leaf_name(fqn: &str) -> &str {
    fqn.rsplit('.').next().unwrap_or(fqn)
}

/// Load all SDK schemas from the given SDK location.
///
/// Scans the schemas directory for `.bfbs` files, parses each one using the
/// FlatBuffer reflection API, and builds a `SchemaRegistry` mapping asset
/// types to their schema definitions.
///
/// Individual schema files that fail to parse are logged as warnings and
/// skipped — the registry will contain schemas for all files that loaded
/// successfully.
///
/// # Errors
///
/// Returns `Err` only for I/O failures that prevent scanning the schemas
/// directory entirely (e.g., permission denied).
pub fn load_schemas(sdk: &SdkLocation, output: &dyn Output) -> Result<SchemaRegistry, CliError> {
    let schemas_dir = sdk.schemas_dir();

    let entries = fs::read_dir(schemas_dir).map_err(|e| {
        CliError::new(
            codes::ERR_SDK_SCHEMA_LOAD_FAILED,
            format!("Cannot read schemas directory: {}", schemas_dir.display()),
            format!("I/O error: {}", e),
        )
        .with_suggestion("Check directory permissions and verify your SDK installation")
    })?;

    let mut schemas: HashMap<AssetType, AssetSchema> = HashMap::new();
    let mut loaded_files: Vec<PathBuf> = Vec::new();
    let mut failed_files: Vec<(PathBuf, String)> = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                output.warning(&format!("Failed to read directory entry in schemas: {}", e));
                continue;
            }
        };

        let path = entry.path();

        // Only process .bfbs files
        if path.extension().is_none_or(|ext| ext != "bfbs") {
            continue;
        }

        match load_single_schema(&path) {
            Ok(asset_schemas) => {
                loaded_files.push(path.clone());
                for schema in asset_schemas {
                    schemas.insert(schema.asset_type, schema);
                }
            }
            Err(e) => {
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                output.warning(&format!("Failed to load schema {}: {}", filename, e));
                failed_files.push((path, e));
            }
        }
    }

    Ok(SchemaRegistry {
        schemas,
        loaded_files,
        failed_files,
    })
}

/// Load and parse a single `.bfbs` schema file.
///
/// Returns asset schemas for each top-level asset type table found in the file.
/// Helper tables (like CurveDefinition) are skipped.
fn load_single_schema(path: &Path) -> Result<Vec<AssetSchema>, String> {
    use flatbuffers_reflection::reflection;

    let bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let schema = reflection::root_as_schema(&bytes)
        .map_err(|e| format!("Failed to parse binary schema: {}", e))?;

    let mut asset_schemas = Vec::new();

    let objects = schema.objects();
    for i in 0..objects.len() {
        let obj = objects.get(i);
        let fqn = obj.name();
        let table_name = leaf_name(fqn).to_string();

        // Only process tables that map to asset types
        let asset_type = match table_name_to_asset_type(&table_name) {
            Some(at) => at,
            None => continue,
        };

        // Extract fields
        let mut fields = Vec::new();
        let obj_fields = obj.fields();
        for j in 0..obj_fields.len() {
            let field = obj_fields.get(j);
            let field_name = field.name().to_string();
            let type_desc = describe_field_type(&field);
            let required = field.required();

            fields.push(SchemaField {
                name: field_name,
                type_desc,
                required,
            });
        }

        asset_schemas.push(AssetSchema {
            asset_type,
            table_name,
            fields,
            source_file: path.to_path_buf(),
        });
    }

    Ok(asset_schemas)
}

/// Produce a human-readable type description for a schema field.
fn describe_field_type(field: &flatbuffers_reflection::reflection::Field) -> String {
    use flatbuffers_reflection::reflection::BaseType;

    let base = field.type_().base_type();
    match base {
        BaseType::Bool => "bool".to_string(),
        BaseType::Byte => "i8".to_string(),
        BaseType::UByte => "u8".to_string(),
        BaseType::Short => "i16".to_string(),
        BaseType::UShort => "u16".to_string(),
        BaseType::Int => "i32".to_string(),
        BaseType::UInt => "u32".to_string(),
        BaseType::Long => "i64".to_string(),
        BaseType::ULong => "u64".to_string(),
        BaseType::Float => "f32".to_string(),
        BaseType::Double => "f64".to_string(),
        BaseType::String => "String".to_string(),
        BaseType::Vector => {
            let element = field.type_().element();
            let inner = match element {
                BaseType::String => "String".to_string(),
                BaseType::Bool => "bool".to_string(),
                BaseType::Int => "i32".to_string(),
                BaseType::UInt => "u32".to_string(),
                BaseType::Long => "i64".to_string(),
                BaseType::ULong => "u64".to_string(),
                BaseType::Float => "f32".to_string(),
                BaseType::Double => "f64".to_string(),
                _ => "unknown".to_string(),
            };
            format!("Vec<{}>", inner)
        }
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::JsonOutput;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_table_name_to_asset_type() {
        assert_eq!(
            table_name_to_asset_type("SoundDefinition"),
            Some(AssetType::Sound)
        );
        assert_eq!(
            table_name_to_asset_type("CollectionDefinition"),
            Some(AssetType::Collection)
        );
        assert_eq!(
            table_name_to_asset_type("SwitchDefinition"),
            Some(AssetType::Switch)
        );
        assert_eq!(
            table_name_to_asset_type("SwitchContainerDefinition"),
            Some(AssetType::SwitchContainer)
        );
        assert_eq!(
            table_name_to_asset_type("SoundBankDefinition"),
            Some(AssetType::Soundbank)
        );
        assert_eq!(
            table_name_to_asset_type("EventDefinition"),
            Some(AssetType::Event)
        );
        assert_eq!(
            table_name_to_asset_type("EffectDefinition"),
            Some(AssetType::Effect)
        );
        assert_eq!(table_name_to_asset_type("CurveDefinition"), None);
        assert_eq!(table_name_to_asset_type("UnknownType"), None);
    }

    #[test]
    fn test_leaf_name() {
        assert_eq!(
            leaf_name("SparkyStudios.Audio.Amplitude.SoundDefinition"),
            "SoundDefinition"
        );
        assert_eq!(leaf_name("SoundDefinition"), "SoundDefinition");
        assert_eq!(leaf_name(""), "");
    }

    #[test]
    fn test_schema_field_accessors() {
        let schema = AssetSchema {
            asset_type: AssetType::Sound,
            table_name: "SoundDefinition".to_string(),
            fields: vec![
                SchemaField {
                    name: "id".to_string(),
                    type_desc: "u64".to_string(),
                    required: true,
                },
                SchemaField {
                    name: "name".to_string(),
                    type_desc: "String".to_string(),
                    required: false,
                },
                SchemaField {
                    name: "path".to_string(),
                    type_desc: "String".to_string(),
                    required: true,
                },
            ],
            source_file: PathBuf::from("test.bfbs"),
        };

        assert!(schema.has_field("id"));
        assert!(schema.has_field("name"));
        assert!(!schema.has_field("nonexistent"));

        let id_field = schema.get_field("id").unwrap();
        assert_eq!(id_field.type_desc, "u64");
        assert!(id_field.required);

        let required = schema.required_fields();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&"id"));
        assert!(required.contains(&"path"));
    }

    #[test]
    fn test_schema_registry_empty() {
        let registry = SchemaRegistry {
            schemas: HashMap::new(),
            loaded_files: vec![],
            failed_files: vec![],
        };

        assert_eq!(registry.schema_count(), 0);
        assert!(!registry.has(AssetType::Sound));
        assert!(registry.get(AssetType::Sound).is_none());
        assert!(registry.asset_types().is_empty());
    }

    #[test]
    fn test_schema_registry_with_schema() {
        let mut schemas = HashMap::new();
        schemas.insert(
            AssetType::Sound,
            AssetSchema {
                asset_type: AssetType::Sound,
                table_name: "SoundDefinition".to_string(),
                fields: vec![SchemaField {
                    name: "id".to_string(),
                    type_desc: "u64".to_string(),
                    required: true,
                }],
                source_file: PathBuf::from("sound.bfbs"),
            },
        );

        let registry = SchemaRegistry {
            schemas,
            loaded_files: vec![PathBuf::from("sound.bfbs")],
            failed_files: vec![],
        };

        assert_eq!(registry.schema_count(), 1);
        assert!(registry.has(AssetType::Sound));
        assert!(!registry.has(AssetType::Event));
        assert_eq!(registry.loaded_file_count(), 1);
        assert!(registry.failed_files().is_empty());

        let sound_schema = registry.get(AssetType::Sound).unwrap();
        assert_eq!(sound_schema.table_name, "SoundDefinition");
        assert!(sound_schema.has_field("id"));
    }

    #[test]
    fn test_load_schemas_empty_directory() {
        let dir = tempdir().unwrap();
        let schemas_dir = dir.path().join("schemas");
        fs::create_dir_all(&schemas_dir).unwrap();

        let sdk = SdkLocation::new_for_test(dir.path().to_path_buf());
        let registry = load_schemas(&sdk, &JsonOutput::new()).unwrap();

        assert_eq!(registry.schema_count(), 0);
        assert_eq!(registry.loaded_file_count(), 0);
    }

    #[test]
    fn test_load_schemas_invalid_bfbs_file() {
        let dir = tempdir().unwrap();
        let schemas_dir = dir.path().join("schemas");
        fs::create_dir_all(&schemas_dir).unwrap();

        // Write an invalid .bfbs file
        fs::write(schemas_dir.join("broken.bfbs"), b"not a valid schema").unwrap();

        let sdk = SdkLocation::new_for_test(dir.path().to_path_buf());
        let registry = load_schemas(&sdk, &JsonOutput::new()).unwrap();

        assert_eq!(registry.schema_count(), 0);
        assert_eq!(registry.failed_files().len(), 1);
    }

    #[test]
    fn test_load_schemas_skips_non_bfbs_files() {
        let dir = tempdir().unwrap();
        let schemas_dir = dir.path().join("schemas");
        fs::create_dir_all(&schemas_dir).unwrap();

        // Write a non-.bfbs file (should be ignored)
        fs::write(schemas_dir.join("readme.txt"), "not a schema").unwrap();
        fs::write(schemas_dir.join("data.json"), "{}").unwrap();

        let sdk = SdkLocation::new_for_test(dir.path().to_path_buf());
        let registry = load_schemas(&sdk, &JsonOutput::new()).unwrap();

        assert_eq!(registry.schema_count(), 0);
        assert_eq!(registry.loaded_file_count(), 0);
        // Non-.bfbs files should NOT appear in failed_files
        assert!(registry.failed_files().is_empty());
    }

    #[test]
    fn test_load_schemas_missing_directory() {
        let dir = tempdir().unwrap();
        // Don't create schemas dir
        let sdk = SdkLocation::new_for_test(dir.path().to_path_buf());

        // This should fail because the schemas directory doesn't exist
        let result = load_schemas(&sdk, &JsonOutput::new());
        assert!(result.is_err());
    }
}
