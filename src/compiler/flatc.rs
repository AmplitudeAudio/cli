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

//! In-process FlatBuffers compiler.
//!
//! Converts JSON data to FlatBuffers binary format using reflection-based
//! schema parsing. This replaces the external `flatc` tool invocation with
//! a pure Rust implementation that reads `.bfbs` schema files at runtime.

use std::cell::Cell;

use anyhow::{Context, Result, bail};
use flatbuffers::{FlatBufferBuilder, PushAlignment, UOffsetT, VOffsetT, WIPOffset};
use flatbuffers_reflection::reflection::{
    BaseType, Enum, Field, Object, Schema, root_as_schema,
};
use serde_json::Value;

// Thread-local storage for passing dynamic struct size/alignment to the Push impl.
// This is needed because Push::size() and Push::alignment() are static methods
// that cannot access instance data, but we need runtime-determined values for
// reflection-based struct construction.
thread_local! {
    static DYN_STRUCT_SIZE: Cell<usize> = const { Cell::new(0) };
    static DYN_STRUCT_ALIGN: Cell<usize> = const { Cell::new(1) };
}

/// Compile a JSON string into FlatBuffers binary format using the given schema.
///
/// # Arguments
///
/// * `schema_bytes` - Raw bytes of a `.bfbs` (compiled FlatBuffers schema) file
/// * `json_str` - JSON string conforming to the schema's root table
///
/// # Returns
///
/// The serialized FlatBuffers binary as a `Vec<u8>`.
pub fn compile_json_to_binary(
    schema_bytes: &[u8],
    json_str: &str,
    output: &dyn crate::presentation::Output,
) -> Result<Vec<u8>> {
    let schema = root_as_schema(schema_bytes)
        .context("failed to parse .bfbs schema")?;

    let root_table = schema
        .root_table()
        .context("schema has no root table defined")?;

    let json: Value = serde_json::from_str(json_str)
        .context("failed to parse JSON input")?;

    let json_obj = json
        .as_object()
        .context("JSON root must be an object")?;

    let mut builder = FlatBufferBuilder::with_capacity(1024);

    let root_offset = build_table(&mut builder, &schema, &root_table, json_obj, output)?;

    builder.finish(root_offset, schema.file_ident());

    Ok(builder.finished_data().to_vec())
}

/// A stored pre-built offset (string, table, vector, or union value).
///
/// FlatBuffers requires all variable-length data to be written before
/// the table that references it. We store the raw `u32` offset value
/// and reconstruct a typed `WIPOffset` when pushing into the table.
#[derive(Debug, Clone, Copy)]
struct PrebuiltOffset(UOffsetT);

impl PrebuiltOffset {
    fn from_wip<T>(w: WIPOffset<T>) -> Self {
        Self(w.value())
    }

    fn as_union_wip(self) -> WIPOffset<flatbuffers::UnionWIPOffset> {
        WIPOffset::new(self.0)
    }
}

/// Dynamically constructed struct data for use with `FlatBufferBuilder`.
///
/// FlatBuffers structs are fixed-size inline data. Generated code uses
/// concrete types that implement `Push`, but for reflection-based compilation
/// we build the raw bytes manually and use thread-local storage to pass
/// size/alignment info to the `Push` trait's static methods.
struct DynStruct {
    /// Raw struct bytes in little-endian layout, sized to `bytesize`.
    data: Vec<u8>,
    /// Minimum alignment required by this struct.
    align: usize,
}

impl DynStruct {
    /// Push this struct onto the builder, setting up thread-local size/alignment
    /// first so the Push trait implementation picks up the correct values.
    fn push_to_builder<'a>(
        self,
        builder: &mut FlatBufferBuilder<'a>,
        slot: VOffsetT,
    ) {
        DYN_STRUCT_SIZE.set(self.data.len());
        DYN_STRUCT_ALIGN.set(self.align);
        builder.push_slot_always(slot, self);
    }
}

// Safety: DynStruct writes pre-computed raw bytes into the builder buffer.
// The bytes are laid out according to the schema's field offsets and sizes.
// Size and alignment are communicated via thread-local storage set by
// push_to_builder() immediately before the builder calls Push methods.
impl flatbuffers::Push for DynStruct {
    type Output = DynStruct;

