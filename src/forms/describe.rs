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

//! Builds [`FieldSpec`]s for an asset type from the loaded SDK schemas.

use crate::assets::AssetType;
use crate::common::errors::{CliError, codes};
use crate::schema::loader::{SchemaField, SchemaRegistry};

use super::field::{FieldKind, FieldSpec};
use super::field_docs::doc_for;
use super::references::rule_for;

/// Maps a schema scalar/table type description to a [`FieldKind`].
fn element_kind(asset_type: AssetType, field: &SchemaField, desc: &str) -> FieldKind {
    // Enum-typed scalars and unions carry their variants.
    if let Some(variants) = &field.enum_variants {
        return FieldKind::Enum(variants.clone());
    }
    match desc {
        "bool" => FieldKind::Bool,
        "String" => FieldKind::Text,
        "f32" | "f64" => FieldKind::Float,
        "RtpcCompatibleValue" => FieldKind::RtpcValue,
        "u64" => match rule_for(asset_type, &field.name) {
            Some(rule) => FieldKind::Reference(rule.target),
            None => FieldKind::Int,
        },
        "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" => FieldKind::Int,
        other => FieldKind::Nested(other.to_string()),
    }
}

fn kind_of(asset_type: AssetType, field: &SchemaField) -> FieldKind {
    match field
        .type_desc
        .strip_prefix("Vec<")
        .and_then(|s| s.strip_suffix('>'))
    {
        Some(inner) => FieldKind::List(Box::new(element_kind(asset_type, field, inner))),
        None => element_kind(asset_type, field, &field.type_desc),
    }
}

/// True for the hidden discriminator field(s) FlatBuffers adds for unions.
fn is_union_discriminator(field: &SchemaField) -> bool {
    matches!(field.type_desc.as_str(), "union_type" | "Vec<union_type>")
}

/// Describes every user-facing top-level field of `asset_type`.
///
/// Union discriminator fields (e.g. `sounds_type`) are omitted.
pub fn describe(
    registry: &SchemaRegistry,
    asset_type: AssetType,
) -> Result<Vec<FieldSpec>, CliError> {
    let schema = registry.get(asset_type).ok_or_else(|| {
        CliError::new(
            codes::ERR_SDK_SCHEMA_LOAD_FAILED,
            format!("No schema loaded for asset type {:?}", asset_type),
            "The SDK schema for this asset type was not found",
        )
        .with_suggestion("Check your SDK installation with 'am sdk status'")
    })?;

    Ok(schema
        .fields
        .iter()
        .filter(|f| !is_union_discriminator(f))
        .map(|f| FieldSpec {
            name: f.name.clone(),
            kind: kind_of(asset_type, f),
            doc: doc_for(asset_type, &f.name).map(str::to_string),
            default: f.default.clone(),
            required: f.required,
        })
        .collect())
}
