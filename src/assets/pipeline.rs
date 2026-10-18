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

//! Pipeline asset type.
//!
//! A pipeline is a processing chain of nodes the SDK runs audio through.

use super::generated::{PipelineDefinition, PipelineNodeDefinition};
use super::pipeline_graph::{builtin_node, validate_graph};
use super::{Asset, AssetType, ProjectContext, Schema, ValidationError};

/// Pipeline definition.
///
/// Type alias to the build-time generated `PipelineDefinition`.
///
/// # Example
///
/// ```
/// use am::assets::Pipeline;
///
/// let pipeline = Pipeline::builder(1, "main").build();
/// assert_eq!(pipeline.id, 1);
/// ```
pub type Pipeline = PipelineDefinition;

impl Pipeline {
    /// Creates a builder for constructing a Pipeline.
    pub fn builder(id: u64, name: impl Into<String>) -> PipelineBuilder {
        PipelineBuilder::new(id, name)
    }
}

/// Builder for constructing Pipeline instances.
#[derive(Debug, Clone)]
pub struct PipelineBuilder {
    pipeline: Pipeline,
}

impl PipelineBuilder {
    /// Creates a new builder with the given id and name.
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            pipeline: Pipeline {
                id,
                name: Some(name.into()),
                nodes: None,
            },
        }
    }

    /// Sets all nodes at once.
    pub fn nodes(mut self, nodes: Vec<PipelineNodeDefinition>) -> Self {
        self.pipeline.nodes = Some(nodes);
        self
    }

    /// Appends a single node.
    #[allow(dead_code)]
    pub fn add_node(mut self, node: PipelineNodeDefinition) -> Self {
        self.pipeline.nodes.get_or_insert_with(Vec::new).push(node);
        self
    }

    /// Builds the Pipeline instance.
    pub fn build(self) -> Pipeline {
        self.pipeline
    }
}

impl Asset for Pipeline {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        self.name.as_deref().unwrap_or("")
    }

    fn asset_type(&self) -> AssetType {
        AssetType::Pipeline
    }

    fn file_extension(&self) -> &'static str {
        AssetType::Pipeline.file_extension()
    }

    fn validate_schema(&self, _schema: &Schema) -> Result<(), ValidationError> {
        Ok(())
    }

    fn validate_rules(&self, _context: &ProjectContext) -> Result<(), ValidationError> {
        if self.name.as_deref().unwrap_or("").is_empty() {
            return Err(ValidationError::type_rule_violation(
                "Pipeline asset has no name",
                "Pipeline assets must have a non-empty name",
            )
            .with_suggestion("Set the 'name' field to a valid identifier (e.g., \"main\")")
            .with_field("name"));
        }

        validate_graph(self.nodes.as_deref())
    }
}

impl Pipeline {
    /// Node type names that are not SDK built-ins (for example plugin nodes).
    ///
    /// The SDK only finds them when the plugin is loaded, so they are accepted
    /// but worth a warning.
    pub fn unknown_node_types(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for name in self
            .nodes
            .iter()
            .flatten()
            .filter_map(|n| n.name.as_deref())
        {
            if builtin_node(name).is_none() && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        names
    }
}
