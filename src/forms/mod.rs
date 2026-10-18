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

//! Schema-driven field model shared by prompts, flags and the TUI.
//!
//! Part of this model is consumed only by the library API and the tests so far;
//! the binary target compiles these modules privately and does not use all of it.

#![allow(dead_code)]

pub mod coverage;
pub mod curve;
pub mod describe;
pub mod field;
pub mod field_docs;
pub mod in_use;
pub mod pick;
pub mod references;
pub mod rtpc_value;
pub mod soundbank_refs;
pub mod validate_refs;
pub mod zone;
