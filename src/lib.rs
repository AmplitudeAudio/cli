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

//! Amplitude CLI library crate.
//!
//! This module exposes public APIs for testing and external use.

// `ValidationError` carries its explanation, suggestion and field inline (136 bytes) and is
// returned from every rule; boxing it everywhere would only trade clarity for a size heuristic.
#![allow(clippy::result_large_err)]

pub mod app;
pub mod assets;
pub mod common;
pub mod compiler;
pub mod config;
pub mod database;
pub mod forms;
pub mod input;
pub mod presentation;
pub mod schema;
pub mod services;

// Re-export commands for testing
pub mod commands {
    pub mod asset;
    pub mod project;
    mod project_root_files;
    pub mod sdk;
    pub mod sudo;
    pub mod template;
}