    #[inline]
    unsafe fn push(&self, dst: &mut [u8], _written_len: usize) {
        dst[..self.data.len()].copy_from_slice(&self.data);
    }

    #[inline]
    fn size() -> usize {
        DYN_STRUCT_SIZE.get()
    }

    #[inline]
    fn alignment() -> PushAlignment {
        PushAlignment::new(DYN_STRUCT_ALIGN.get())
    }
}

/// Build a struct as raw bytes according to its schema definition.
///
/// Struct fields are laid out in memory at their `offset()` positions.
/// The returned `DynStruct` can be pushed onto a `FlatBufferBuilder`.
fn build_struct_bytes(
    schema: &Schema<'_>,
    object: &Object<'_>,
    json_obj: &serde_json::Map<String, Value>,
) -> Result<DynStruct> {
    let size = object.bytesize() as usize;
    let align = object.minalign() as usize;
    let mut data = vec![0u8; size];

    let fields = object.fields();
    for i in 0..fields.len() {
        let field = fields.get(i);
        let field_name = field.name();
        let offset = field.offset() as usize;
        let base_type = field.type_().base_type();

        let json_val = match json_obj.get(field_name) {
            Some(v) => v,
            None => continue, // Use zero-initialized default.
        };

        write_scalar_to_buf(&mut data, offset, base_type, schema, &field, json_val)?;
    }

    Ok(DynStruct { data, align })
}

