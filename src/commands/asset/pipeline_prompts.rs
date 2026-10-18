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

//! Interactive prompts for `pipeline create` and `pipeline update`.
//!
//! Nodes are entered one at a time; the nodes a new node consumes are picked
//! from the nodes entered so far, never typed. Only called when the session
//! can prompt.

use anyhow::Result;
use inquire::validator::Validation;

use crate::assets::generated::PipelineNodeDefinition;
use crate::assets::{BUILTIN_NODES, InputArity, builtin_node};
use crate::common::errors::{CliError, codes};
use crate::input::Input;
use crate::services::pipeline::next_node_id;

const OTHER_TYPE: &str = "Other (plugin node)";
const DONE: &str = "(done)";
const ADD: &str = "Add a node";
const EDIT: &str = "Edit a node";
const REMOVE: &str = "Remove a node";
const FINISH: &str = "Done";

fn label(node: &PipelineNodeDefinition) -> String {
    format!("{} {}", node.id, node.name.as_deref().unwrap_or("?"))
}

fn validate_numbers(text: &str) -> Result<Validation, inquire::CustomUserError> {
    let ok = text
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .all(|p| p.parse::<f32>().is_ok_and(f32::is_finite));
    Ok(if ok {
        Validation::Valid
    } else {
        Validation::Invalid("Enter comma-separated numbers, or nothing".into())
    })
}

fn ask_parameters(
    input: &dyn Input,
    node_label: &str,
    current: Option<&[f32]>,
) -> Result<Option<Vec<f32>>> {
    let prompt = format!("Parameters of node {node_label} (comma-separated, empty for none)");
    let text = match current {
        Some(values) => {
            let shown: Vec<String> = values.iter().map(f32::to_string).collect();
            input.prompt_text_with_default(&prompt, &shown.join(", "), Some(&validate_numbers))?
        }
        None => input.prompt_text(&prompt, None, None, Some(&validate_numbers))?,
    };
    let values: Vec<f32> = text
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    Ok((!values.is_empty()).then_some(values))
}

fn pick_node<'a>(
    input: &dyn Input,
    prompt: &str,
    nodes: &[&'a PipelineNodeDefinition],
) -> Result<&'a PipelineNodeDefinition> {
    let labels: Vec<String> = nodes.iter().map(|n| label(n)).collect();
    let chosen = input.select(prompt, &labels)?;
    let index = labels.iter().position(|l| *l == chosen).unwrap_or(0);
    Ok(nodes[index])
}

fn no_providers_error(name: &str) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        format!("A {name} node needs a node to consume, but there is none yet"),
        "Consumed nodes are picked from the nodes already entered",
    )
    .with_suggestion("Start the pipeline with an Input node")
}

/// Asks which of `available` the node `name` consumes, as its arity requires.
fn pick_consumed(
    input: &dyn Input,
    name: &str,
    available: &[&PipelineNodeDefinition],
) -> Result<Vec<u32>> {
    let arity = builtin_node(name).unwrap_or(InputArity { min: 0, max: None });
    if !arity.consumes() {
        return Ok(Vec::new());
    }
    if available.is_empty() && arity.min > 0 {
        return Err(no_providers_error(name).into());
    }
    let mut remaining: Vec<&PipelineNodeDefinition> = available.to_vec();
    let mut chosen: Vec<u32> = Vec::new();
    loop {
        if remaining.is_empty() || arity.max == Some(chosen.len()) {
            break;
        }
        let needs_more = chosen.len() < arity.min;
        let prompt = format!("Node consumed by {name} (input {})", chosen.len() + 1);
        let node = if needs_more {
            pick_node(input, &prompt, &remaining)?
        } else {
            let mut labels: Vec<String> = remaining.iter().map(|n| label(n)).collect();
            labels.push(DONE.to_string());
            let answer = input.select(&prompt, &labels)?;
            let Some(index) = labels.iter().position(|l| *l == answer) else {
                break;
            };
            if answer == DONE {
                break;
            }
            remaining[index]
        };
        chosen.push(u32::try_from(node.id)?);
        remaining.retain(|n| n.id != node.id);
    }
    Ok(chosen)
}

fn ask_node_type(input: &dyn Input) -> Result<String> {
    let mut options: Vec<String> = BUILTIN_NODES.iter().map(|(n, _)| n.to_string()).collect();
    options.push(OTHER_TYPE.to_string());
    let chosen = input.select("Node type", &options)?;
    if chosen != OTHER_TYPE {
        return Ok(chosen);
    }
    let validator = |text: &str| -> Result<Validation, inquire::CustomUserError> {
        Ok(if text.trim().is_empty() {
            Validation::Invalid("Enter the node type name".into())
        } else {
            Validation::Valid
        })
    };
    Ok(input
        .prompt_text("Plugin node type name", None, None, Some(&validator))?
        .trim()
        .to_string())
}

