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

// build.rs — Generates Rust types from Amplitude SDK FlatBuffer schemas (.bfbs files).
//
// Reads binary schema files from $AM_SDK_PATH/schemas/, parses them using the
// flatbuffers reflection API, and emits serde-compatible Rust types to $OUT_DIR.

use flatbuffers_reflection::reflection::{self, BaseType};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::PathBuf;
use std::{env, fs};

// =============================================================================
// Rust reserved words — field names matching these get a `_` suffix + serde rename
// =============================================================================

const RUST_RESERVED: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield",
];

fn is_reserved(name: &str) -> bool {
    RUST_RESERVED.contains(&name)
}

// =============================================================================
// Intermediate representations collected from schemas
// =============================================================================

#[derive(Debug, Clone)]
struct EnumDef {
    name: String,
    variants: Vec<EnumVariant>,
    is_union: bool,
}

#[derive(Debug, Clone)]
struct EnumVariant {
    name: String,
    value: i64,
    rust_type: Option<String>,
}

#[derive(Debug, Clone)]
struct StructDef {
    name: String,
    fields: Vec<FieldDef>,
    #[allow(dead_code)]
    is_struct: bool, // FlatBuffer struct (fixed-size) vs table — kept for future use
}

#[derive(Debug, Clone)]
struct FieldDef {
    name: String,
    rust_type: String,
    is_optional: bool,
    default_value: Option<String>,
    serde_rename: Option<String>,
}

// =============================================================================
// Schema processing
// =============================================================================

/// Extracts the leaf name from a fully qualified FlatBuffer name.
/// e.g., "SparkyStudios.Audio.Amplitude.SoundDefinition" → "SoundDefinition"
fn leaf_name(fqn: &str) -> &str {
    fqn.rsplit('.').next().unwrap_or(fqn)
}

/// Maps a FlatBuffer base type to a Rust type string.
fn base_type_to_rust(base: BaseType) -> Option<&'static str> {
    match base {
        BaseType::Bool => Some("bool"),
        BaseType::Byte => Some("i8"),
        BaseType::UByte => Some("u8"),
        BaseType::Short => Some("i16"),
        BaseType::UShort => Some("u16"),
        BaseType::Int => Some("i32"),
        BaseType::UInt => Some("u32"),
        BaseType::Long => Some("i64"),
        BaseType::ULong => Some("u64"),
        BaseType::Float => Some("f32"),
        BaseType::Double => Some("f64"),
        BaseType::String => Some("String"),
        _ => None,
    }
}