/// Write a scalar (or nested struct) value into a byte buffer at the given offset.
fn write_scalar_to_buf(
    buf: &mut [u8],
    offset: usize,
    base_type: BaseType,
    schema: &Schema<'_>,
    field: &Field<'_>,
    json_val: &Value,
) -> Result<()> {
    match base_type {
        BaseType::Bool => {
            let v = json_val.as_bool().unwrap_or(false);
            buf[offset] = if v { 1 } else { 0 };
        }
        BaseType::Byte => {
            let v = resolve_integer_or_enum(schema, field, json_val).unwrap_or(0) as i8;
            buf[offset..offset + 1].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::UByte => {
            let v = resolve_integer_or_enum(schema, field, json_val).unwrap_or(0) as u8;
            buf[offset..offset + 1].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::Short => {
            let v = resolve_integer_or_enum(schema, field, json_val).unwrap_or(0) as i16;
            buf[offset..offset + 2].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::UShort => {
            let v = resolve_integer_or_enum(schema, field, json_val).unwrap_or(0) as u16;
            buf[offset..offset + 2].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::Int => {
            let v = resolve_integer_or_enum(schema, field, json_val).unwrap_or(0) as i32;
            buf[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::UInt => {
            let v = resolve_integer_or_enum(schema, field, json_val).unwrap_or(0) as u32;
            buf[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::Long => {
            let v = resolve_integer_or_enum(schema, field, json_val).unwrap_or(0);
            buf[offset..offset + 8].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::ULong => {
            let v = resolve_integer_or_enum(schema, field, json_val).unwrap_or(0) as u64;
            buf[offset..offset + 8].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::Float => {
            let v = json_val.as_f64().unwrap_or(0.0) as f32;
            buf[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::Double => {
            let v = json_val.as_f64().unwrap_or(0.0);
            buf[offset..offset + 8].copy_from_slice(&v.to_le_bytes());
        }
        BaseType::Obj => {
            // Nested struct — recursively fill bytes at offset.
            let idx = field.type_().index();
            let nested_obj = schema.objects().get(idx as usize);
            let nested_size = nested_obj.bytesize() as usize;

            let child_json = json_val.as_object();
            let empty_map = serde_json::Map::new();
            let child = child_json.unwrap_or(&empty_map);

            let nested_fields = nested_obj.fields();
            for j in 0..nested_fields.len() {
                let nf = nested_fields.get(j);
                let nf_name = nf.name();
                let nf_offset = nf.offset() as usize;
                let nf_base = nf.type_().base_type();

                if offset + nf_offset + scalar_size(nf_base) > buf.len() {
                    bail!(
                        "nested struct field '{}' overflows buffer at offset {}",
                        nf_name,
                        offset + nf_offset
                    );
                }

                if let Some(nf_val) = child.get(nf_name) {
                    write_scalar_to_buf(
                        &mut buf[offset..offset + nested_size],
                        nf_offset,
                        nf_base,
                        schema,
                        &nf,
                        nf_val,
                    )?;
                }
            }
        }
        _ => {
            bail!(
                "unsupported type {:?} in struct field '{}'",
                base_type.variant_name(),
                field.name()
            );
        }
    }
    Ok(())
}

/// Return the byte size of a scalar base type.
fn scalar_size(base_type: BaseType) -> usize {
    match base_type {
        BaseType::Bool | BaseType::Byte | BaseType::UByte => 1,
        BaseType::Short | BaseType::UShort => 2,
        BaseType::Int | BaseType::UInt | BaseType::Float => 4,
        BaseType::Long | BaseType::ULong | BaseType::Double => 8,
        _ => 0, // Non-scalar types; caller must handle separately.
    }
}

/// Build a FlatBuffers table from a JSON object using the schema's object definition.
///
/// Uses a two-pass approach:
/// 1. Pre-build all variable-length fields (strings, nested tables, vectors, unions)
/// 2. Start the table and push all fields (scalars inline, pre-built via saved offsets)
fn build_table<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    schema: &Schema<'_>,
    object: &Object<'_>,
    json_obj: &serde_json::Map<String, Value>,
    output: &dyn crate::presentation::Output,
) -> Result<WIPOffset<flatbuffers::TableFinishedWIPOffset>> {
    let fields = object.fields();
    let num_fields = fields.len();

    // Pass 1: Pre-build variable-length fields.
    // Key: field index in the fields vector. Value: pre-built offset.
    let mut prebuilt: Vec<Option<PrebuiltOffset>> = vec![None; num_fields];
    // For unions we also need the discriminator value.
    let mut union_types: Vec<Option<u8>> = vec![None; num_fields];

    for i in 0..num_fields {
        let field = fields.get(i);
        let field_name = field.name();

        if field.deprecated() {
            continue;
        }

        let base_type = field.type_().base_type();

        // UType fields are handled together with their union value field.
        if base_type == BaseType::UType {
            continue;
        }

        let json_val = match json_obj.get(field_name) {
            Some(v) => v,
            None => continue,
        };

        match base_type {
            BaseType::String => {
                let s = json_val
                    .as_str()
                    .with_context(|| format!("field '{}' expected string", field_name))?;
                let off = builder.create_string(s);
                prebuilt[i] = Some(PrebuiltOffset::from_wip(off));
            }
            BaseType::Obj => {
                let idx = field.type_().index();
                let child_obj = schema.objects().get(idx as usize);
                if child_obj.is_struct() {
                    // Structs are written inline during the table pass.
                } else {
                    let child_json = json_val
                        .as_object()
                        .with_context(|| format!("field '{}' expected object", field_name))?;
                    let off = build_table(builder, schema, &child_obj, child_json, output)?;
                    prebuilt[i] = Some(PrebuiltOffset::from_wip(off));
                }
            }
            BaseType::Vector | BaseType::Vector64 => {
                let off = build_vector(builder, schema, &field, json_val, output)?;
                prebuilt[i] = Some(off);
            }
            BaseType::Union => {
                let (type_val, off) =
                    build_union_value(builder, schema, &field, json_obj, field_name, output)?;
                prebuilt[i] = Some(off);
                // Find the corresponding UType field and store the discriminator.
                union_types[i] = Some(type_val);
            }
            _ => {
                // Scalars, enums, bools — handled in pass 2.
            }
        }
    }

    // Pass 2: Build the table.
    let table_start = builder.start_table();

    for i in 0..num_fields {
        let field = fields.get(i);
        let field_name = field.name();

        if field.deprecated() {
            continue;
        }

        let slot = flatbuffers::field_index_to_field_offset(field.id());
        let base_type = field.type_().base_type();

        // Handle UType discriminators (pushed as u8).
        if base_type == BaseType::UType {
            // Find the union value field that this UType belongs to.
            // Convention: UType field name is "<union_field>_type" or the union
            // value field has base_type == Union and its name matches ours minus "_type".
            if let Some(type_val) = find_union_type_value(&fields, i, &union_types) {
                builder.push_slot::<u8>(slot, type_val, 0);
            } else if let Some(json_val) = json_obj.get(field_name) {
                // Fallback: try to resolve from JSON directly.
                let type_val = resolve_utype_from_json(schema, &field, json_val)?;
                builder.push_slot::<u8>(slot, type_val, 0);
            }
            continue;
        }

        let json_val = match json_obj.get(field_name) {
            Some(v) => v,
            None => {
                // For optional fields with no JSON value, skip (use default).
                continue;
            }
        };

        if let Some(pre) = prebuilt[i] {
            match base_type {
                BaseType::String | BaseType::Vector | BaseType::Vector64 => {
                    let wip: WIPOffset<flatbuffers::ForwardsUOffset<&str>> =
                        WIPOffset::new(pre.0);
                    builder.push_slot_always(slot, wip);
                }
                BaseType::Obj => {
                    let wip: WIPOffset<flatbuffers::ForwardsUOffset<&str>> =
                        WIPOffset::new(pre.0);
                    builder.push_slot_always(slot, wip);
                }
                BaseType::Union => {
                    builder.push_slot_always(slot, pre.as_union_wip());
                }
                _ => {}
            }
            continue;
        }

        // Inline values: scalars, enums, structs.
        match base_type {
            BaseType::Bool => {
                let v = json_val
                    .as_bool()
                    .with_context(|| format!("field '{}' expected bool", field_name))?;
                builder.push_slot::<bool>(slot, v, field.default_integer() != 0);
            }
            BaseType::Byte => {
                let v = resolve_integer_or_enum(schema, &field, json_val)? as i8;
                builder.push_slot::<i8>(slot, v, field.default_integer() as i8);
            }
            BaseType::UByte => {
                let v = resolve_integer_or_enum(schema, &field, json_val)? as u8;
                builder.push_slot::<u8>(slot, v, field.default_integer() as u8);
            }
            BaseType::Short => {
                let v = resolve_integer_or_enum(schema, &field, json_val)? as i16;
                builder.push_slot::<i16>(slot, v, field.default_integer() as i16);
            }
            BaseType::UShort => {
                let v = resolve_integer_or_enum(schema, &field, json_val)? as u16;
                builder.push_slot::<u16>(slot, v, field.default_integer() as u16);
            }
            BaseType::Int => {
                let v = resolve_integer_or_enum(schema, &field, json_val)? as i32;
                builder.push_slot::<i32>(slot, v, field.default_integer() as i32);
            }
            BaseType::UInt => {
                let v = resolve_integer_or_enum(schema, &field, json_val)? as u32;
                builder.push_slot::<u32>(slot, v, field.default_integer() as u32);
            }
            BaseType::Long => {
                let v = resolve_integer_or_enum(schema, &field, json_val)?;
                builder.push_slot::<i64>(slot, v, field.default_integer());
            }
            BaseType::ULong => {
                let v = resolve_integer_or_enum(schema, &field, json_val)? as u64;
                builder.push_slot::<u64>(slot, v, field.default_integer() as u64);
            }
            BaseType::Float => {
                let v = json_val
                    .as_f64()
                    .with_context(|| format!("field '{}' expected number", field_name))?
                    as f32;
                builder.push_slot::<f32>(slot, v, field.default_real() as f32);
            }
            BaseType::Double => {
                let v = json_val
                    .as_f64()
                    .with_context(|| format!("field '{}' expected number", field_name))?;
                builder.push_slot::<f64>(slot, v, field.default_real());
            }
            BaseType::Obj => {
                // Inline struct (prebuilt was None, meaning it's a struct).
                let idx = field.type_().index();
                let struct_obj = schema.objects().get(idx as usize);
                let child_json = json_val
                    .as_object()
                    .with_context(|| format!("field '{}' expected object", field_name))?;
                let dyn_struct = build_struct_bytes(schema, &struct_obj, child_json)?;
                dyn_struct.push_to_builder(builder, slot);
            }
            _ => {
                // Unknown or unhandled type — skip silently.
                output.warning(&format!(
                    "skipping field '{}' with unhandled base type {:?}",
                    field_name,
                    base_type.variant_name()
                ));
            }
        }
    }

    Ok(builder.end_table(table_start))
}

// build_struct removed — struct building is handled via build_struct_bytes + DynStruct

/// Build a vector field and return its offset.
fn build_vector<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    schema: &Schema<'_>,
    field: &Field<'_>,
    json_val: &Value,
    output: &dyn crate::presentation::Output,
) -> Result<PrebuiltOffset> {
    let arr = json_val
        .as_array()
        .with_context(|| format!("field '{}' expected array", field.name()))?;

    let element_type = field.type_().element();

    match element_type {
        BaseType::Bool => {
            let items: Vec<bool> = arr
                .iter()
                .map(|v| v.as_bool().unwrap_or(false))
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::Byte => {
            let items: Vec<i8> = arr
                .iter()
                .map(|v| v.as_i64().unwrap_or(0) as i8)
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::UByte => {
            let items: Vec<u8> = arr
                .iter()
                .map(|v| v.as_i64().unwrap_or(0) as u8)
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::Short => {
            let items: Vec<i16> = arr
                .iter()
                .map(|v| v.as_i64().unwrap_or(0) as i16)
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::UShort => {
            let items: Vec<u16> = arr
                .iter()
                .map(|v| v.as_i64().unwrap_or(0) as u16)
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::Int => {
            let items: Vec<i32> = arr
                .iter()
                .map(|v| v.as_i64().unwrap_or(0) as i32)
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::UInt => {
            let items: Vec<u32> = arr
                .iter()
                .map(|v| v.as_i64().unwrap_or(0) as u32)
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::Long => {
            let items: Vec<i64> = arr
                .iter()
                .map(|v| v.as_i64().unwrap_or(0))
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::ULong => {
            let items: Vec<u64> = arr
                .iter()
                .map(|v| v.as_u64().unwrap_or(0))
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::Float => {
            let items: Vec<f32> = arr
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::Double => {
            let items: Vec<f64> = arr
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0))
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::String => {
            let strings: Vec<WIPOffset<&str>> = arr
                .iter()
                .map(|v| builder.create_string(v.as_str().unwrap_or("")))
                .collect();
            let off = builder.create_vector(&strings);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::Obj => {
            let idx = field.type_().index();
            let child_obj = schema.objects().get(idx as usize);

            if child_obj.is_struct() {
                // Vector of structs: build each struct as raw bytes, then
                // concatenate into a single byte vector.
                let struct_size = child_obj.bytesize() as usize;
                let mut all_bytes: Vec<u8> = Vec::with_capacity(arr.len() * struct_size);
                for v in arr {
                    let child_json = v.as_object();
                    let empty_map = serde_json::Map::new();
                    let child = child_json.unwrap_or(&empty_map);
                    let ds = build_struct_bytes(schema, &child_obj, child)?;
                    all_bytes.extend_from_slice(&ds.data);
                }
                let off = builder.create_vector(&all_bytes);
                Ok(PrebuiltOffset::from_wip(off))
            } else {
                // Vector of tables.
                let mut offsets: Vec<WIPOffset<flatbuffers::TableFinishedWIPOffset>> = Vec::with_capacity(arr.len());
                for v in arr {
                    let child_json = v
                        .as_object()
                        .with_context(|| {
                            format!("vector element in '{}' expected object", field.name())
                        })?;
                    let off = build_table(builder, schema, &child_obj, child_json, output)?;
                    offsets.push(off);
                }
                let off = builder.create_vector(&offsets);
                Ok(PrebuiltOffset::from_wip(off))
            }
        }
        BaseType::Union => {
            // Vector of unions: each element is a table whose type is determined
            // by the companion `<field>_type` vector in the parent JSON.
            // We build each element as a table based on its resolved variant.
            let enum_idx = field.type_().index();
            let union_enum = schema.enums().get(enum_idx as usize);

            let mut offsets: Vec<WIPOffset<flatbuffers::TableFinishedWIPOffset>> =
                Vec::with_capacity(arr.len());

            for v in arr {
                // Each element in a union vector is a table.
                // The type discriminator comes from the companion _type vector,
                // but we don't have it here. Instead, we look at the JSON object
                // fields to determine the variant — union vector elements are
                // typically all the same type, or we pick the first non-NONE variant.
                // In FlatBuffers JSON, vector-of-union elements are just objects
                // whose type is implied by the companion _type array.
                // We need to try each non-NONE variant until one succeeds.
                let child_json = v
                    .as_object()
                    .with_context(|| format!("union vector element in '{}' expected object", field.name()))?;

                let mut built = false;
                let values = union_enum.values();
                for j in 0..values.len() {
                    let ev = values.get(j);
                    if ev.value() == 0 {
                        continue; // Skip NONE
                    }
                    if let Some(ut) = ev.union_type() {
                        if ut.base_type() == BaseType::Obj {
                            let obj_idx = ut.index();
                            let variant_obj = schema.objects().get(obj_idx as usize);
                            // Try building with this variant's schema
                            match build_table(builder, schema, &variant_obj, child_json, output) {
                                Ok(off) => {
                                    offsets.push(off);
                                    built = true;
                                    break;
                                }
                                Err(_) => continue,
                            }
                        }
                    }
                }
                if !built {
                    bail!(
                        "could not resolve union vector element in field '{}'",
                        field.name()
                    );
                }
            }
            let off = builder.create_vector(&offsets);
            Ok(PrebuiltOffset::from_wip(off))
        }
        BaseType::UType => {
            // Vector of union type discriminators (e.g., sounds_type).
            // Values are enum variant names or integers, resolved to u8.
            let enum_idx = field.type_().index();
            let items: Vec<u8> = arr
                .iter()
                .map(|v| {
                    if let Some(n) = v.as_i64() {
                        n as u8
                    } else if let Some(s) = v.as_str() {
                        if enum_idx >= 0 {
                            resolve_enum_name(schema, enum_idx as usize, s).unwrap_or(0) as u8
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                })
                .collect();
            let off = builder.create_vector(&items);
            Ok(PrebuiltOffset::from_wip(off))
        }
        _ => {
            // For enum-typed vectors, try to resolve as integers.
            let enum_idx = field.type_().index();
            let items: Vec<i64> = arr
                .iter()
                .map(|v| {
                    if let Some(n) = v.as_i64() {
                        n
                    } else if let Some(s) = v.as_str() {
                        if enum_idx >= 0 {
                            resolve_enum_name(schema, enum_idx as usize, s).unwrap_or(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                })
                .collect();

            let i32_items: Vec<i32> = items.iter().map(|&v| v as i32).collect();
            let off = builder.create_vector(&i32_items);
            Ok(PrebuiltOffset::from_wip(off))
        }
    }
}

/// Build a union value from JSON.
///
/// Unions in FlatBuffers have a type discriminator field (`<field>_type`)
/// and a value field (`<field>`). This function reads the type from JSON,
/// resolves the variant, and builds the union value (typically a table).
///
/// Returns `(type_discriminator, prebuilt_offset)`.
fn build_union_value(
    builder: &mut FlatBufferBuilder<'_>,
    schema: &Schema<'_>,
    field: &Field<'_>,
    json_obj: &serde_json::Map<String, Value>,
    field_name: &str,
    output: &dyn crate::presentation::Output,
) -> Result<(u8, PrebuiltOffset)> {
    let enum_idx = field.type_().index();
    let union_enum = schema.enums().get(enum_idx as usize);
    let type_field_name = format!("{}_type", field_name);

    let type_json = json_obj
        .get(&type_field_name)
        .with_context(|| format!("union field '{}' missing type discriminator '{}'", field_name, type_field_name))?;

    let (type_val, variant) = resolve_union_variant(&union_enum, type_json)?;

    let value_json = json_obj
        .get(field_name)
        .with_context(|| format!("union field '{}' missing value", field_name))?;

    // Get the variant's type info to find which table to build.
    let variant_type = variant
        .union_type()
        .with_context(|| format!("union variant has no type info"))?;

    let obj_idx = variant_type.index();
    let variant_obj = schema.objects().get(obj_idx as usize);

    let child_json = value_json
        .as_object()
        .with_context(|| format!("union value '{}' expected object", field_name))?;

    let off = build_table(builder, schema, &variant_obj, child_json, output)?;

    Ok((type_val, PrebuiltOffset::from_wip(off)))
}

/// Resolve a union variant from a JSON value (string name or integer).
///
/// Returns `(discriminator_u8, EnumVal)`.
fn resolve_union_variant<'a>(
    union_enum: &Enum<'a>,
    json_val: &Value,
) -> Result<(u8, flatbuffers_reflection::reflection::EnumVal<'a>)> {
    let values = union_enum.values();

    if let Some(s) = json_val.as_str() {
        for i in 0..values.len() {
            let ev = values.get(i);
            if ev.name() == s {
                return Ok((ev.value() as u8, ev));
            }
        }
        bail!(
            "unknown union variant '{}' in enum '{}'",
            s,
            union_enum.name()
        );
    } else if let Some(n) = json_val.as_i64() {
        for i in 0..values.len() {
            let ev = values.get(i);
            if ev.value() == n {
                return Ok((n as u8, ev));
            }
        }
        bail!(
            "unknown union variant value {} in enum '{}'",
            n,
            union_enum.name()
        );
    } else {
        bail!("union type discriminator must be a string or integer");
    }
}

/// Find the union type value for a UType field by looking at the paired Union field.
fn find_union_type_value(
    fields: &flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<Field<'_>>>,
    utype_field_idx: usize,
    union_types: &[Option<u8>],
) -> Option<u8> {
    let utype_field = fields.get(utype_field_idx);
    let utype_name = utype_field.name();

    // Convention: if UType field is "foo_type", the union value field is "foo".
    let base_name = utype_name.strip_suffix("_type").unwrap_or(utype_name);

    for i in 0..fields.len() {
        let f = fields.get(i);
        if f.type_().base_type() == BaseType::Union && f.name() == base_name {
            return union_types[i];
        }
    }

    None
}

/// Resolve a UType field value from JSON when no pre-built union was found.
fn resolve_utype_from_json(
    schema: &Schema<'_>,
    field: &Field<'_>,
    json_val: &Value,
) -> Result<u8> {
    if let Some(n) = json_val.as_i64() {
        return Ok(n as u8);
    }
    if let Some(s) = json_val.as_str() {
        let enum_idx = field.type_().index();
        if enum_idx >= 0 {
            let val = resolve_enum_name(schema, enum_idx as usize, s)?;
            return Ok(val as u8);
        }
    }
    bail!("cannot resolve UType value for field '{}'", field.name());
}

/// Resolve a JSON value that may be an integer or an enum name string.
///
/// If the field has an enum type (index >= 0 pointing into schema.enums()),
/// string values are resolved to enum integer values.
fn resolve_integer_or_enum(
    schema: &Schema<'_>,
    field: &Field<'_>,
    json_val: &Value,
) -> Result<i64> {
    if let Some(n) = json_val.as_i64() {
        return Ok(n);
    }
    if let Some(n) = json_val.as_u64() {
        return Ok(n as i64);
    }
    if let Some(n) = json_val.as_f64() {
        return Ok(n as i64);
    }
    if let Some(s) = json_val.as_str() {
        let enum_idx = field.type_().index();
        if enum_idx >= 0 {
            return resolve_enum_name(schema, enum_idx as usize, s);
        }
        // Try parsing as integer string.
        if let Ok(n) = s.parse::<i64>() {
            return Ok(n);
        }
        bail!(
            "cannot resolve string '{}' for field '{}' (no enum type)",
            s,
            field.name()
        );
    }
    if json_val.is_null() {
        return Ok(field.default_integer());
    }
    bail!(
        "expected integer or enum name for field '{}', got {:?}",
        field.name(),
        json_val
    );
}

/// Resolve an enum value name to its integer value.
fn resolve_enum_name(schema: &Schema<'_>, enum_idx: usize, name: &str) -> Result<i64> {
    let enums = schema.enums();
    if enum_idx >= enums.len() {
        bail!("enum index {} out of range", enum_idx);
    }
    let enum_def = enums.get(enum_idx);
    let values = enum_def.values();
    for i in 0..values.len() {
        let ev = values.get(i);
        if ev.name() == name {
            return Ok(ev.value());
        }
    }
    bail!(
        "unknown enum value '{}' in enum '{}'",
        name,
        enum_def.name()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prebuilt_offset_roundtrip() {
        let wip: WIPOffset<&str> = WIPOffset::new(42);
        let pre = PrebuiltOffset::from_wip(wip);
        assert_eq!(pre.0, 42);
        let union_wip = pre.as_union_wip();
        assert_eq!(union_wip.value(), 42);
    }
}
