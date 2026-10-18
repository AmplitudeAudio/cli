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

//! Pipeline creation and update logic, independent of how values were gathered.
//!
//! Flags, prompts and the future TUI produce [`PipelineValues`] (create) or a
//! [`PipelinePatch`] (update); this module turns them into a validated
//! [`Pipeline`] without touching the terminal or the file system. Functions
//! return new values and never mutate their inputs.

use std::collections::HashSet;

use crate::assets::generated::PipelineNodeDefinition;
use crate::assets::{Asset, Pipeline, ProjectContext, ValidationError, ValidationLayer};
use crate::common::errors::codes::{ERR_ASSET_ALREADY_EXISTS, ERR_VALIDATION_REFERENCE};
use crate::common::errors::{CliError, codes};
use crate::common::utils::generate_unique_id;

/// Number of times id generation is retried after a collision.
const ID_COLLISION_RETRIES: usize = 3;

/// Every value a new pipeline needs.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineValues {
    pub name: String,
    pub nodes: Vec<PipelineNodeDefinition>,
}

/// Requested changes to the node list of an existing pipeline.
///
/// Applied in this order: `replace` (the whole list), then `remove`, then `add`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PipelinePatch {
    pub replace: Option<Vec<PipelineNodeDefinition>>,
    pub add: Vec<PipelineNodeDefinition>,
    /// Ids of the nodes to remove.
    pub remove: Vec<u64>,
}

impl PipelinePatch {
    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self.replace.is_none() && self.add.is_empty() && self.remove.is_empty()
    }

    /// Schema names of the fields this patch sets.
    pub fn changed_fields(&self) -> Vec<&'static str> {
        if self.is_empty() {
            Vec::new()
        } else {
            vec!["nodes"]
        }
    }
}

/// The next free node id: one above the highest in use (1 for an empty list).
pub fn next_node_id(nodes: &[PipelineNodeDefinition]) -> u64 {
    nodes.iter().map(|n| n.id).max().map_or(1, |max| max + 1)
}

/// A readable name for a node in messages, such as `Output (id 3)`.
pub fn node_label(node: &PipelineNodeDefinition) -> String {
    format!("{} (id {})", node.name.as_deref().unwrap_or("?"), node.id)
}

fn spec_error(flag: &str, spec: &str, why: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FORMAT,
        format!("Invalid {flag} value '{spec}'"),
        why.to_string(),
    )
    .with_suggestion(format!(
        "Use {flag} ID:Type[:CONSUMED_IDS[:PARAMETERS]], e.g. {flag} 2:Reverb:1:0.5,0.25"
    ))
}

fn parse_list<T: std::str::FromStr>(
    flag: &str,
    spec: &str,
    text: &str,
    what: &str,
) -> Result<Vec<T>, CliError> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<T>()
                .map_err(|_| spec_error(flag, spec, &format!("'{part}' is not a valid {what}")))
        })
        .collect()
}

/// Parses `ID:Type[:CONSUMED_IDS[:PARAMETERS]]` into a node.
///
/// `ID` is the node id (`id`), `Type` the node type name (`name`),
/// `CONSUMED_IDS` a comma-separated list of node ids (`consume`) and
/// `PARAMETERS` a comma-separated list of numbers (`parameters`).
/// `flag` is only used to word errors.
pub fn parse_node_spec(flag: &str, spec: &str) -> Result<PipelineNodeDefinition, CliError> {
    let parts: Vec<&str> = spec.split(':').collect();
    if !(2..=4).contains(&parts.len()) {
        return Err(spec_error(
            flag,
            spec,
            "A node needs an id and a type, optionally followed by the consumed node ids and the parameters",
        ));
    }
    let id = parts[0]
        .trim()
        .parse::<u64>()
        .map_err(|_| spec_error(flag, spec, "The node id must be a whole number"))?;
    let name = parts[1].trim();
    if name.is_empty() {
        return Err(spec_error(flag, spec, "The node type is empty"));
    }
    let consume = parse_list::<u32>(flag, spec, parts.get(2).copied().unwrap_or(""), "node id")?;
    let parameters = match parts.get(3) {
        Some(text) => Some(parse_list::<f32>(flag, spec, text, "number")?),
        None => None,
    };
    Ok(PipelineNodeDefinition {
        id,
        name: Some(name.to_string()),
        consume: Some(consume),
        parameters,
    })
}