/// Asks for one new node with the given id; it may consume any of `existing`.
fn prompt_node(
    input: &dyn Input,
    id: u64,
    existing: &[PipelineNodeDefinition],
) -> Result<PipelineNodeDefinition> {
    let name = ask_node_type(input)?;
    let available: Vec<&PipelineNodeDefinition> = existing.iter().collect();
    let consume = pick_consumed(input, &name, &available)?;
    let parameters = ask_parameters(input, &format!("{id} {name}"), None)?;
    Ok(PipelineNodeDefinition {
        id,
        name: Some(name),
        consume: Some(consume),
        parameters,
    })
}

/// Adds nodes one at a time, after `existing`, until the author is done.
fn prompt_more_nodes(
    input: &dyn Input,
    existing: &[PipelineNodeDefinition],
) -> Result<Vec<PipelineNodeDefinition>> {
    let mut nodes = existing.to_vec();
    loop {
        let node = prompt_node(input, next_node_id(&nodes), &nodes)?;
        nodes = [nodes, vec![node]].concat();
        if !input.confirm("Add another node?", Some(true))? {
            return Ok(nodes);
        }
    }
}

/// Walks a new pipeline through its nodes.
pub fn prompt_new_nodes(input: &dyn Input) -> Result<Vec<PipelineNodeDefinition>> {
    prompt_more_nodes(input, &[])
}

/// Re-asks what the node consumes and its parameters, keeping its id and type.
fn prompt_edit(
    input: &dyn Input,
    node: &PipelineNodeDefinition,
    all: &[PipelineNodeDefinition],
) -> Result<PipelineNodeDefinition> {
    let name = node.name.clone().unwrap_or_default();
    let available: Vec<&PipelineNodeDefinition> = all.iter().filter(|n| n.id != node.id).collect();
    let consume = pick_consumed(input, &name, &available)?;
    let parameters = ask_parameters(input, &label(node), node.parameters.as_deref())?;
    Ok(PipelineNodeDefinition {
        consume: Some(consume),
        parameters,
        ..node.clone()
    })
}

fn consumed_by_others(node: &PipelineNodeDefinition, all: &[PipelineNodeDefinition]) -> bool {
    all.iter().any(|other| {
        other.id != node.id
            && other
                .consume
                .iter()
                .flatten()
                .any(|c| u64::from(*c) == node.id)
    })
}

/// Lets the author add, edit and remove nodes; `None` when nothing changed.
///
/// Only nodes that no other node consumes are offered for removal.
pub fn prompt_update_nodes(
    input: &dyn Input,
    existing: &[PipelineNodeDefinition],
) -> Result<Option<Vec<PipelineNodeDefinition>>> {
    let mut nodes = existing.to_vec();
    loop {
        // Offer only what can act on the current nodes.
        let can_remove = nodes.iter().any(|n| !consumed_by_others(n, &nodes));
        let actions: Vec<String> = [
            (ADD, true),
            (EDIT, !nodes.is_empty()),
            (REMOVE, can_remove),
            (FINISH, true),
        ]
        .into_iter()
        .filter(|&(_, offered)| offered)
        .map(|(name, _)| name.to_string())
        .collect();
        match input.select("What do you want to do?", &actions)?.as_str() {
            ADD => {
                let node = prompt_node(input, next_node_id(&nodes), &nodes)?;
                nodes = [nodes, vec![node]].concat();
            }
            EDIT if !nodes.is_empty() => {
                let refs: Vec<&PipelineNodeDefinition> = nodes.iter().collect();
                let id = pick_node(input, "Node to edit", &refs)?.id;
                nodes = nodes
                    .iter()
                    .map(|n| {
                        if n.id == id {
                            prompt_edit(input, n, &nodes)
                        } else {
                            Ok(n.clone())
                        }
                    })
                    .collect::<Result<_>>()?;
            }
            REMOVE => {
                let removable: Vec<&PipelineNodeDefinition> = nodes
                    .iter()
                    .filter(|n| !consumed_by_others(n, &nodes))
                    .collect();
                if !removable.is_empty() {
                    let id = pick_node(input, "Node to remove", &removable)?.id;
                    nodes.retain(|n| n.id != id);
                }
            }
            _ => break,
        }
    }
    Ok((nodes != existing).then_some(nodes))
}
