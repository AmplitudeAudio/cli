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

//! Field model describing one asset field independent of any front end.

use super::references::RefTarget;

/// The shape of a field's value.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Bool,
    Int,
    Float,
    Text,
    /// An enum; carries the variant names.
    Enum(Vec<String>),
    /// A value that is either constant or driven by an RTPC.
    RtpcValue,
    /// The ID of another object.
    Reference(RefTarget),
    /// A nested table (or union), by schema name.
    Nested(String),
    List(Box<FieldKind>),
}

/// Description of one schema field.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSpec {
    pub name: String,
    pub kind: FieldKind,
    pub doc: Option<String>,
    pub default: Option<serde_json::Value>,
    pub required: bool,
}