fn unique_id(name: &str, ctx: &ProjectContext) -> Result<u64, ValidationError> {
    let mut id = generate_unique_id(name);
    for attempt in 0..ID_COLLISION_RETRIES {
        if !ctx.has_id(id) {
            break;
        }
        id = generate_unique_id(&format!("{name}{attempt}"));
    }
    if ctx.has_id(id) {
        return Err(ValidationError::new(
            ValidationLayer::IdUniqueness,
            ERR_ASSET_ALREADY_EXISTS,
            format!("Generated ID {id} for pipeline '{name}' collides with an existing asset"),
            "All generated ID attempts collided with existing assets in the project",
        )
        .with_suggestion("Try a different name or wait a moment and retry")
        .with_field("id"));
    }
    Ok(id)
}

/// Builds a validated pipeline from `values`, generating a unique id.
pub fn build_pipeline(
    values: PipelineValues,
    ctx: &ProjectContext,
) -> Result<Pipeline, ValidationError> {
    let id = unique_id(&values.name, ctx)?;
    let pipeline = Pipeline::builder(id, &values.name)
        .nodes(values.nodes)
        .build();
    pipeline.validate_rules(ctx)?;
    Ok(pipeline)
}

fn reference_error(what: String, why: String, suggestion: &str) -> ValidationError {
    ValidationError::new(
        ValidationLayer::TypeRules,
        ERR_VALIDATION_REFERENCE,
        what,
        why,
    )
    .with_suggestion(suggestion)
    .with_field("nodes")
}

/// Removes `ids` from `nodes`; fails when a node that stays consumes one of them.
fn remove_nodes(
    nodes: Vec<PipelineNodeDefinition>,
    ids: &[u64],
) -> Result<Vec<PipelineNodeDefinition>, ValidationError> {
    let removing: HashSet<u64> = ids.iter().copied().collect();
    for id in &removing {
        if !nodes.iter().any(|n| n.id == *id) {
            return Err(reference_error(
                format!("Cannot remove node {id}: the pipeline has no such node"),
                "Nodes are identified by their id".to_string(),
                "Check the node ids with 'am asset pipeline list --json'",
            ));
        }
    }
    for id in &removing {
        let consumers: Vec<String> = nodes
            .iter()
            .filter(|n| !removing.contains(&n.id))
            .filter(|n| n.consume.iter().flatten().any(|c| u64::from(*c) == *id))
            .map(node_label)
            .collect();
        if !consumers.is_empty() {
            return Err(reference_error(
                format!("Cannot remove node {id}: other nodes consume it"),
                format!("It is consumed by: {}", consumers.join(", ")),
                "Remove or rewire the consuming nodes first (or remove them in the same command)",
            ));
        }
    }
    Ok(nodes
        .into_iter()
        .filter(|n| !removing.contains(&n.id))
        .collect())
}

/// Applies `patch` to a copy of `existing`'s nodes; the result is not yet validated.
pub fn apply_patch(
    existing: &Pipeline,
    patch: &PipelinePatch,
) -> Result<Pipeline, ValidationError> {
    let base = patch
        .replace
        .clone()
        .or_else(|| existing.nodes.clone())
        .unwrap_or_default();
    let mut nodes = remove_nodes(base, &patch.remove)?;
    for added in &patch.add {
        if nodes.iter().any(|n| n.id == added.id) {
            return Err(reference_error(
                format!("Cannot add node {}: the id is already in use", added.id),
                format!(
                    "Node ids must be unique; next free id is {}",
                    next_node_id(&nodes)
                ),
                "Give the new node an unused id",
            ));
        }
        nodes.push(added.clone());
    }
    let mut pipeline = existing.clone();
    pipeline.nodes = Some(nodes);
    Ok(pipeline)
}