/// Returns a Rust literal for a default value given the base type.
fn format_default(base: BaseType, def_int: i64, def_real: f64) -> Option<String> {
    match base {
        BaseType::Bool => {
            if def_int != 0 {
                Some("true".to_string())
            } else {
                None // false is the default for bool
            }
        }
        BaseType::Byte => {
            if def_int != 0 {
                Some(format!("{}i8", def_int))
            } else {
                None
            }
        }
        BaseType::UByte => {
            if def_int != 0 {
                Some(format!("{}u8", def_int as u8))
            } else {
                None
            }
        }
        BaseType::Short => {
            if def_int != 0 {
                Some(format!("{}i16", def_int))
            } else {
                None
            }
        }
        BaseType::UShort => {
            if def_int != 0 {
                Some(format!("{}u16", def_int as u16))
            } else {
                None
            }
        }
        BaseType::Int => {
            if def_int != 0 {
                Some(format!("{}i32", def_int))
            } else {
                None
            }
        }
        BaseType::UInt => {
            if def_int != 0 {
                Some(format!("{}u32", def_int as u32))
            } else {
                None
            }
        }
        BaseType::Long => {
            if def_int != 0 {
                Some(format!("{}i64", def_int))
            } else {
                None
            }
        }
        BaseType::ULong => {
            if def_int != 0 {
                Some(format!("{}u64", def_int as u64))
            } else {
                None
            }
        }
        BaseType::Float => {
            if def_real != 0.0 {
                Some(format!("{}f32", def_real))
            } else {
                None
            }
        }
        BaseType::Double => {
            if def_real != 0.0 {
                Some(format!("{}f64", def_real))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Processes a single .bfbs schema file, collecting enum and struct definitions
/// into the provided maps. Deduplicates by fully qualified name.
fn process_schema(
    schema_bytes: &[u8],
    enums: &mut BTreeMap<String, EnumDef>,
    structs: &mut BTreeMap<String, StructDef>,
    all_enum_names: &BTreeMap<String, String>,
) -> Result<(), String> {
    let schema = reflection::root_as_schema(schema_bytes)
        .map_err(|e| format!("Failed to parse schema: {}", e))?;

    // Collect objects (tables and structs) first so we can resolve union variants
    let schema_objects = schema.objects();

    // Collect enums
    let schema_enums = schema.enums();
    for i in 0..schema_enums.len() {
        let e = schema_enums.get(i);
        let fqn = e.name().to_string();
        let name = leaf_name(&fqn).to_string();

        if enums.contains_key(&fqn) {
            continue; // Already processed
        }

        let is_union = e.is_union();

        let mut variants = Vec::new();
        let vals = e.values();
        for j in 0..vals.len() {
            let v = vals.get(j);
            let rust_type = if is_union {
                if let Some(union_type) = v.union_type() {
                    let base = union_type.base_type();
                    let idx = union_type.index();
                    if base == BaseType::Obj {
                        let obj_fqn = schema_objects.get(idx as usize).name().to_string();
                        Some(leaf_name(&obj_fqn).to_string())
                    } else if base == BaseType::String {
                        Some("String".to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            variants.push(EnumVariant {
                name: v.name().to_string(),
                value: v.value(),
                rust_type,
            });
        }

        enums.insert(
            fqn,
            EnumDef {
                name,
                variants,
                is_union,
            },
        );
    }

    // Process fields for objects
    for i in 0..schema_objects.len() {
        let obj = schema_objects.get(i);
        let fqn = obj.name().to_string();
        let name = leaf_name(&fqn).to_string();

        if structs.contains_key(&fqn) {
            continue; // Already processed
        }

        let mut fields = Vec::new();
        let obj_fields = obj.fields();
        for j in 0..obj_fields.len() {
            let f = obj_fields.get(j);
            let field_name = f.name().to_string();
            let ty = f.type_();
            let base = ty.base_type();

            // Skip union discriminator fields (UType)
            if base == BaseType::UType {
                continue;
            }

            // Union fields are now processed
            // if base == BaseType::Union {
            //     continue;
            // }

            // Union vector fields are now processed
            // if base == BaseType::Vector
            //     && (ty.element() == BaseType::Union || ty.element() == BaseType::UType)
            // {
            //     continue;
            // }

            // Determine the Rust type for this field
            let Some((rust_type, is_optional, default_value, serde_rename)) =
                resolve_field_type(&f, &schema_objects, &schema_enums, all_enum_names)
            else {
                continue; // Skip fields with unmappable types
            };

            // Handle reserved word field names
            let final_name;
            let final_rename;
            if is_reserved(&field_name) {
                final_name = format!("{}_", field_name);
                final_rename = Some(serde_rename.unwrap_or(field_name.clone()));
            } else {
                final_name = field_name.clone();
                final_rename = serde_rename;
            }

            fields.push(FieldDef {
                name: final_name,
                rust_type,
                is_optional,
                default_value,
                serde_rename: final_rename,
            });
        }

        structs.insert(
            fqn,
            StructDef {
                name,
                fields,
                is_struct: obj.is_struct(),
            },
        );
    }

    Ok(())
}

/// Resolves the Rust type, optionality, default, and serde rename for a field.
/// Returns `None` if the field type cannot be mapped to Rust (unknown base/element type).
fn resolve_field_type(
    field: &reflection::Field,
    objects: &flatbuffers::Vector<flatbuffers::ForwardsUOffset<reflection::Object>>,
    schema_enums: &flatbuffers::Vector<flatbuffers::ForwardsUOffset<reflection::Enum>>,
    all_enum_names: &BTreeMap<String, String>,
) -> Option<(String, bool, Option<String>, Option<String>)> {
    let ty = field.type_();
    let base = ty.base_type();
    let idx = ty.index();
    let is_opt = field.optional();

    // Check if this is an enum-typed scalar field
    if idx >= 0 && is_scalar_base(base) {
        let enum_fqn = schema_enums.get(idx as usize).name().to_string();
        let enum_name = all_enum_names
            .get(&enum_fqn)
            .cloned()
            .unwrap_or_else(|| leaf_name(&enum_fqn).to_string());

        // Check if there's a non-zero default
        let def_int = field.default_integer();
        let default_value = if def_int != 0 {
            // Find the enum variant name for this value
            let e = schema_enums.get(idx as usize);
            let vals = e.values();
            let mut variant_name = None;
            for j in 0..vals.len() {
                let v = vals.get(j);
                if v.value() == def_int {
                    variant_name = Some(v.name().to_string());
                    break;
                }
            }
            if variant_name.is_none() {
                println!(
                    "cargo:warning=Enum {} has no variant for default value {} on field {}",
                    enum_name,
                    def_int,
                    field.name()
                );
            }
            variant_name.map(|vn| format!("{}::{}", enum_name, vn))
        } else {
            None
        };

        return Some((enum_name, is_opt, default_value, None));
    }

    match base {
        BaseType::Union => {
            let enum_fqn = schema_enums.get(idx as usize).name().to_string();
            let enum_name = all_enum_names
                .get(&enum_fqn)
                .cloned()
                .unwrap_or_else(|| leaf_name(&enum_fqn).to_string());
            Some((enum_name, is_opt, None, None))
        }
        BaseType::Obj => {
            let obj_fqn = objects.get(idx as usize).name().to_string();
            let obj_name = leaf_name(&obj_fqn).to_string();
            let is_optional = field.optional();
            Some((obj_name, is_optional, None, None))
        }
        BaseType::Vector => {
            let inner = match ty.element() {
                BaseType::Union => {
                    let enum_fqn = schema_enums.get(idx as usize).name().to_string();
                    leaf_name(&enum_fqn).to_string()
                }
                BaseType::Obj => {
                    let obj_fqn = objects.get(idx as usize).name().to_string();
                    leaf_name(&obj_fqn).to_string()
                }
                BaseType::String => "String".to_string(),
                other => match base_type_to_rust(other) {
                    Some(t) => t.to_string(),
                    None => {
                        println!(
                            "cargo:warning=Skipping field {} — unknown vector element type {:?}",
                            field.name(),
                            other.variant_name()
                        );
                        return None;
                    }
                },
            };
            let is_optional = field.optional();
            Some((format!("Vec<{}>", inner), is_optional, None, None))
        }
        BaseType::String => Some(("String".to_string(), field.optional(), None, None)),
        _ => {
            // Scalar types
            match base_type_to_rust(base) {
                Some(rust_type) => {
                    let default_value =
                        format_default(base, field.default_integer(), field.default_real());
                    Some((rust_type.to_string(), is_opt, default_value, None))
                }
                None => {
                    println!(
                        "cargo:warning=Skipping field {} — unknown base type {:?}",
                        field.name(),
                        base.variant_name()
                    );
                    None
                }
            }
        }
    }
}

fn is_scalar_base(base: BaseType) -> bool {
    matches!(
        base,
        BaseType::Bool
            | BaseType::Byte
            | BaseType::UByte
            | BaseType::Short
            | BaseType::UShort
            | BaseType::Int
            | BaseType::UInt
            | BaseType::Long
            | BaseType::ULong
            | BaseType::Float
            | BaseType::Double
    )
}

// =============================================================================
// Code generation
// =============================================================================

fn generate_code(
    enums: &BTreeMap<String, EnumDef>,
    structs: &BTreeMap<String, StructDef>,
) -> String {
    let mut out = String::new();

    writeln!(
        out,
        "// Auto-generated from SDK FlatBuffer schemas. DO NOT EDIT."
    )
    .unwrap();
    writeln!(
        out,
        "// Generated by build.rs from .bfbs files in $AM_SDK_PATH/schemas/"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "use serde::{{Serialize, Deserialize}};").unwrap();
    writeln!(out).unwrap();

    // Generate enums
    for (fqn, def) in enums {
        if def.is_union {
            writeln!(
                out,
                "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
            )
            .unwrap();
            writeln!(out, "#[serde(untagged)]").unwrap();
            writeln!(out, "pub enum {} {{", def.name).unwrap();
            for variant in &def.variants {
                let rust_name = &variant.name;
                if rust_name == "NONE" {
                    writeln!(out, "    None,").unwrap();
                } else {
                    let ty = variant.rust_type.as_deref().unwrap_or(rust_name);
                    writeln!(out, "    {}({}),", rust_name, ty).unwrap();
                }
            }
            writeln!(out, "}}").unwrap();
            writeln!(out).unwrap();
            continue;
        }

        generate_enum(&mut out, def, fqn);
    }

    // Generate structs
    for (fqn, def) in structs {
        generate_struct(&mut out, def, fqn);
    }

    out
}

fn generate_enum(out: &mut String, def: &EnumDef, _fqn: &str) {
    // All enums use standard serde serialization (strings)
    // but we add a custom deserializer to support both string and integer formats
    if def.is_union {
        writeln!(
            out,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
        )
        .unwrap();
        writeln!(out, "#[serde(untagged)]").unwrap();
    } else {
        writeln!(
            out,
            "#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]"
        )
        .unwrap();
    }

    // Find the variant with value 0 for Default
    let has_zero_variant = def.variants.iter().any(|v| v.value == 0);
    if has_zero_variant {
        writeln!(out, "#[derive(Default)]").unwrap();
    }

    writeln!(out, "pub enum {} {{", def.name).unwrap();

    for variant in &def.variants {
        let rust_name = &variant.name;

        // Add #[default] for value 0
        if variant.value == 0 && has_zero_variant {
            writeln!(out, "    #[default]").unwrap();
        }

        // Add serde rename if the variant name would differ from Rust conventions
        // Special cases: HRTF, RTPC, NONE (for unions) — keep as-is in serde
        let needs_rename = rust_name.chars().all(|c| c.is_uppercase() || c == '_')
            && rust_name.len() > 1
            && rust_name != "NONE";

        if needs_rename {
            writeln!(out, "    #[serde(rename = \"{}\")]", rust_name).unwrap();
        }

        writeln!(out, "    {},", rust_name).unwrap();
    }

    writeln!(out, "}}").unwrap();

    // Add custom Deserialize implementation for non-union enums
    // to support both string and integer representations
    if !def.is_union {
        writeln!(out).unwrap();
        writeln!(out, "impl<'de> serde::Deserialize<'de> for {} {{", def.name).unwrap();
        writeln!(
            out,
            "    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>"
        )
        .unwrap();
        writeln!(out, "    where").unwrap();
        writeln!(out, "        D: serde::Deserializer<'de>,").unwrap();
        writeln!(out, "    {{").unwrap();
        writeln!(out, "        use serde::de::{{self, Visitor}};").unwrap();
        writeln!(out, "        use std::fmt;").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "        struct {}Visitor;", def.name).unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "        impl<'de> Visitor<'de> for {}Visitor {{",
            def.name
        )
        .unwrap();
        writeln!(out, "            type Value = {};", def.name).unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {{"
        )
        .unwrap();
        writeln!(
            out,
            "                write!(formatter, \"string or integer\")"
        )
        .unwrap();
        writeln!(out, "            }}").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>"
        )
        .unwrap();
        writeln!(out, "            where").unwrap();
        writeln!(out, "                E: de::Error,").unwrap();
        writeln!(out, "            {{").unwrap();
        writeln!(out, "                match value {{").unwrap();

        // Generate match arms for string values
        for variant in &def.variants {
            let rust_name = &variant.name;
            let serde_name =
                if rust_name.chars().all(|c| c.is_uppercase() || c == '_') && rust_name.len() > 1 {
                    rust_name.clone() // Keep as-is for HRTF, etc.
                } else {
                    rust_name.clone()
                };
            writeln!(
                out,
                "                    \"{}\" => Ok({}::{}),",
                serde_name, def.name, rust_name
            )
            .unwrap();
        }
        writeln!(
            out,
            "                    _ => Err(de::Error::unknown_variant(value, &[])),"
        )
        .unwrap();
        writeln!(out, "                }}").unwrap();
        writeln!(out, "            }}").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>"
        )
        .unwrap();
        writeln!(out, "            where").unwrap();
        writeln!(out, "                E: de::Error,").unwrap();
        writeln!(out, "            {{").unwrap();
        writeln!(out, "                match value {{").unwrap();

        // Generate match arms for integer values
        for variant in &def.variants {
            let rust_name = &variant.name;
            writeln!(
                out,
                "                    {} => Ok({}::{}),",
                variant.value, def.name, rust_name
            )
            .unwrap();
        }
        writeln!(out, "                    _ => Err(de::Error::invalid_value(de::Unexpected::Unsigned(value), &self)),").unwrap();
        writeln!(out, "                }}").unwrap();
        writeln!(out, "            }}").unwrap();
        writeln!(out, "        }}").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "        deserializer.deserialize_any({}Visitor)",
            def.name
        )
        .unwrap();
        writeln!(out, "    }}").unwrap();
        writeln!(out, "}}").unwrap();
    }

    writeln!(out).unwrap();
}

fn generate_struct(out: &mut String, def: &StructDef, _fqn: &str) {
    writeln!(
        out,
        "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]"
    )
    .unwrap();
    writeln!(out, "pub struct {} {{", def.name).unwrap();

    // Collect default function names we need to generate
    let mut default_fns: Vec<(String, String, String)> = Vec::new(); // (fn_name, type, value)

    for field in &def.fields {
        // serde rename attribute
        if let Some(rename) = &field.serde_rename {
            writeln!(out, "    #[serde(rename = \"{}\")]", rename).unwrap();
        }

        // serde default attribute
        if let Some(default_val) = &field.default_value {
            let fn_name = format!("default_{}_{}", def.name, field.name);
            writeln!(out, "    #[serde(default = \"{}\")]", fn_name).unwrap();
            default_fns.push((fn_name, field.rust_type.clone(), default_val.clone()));
        } else if field.is_optional {
            // Optional fields with no default get serde default (produces None/empty)
        }

        // Add skip_serializing_if for Option fields to produce cleaner JSON
        if field.is_optional {
            writeln!(
                out,
                "    #[serde(skip_serializing_if = \"Option::is_none\")]"
            )
            .unwrap();
        }

        // Field type
        let full_type = if field.is_optional {
            format!("Option<{}>", field.rust_type)
        } else {
            field.rust_type.clone()
        };

        writeln!(out, "    pub {}: {},", field.name, full_type).unwrap();
    }

    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Generate default functions
    for (fn_name, ty, value) in &default_fns {
        writeln!(out, "fn {}() -> {} {{ {} }}", fn_name, ty, value).unwrap();
        writeln!(out).unwrap();
    }
}

// =============================================================================
// First pass: collect all enum names for cross-reference resolution
// =============================================================================

fn collect_enum_names(schema_bytes: &[u8], names: &mut BTreeMap<String, String>) {
    if let Ok(schema) = reflection::root_as_schema(schema_bytes) {
        let schema_enums = schema.enums();
        for i in 0..schema_enums.len() {
            let e = schema_enums.get(i);
            let fqn = e.name().to_string();
            let name = leaf_name(&fqn).to_string();
            // Check for leaf name collision from a different FQN
            if let Some((existing_fqn, _)) = names.iter().find(|(k, v)| *v == &name && *k != &fqn) {
                println!(
                    "cargo:warning=Duplicate leaf name '{}' from FQNs '{}' and '{}' — first wins",
                    name, existing_fqn, fqn
                );
            }
            names.entry(fqn).or_insert(name);
        }
    }
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    // Read AM_SDK_PATH from environment
    let sdk_path = match env::var("AM_SDK_PATH") {
        Ok(path) => path,
        Err(_) => {
            // Emit compile_error! so the build fails with a clear message
            let out_dir = env::var("OUT_DIR").unwrap();
            let out_path = PathBuf::from(&out_dir).join("generated_assets.rs");
            fs::write(
                &out_path,
                r#"compile_error!("
AM_SDK_PATH environment variable is not set.

The Amplitude Audio SDK is required at build time for FlatBuffer schema code generation.

To fix this:
  1. Clone the SDK:  git clone https://github.com/AmplitudeAudio/sdk
  2. Set the environment variable:  export AM_SDK_PATH=/path/to/amplitude/sdk

See the README for detailed setup instructions.
");"#,
            )
            .unwrap();
            println!("cargo:rerun-if-env-changed=AM_SDK_PATH");
            return;
        }
    };

    // Re-run when the env var changes
    println!("cargo:rerun-if-env-changed=AM_SDK_PATH");
    // Re-run when build.rs itself changes
    println!("cargo:rerun-if-changed=build.rs");

    let schemas_dir = PathBuf::from(&sdk_path).join("schemas");
    if !schemas_dir.is_dir() {
        let out_dir = env::var("OUT_DIR").unwrap();
        let out_path = PathBuf::from(&out_dir).join("generated_assets.rs");
        fs::write(
            &out_path,
            format!(
                "compile_error!(\"AM_SDK_PATH schemas directory not found: {}\");",
                schemas_dir.display()
            ),
        )
        .unwrap();
        return;
    }

    // Discover all .bfbs files
    let mut bfbs_files: Vec<PathBuf> = Vec::new();
    let entries = match fs::read_dir(&schemas_dir) {
        Ok(iter) => iter,
        Err(e) => {
            println!("cargo:warning=Failed to read schemas directory: {}", e);
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                println!("cargo:warning=Failed to read directory entry: {}", e);
                continue;
            }
        };
        let path = entry.path();
        if path.extension().map(|e| e == "bfbs").unwrap_or(false) {
            bfbs_files.push(path);
        }
    }

    bfbs_files.sort(); // Deterministic ordering

    if bfbs_files.is_empty() {
        let out_dir = env::var("OUT_DIR").unwrap();
        let out_path = PathBuf::from(&out_dir).join("generated_assets.rs");
        fs::write(
            &out_path,
            format!(
                "compile_error!(\"No .bfbs schema files found in: {}\");",
                schemas_dir.display()
            ),
        )
        .unwrap();
        return;
    }

    // Add rerun-if-changed for each schema file
    for path in &bfbs_files {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    // First pass: collect all enum names across all schemas for cross-reference
    let mut all_enum_names: BTreeMap<String, String> = BTreeMap::new();
    let mut schema_data: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for path in &bfbs_files {
        match fs::read(path) {
            Ok(bytes) => {
                collect_enum_names(&bytes, &mut all_enum_names);
                schema_data.push((path.clone(), bytes));
            }
            Err(e) => {
                println!(
                    "cargo:warning=Failed to read schema file {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }

    // Second pass: process all schemas, collecting types
    let mut enums: BTreeMap<String, EnumDef> = BTreeMap::new();
    let mut structs: BTreeMap<String, StructDef> = BTreeMap::new();

    for (path, bytes) in &schema_data {
        if let Err(e) = process_schema(bytes, &mut enums, &mut structs, &all_enum_names) {
            println!(
                "cargo:warning=Failed to process schema {}: {}",
                path.display(),
                e
            );
        }
    }

    // Generate code
    let code = generate_code(&enums, &structs);

    // Write to $OUT_DIR
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = PathBuf::from(&out_dir).join("generated_assets.rs");
    fs::write(&out_path, &code).unwrap_or_else(|e| {
        panic!(
            "Failed to write generated code to {}: {}",
            out_path.display(),
            e
        );
    });

    // Build visibility message
    let enum_count = enums.values().filter(|e| !e.is_union).count();
    let union_count = enums.values().filter(|e| e.is_union).count();
    let struct_count = structs.len();
    println!(
        "cargo:warning=Generated asset types from {} schema files: {} enums, {} structs ({} unions processed)",
        bfbs_files.len(),
        enum_count,
        struct_count,
        union_count,
    );
}
