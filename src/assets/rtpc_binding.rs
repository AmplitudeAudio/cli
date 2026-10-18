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

//! The RTPC binding of an `RtpcCompatibleValue`.
//!
//! `RtpcValue::Init` (`Rtpc.cpp`) reads `rtpc()->id()` and `rtpc()->curve()`
//! of an RTPC-kind value without a check, so such a value needs an `rtpc`
//! object whose id names an existing RTPC. Static values are not looked at,
//! and nothing beyond what the SDK dereferences is required (the curve is
//! not checked).

use super::generated::{RtpcCompatibleValue, ValueKind};
use super::{AssetType, ProjectValidator, ValidationError};
use crate::forms::references::{RefRule, RefTarget};
use crate::forms::validate_refs::validate_reference;

/// Checks that an RTPC-kind `value` names an existing RTPC.
///
/// `rule_field` is the schema field of `owner` (`gain`, `sounds.pitch`, ...);
/// `path` is where the value sits in the file (`sounds[2].pitch`) and is
/// reported as the failing field.
pub fn check_rtpc_binding(
    owner: AssetType,
    rule_field: &'static str,
    path: &str,
    value: Option<&RtpcCompatibleValue>,
    validator: &ProjectValidator,
) -> Result<(), ValidationError> {
    let Some(value) = value.filter(|v| v.kind == ValueKind::RTPC) else {
        return Ok(());
    };
    let Some(rtpc) = value.rtpc.as_ref() else {
        return Err(ValidationError::type_rule_violation(
            format!("'{path}' is bound to an RTPC but names none"),
            "An RTPC-kind value needs an 'rtpc' object with the RTPC id; the SDK reads it without a check",
        )
        .with_suggestion(
            "Bind the value to an RTPC (see 'am asset rtpc list'), or make it a static number",
        )
        .with_field(path));
    };
    let rule = RefRule {
        asset_type: owner,
        field: rule_field,
        target: RefTarget::Asset(AssetType::Rtpc),
        required: true,
    };
    validate_reference(&rule, rtpc.id, validator).map_err(|e| e.with_field(path))
}
