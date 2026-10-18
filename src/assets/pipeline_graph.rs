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

//! Pipeline node graph rules, mirroring what the SDK checks or dereferences
//! when it builds a pipeline (`PipelineImpl::CreateInstance` and
//! `PipelineInstanceImpl::ConfigureNodeGraph` in `Mixer/Pipeline.cpp`).

use std::collections::{HashMap, HashSet};

use super::ValidationError;
use super::generated::PipelineNodeDefinition;

/// How many nodes a node type consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputArity {
    pub min: usize,
    /// `None` means unbounded.
    pub max: Option<usize>,
}

impl InputArity {
    /// Whether `count` inputs are acceptable.
    pub fn allows(&self, count: usize) -> bool {
        count >= self.min && self.max.is_none_or(|max| count <= max)
    }

    /// Whether the node type reads inputs at all.
    pub fn consumes(&self) -> bool {
        self.max != Some(0)
    }

    fn describe(&self) -> String {
        match (self.min, self.max) {
            (0, Some(0)) => "no inputs".to_string(),
            (min, Some(max)) if min == max => format!("exactly {min} input(s)"),
            (min, None) => format!("at least {min} input(s)"),
            (min, Some(max)) => format!("{min} to {max} input(s)"),
        }
    }
}

const PRODUCER: InputArity = InputArity {
    min: 0,
    max: Some(0),
};
const SINGLE: InputArity = InputArity {
    min: 1,
    max: Some(1),
};
const MIXER: InputArity = InputArity { min: 1, max: None };

/// Node types the SDK registers by default (`Engine::RegisterDefaultExtensions`),
/// with the inputs each takes (`Node::GetMinInputCount` / `GetMaxInputCount`).
/// "StereoMixer" and "StereoPanning" are legacy aliases the SDK still loads.
pub const BUILTIN_NODES: &[(&str, InputArity)] = &[
    ("Input", PRODUCER),
    ("Output", SINGLE),
    ("Mixer", MIXER),
    ("StereoMixer", MIXER),
    ("AmbisonicMixer", MIXER),
    ("SpeakerPanning", SINGLE),
    ("StereoPanning", SINGLE),
    ("Reverb", SINGLE),
    ("Freeverb", SINGLE),
    ("Dattorro", SINGLE),
    ("RoomReverb", SINGLE),
    ("Attenuation", SINGLE),
    ("Clamp", SINGLE),
    ("HardClip", SINGLE),
    ("RoundoffClip", SINGLE),
    ("Limiter", SINGLE),
    ("EnvironmentEffect", SINGLE),
    ("NearFieldEffect", SINGLE),
    ("Obstruction", SINGLE),
    ("Occlusion", SINGLE),
    ("Reflections", SINGLE),
    ("AmbisonicBinauralDecoder", SINGLE),
    ("AmbisonicPanning", SINGLE),
    ("AmbisonicRotator", SINGLE),
    ("AmbisonicSpeakerDecoder", SINGLE),
];

/// The arity of a built-in node type, or `None` for unknown (plugin) types.
pub fn builtin_node(name: &str) -> Option<InputArity> {
    BUILTIN_NODES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, arity)| *arity)
}

fn violation(what: impl Into<String>, why: impl Into<String>, suggestion: &str) -> ValidationError {
    ValidationError::type_rule_violation(what, why)
        .with_suggestion(suggestion)
        .with_field("nodes")
}

fn label(node: &PipelineNodeDefinition) -> String {
    format!("{} (id {})", node.name.as_deref().unwrap_or("?"), node.id)
}

fn validate_node_fields(nodes: &[PipelineNodeDefinition]) -> Result<(), ValidationError> {
    let mut seen = HashSet::new();
    for (index, node) in nodes.iter().enumerate() {
        // The SDK dereferences `name()` without a null check.
        if node.name.as_deref().unwrap_or("").is_empty() {
            return Err(violation(
                format!("Pipeline node at position {index} has no name"),
                "The SDK reads the node name to find the node type, so every node needs one",
                "Set the node's 'name' to a node type such as \"Input\", \"Reverb\" or \"Output\"",
            ));
        }
        // `AddNode` silently ignores a repeated id.
        if !seen.insert(node.id) {
            return Err(violation(
                format!("Pipeline node id {} is used twice", node.id),
                "Node ids must be unique within a pipeline; the SDK drops the later node otherwise",
                "Give each node its own id",
            ));
        }
        if node.parameters.iter().flatten().any(|p| !p.is_finite()) {
            return Err(violation(
                format!("Pipeline node {} has a non-finite parameter", label(node)),
                "Node parameters must be finite numbers",
                "Fix the node's 'parameters' list",
            ));
        }
    }
    Ok(())
}

fn validate_single_endpoints(nodes: &[PipelineNodeDefinition]) -> Result<(), ValidationError> {
    for name in ["Input", "Output"] {
        let count = nodes
            .iter()
            .filter(|n| n.name.as_deref() == Some(name))
            .count();
        match count {
            1 => {}
            0 => {
                return Err(violation(
                    format!("Pipeline has no {name} node"),
                    "The SDK refuses a pipeline without exactly one Input and one Output node",
                    &format!("Add a node named \"{name}\""),
                ));
            }
            _ => {
                return Err(violation(
                    format!("Pipeline has more than one {name} node"),
                    "The SDK refuses a pipeline with more than one Input or Output node",
                    &format!("Keep a single \"{name}\" node"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_consume(
    node: &PipelineNodeDefinition,
    ids: &HashSet<u64>,
) -> Result<(), ValidationError> {
    let name = node.name.as_deref().unwrap_or("");
    let arity = builtin_node(name);

    // The SDK reads the size of the 'consume' list of every node that takes inputs.
    if node.consume.is_none() && arity.is_some_and(|a| a.consumes()) {
        return Err(violation(
            format!("Pipeline node {} has no 'consume' list", label(node)),
            "The SDK reads the 'consume' list of every node that takes inputs",
            "Add a 'consume' list naming the node(s) this node reads from",
        ));
    }
    let consume = node.consume.as_deref().unwrap_or(&[]);

    if let Some(arity) = arity
        && !arity.allows(consume.len())
    {
        return Err(violation(
            format!(
                "Pipeline node {} has {} input(s)",
                label(node),
                consume.len()
            ),
            format!("A {name} node takes {}", arity.describe()),
            "Fix the node's 'consume' list",
        ));
    }
    for &target in consume {
        let target = u64::from(target);
        // A dangling provider id is dereferenced while the graph is configured.
        if target == node.id {
            return Err(violation(
                format!("Pipeline node {} consumes itself", label(node)),
                "A node cannot consume itself",
                "Remove the node's own id from its 'consume' list",
            ));
        }
        if !ids.contains(&target) {
            return Err(violation(
                format!(
                    "Pipeline node {} consumes unknown node {target}",
                    label(node)
                ),
                "Every id in 'consume' must be the id of a node of the same pipeline",
                "Add the missing node or remove the id from 'consume'",
            ));
        }
    }
    Ok(())
}

/// Whether following 'consume' links from some node leads back to it.
fn has_cycle(nodes: &[PipelineNodeDefinition]) -> bool {
    let providers: HashMap<u64, Vec<u64>> = nodes
        .iter()
        .map(|n| {
            let ids = n.consume.iter().flatten().map(|&c| u64::from(c)).collect();
            (n.id, ids)
        })
        .collect();

    const ON_PATH: u8 = 1;
    const DONE: u8 = 2;
    let mut state: HashMap<u64, u8> = HashMap::new();
    for &start in providers.keys() {
        if state.contains_key(&start) {
            continue;
        }
        state.insert(start, ON_PATH);
        let mut stack = vec![(start, 0usize)];
        while let Some((id, next)) = stack.pop() {
            let edges = providers.get(&id).map(Vec::as_slice).unwrap_or(&[]);
            let Some(&target) = edges.get(next) else {
                state.insert(id, DONE);
                continue;
            };
            stack.push((id, next + 1));
            match state.get(&target) {
                Some(&ON_PATH) => return true,
                Some(_) => {}
                None => {
                    state.insert(target, ON_PATH);
                    stack.push((target, 0));
                }
            }
        }
    }
    false
}

/// Checks `nodes` the way the SDK builds the graph; the first problem found is returned.
pub fn validate_graph(nodes: Option<&[PipelineNodeDefinition]>) -> Result<(), ValidationError> {
    let nodes = nodes.unwrap_or(&[]);
    if nodes.is_empty() {
        return Err(violation(
            "Pipeline has no nodes",
            "The SDK reads the node list when it builds the pipeline; it cannot be missing or empty",
            "Add at least an Input and an Output node (e.g. --node 1:Input --node 2:Output:1)",
        ));
    }
    validate_node_fields(nodes)?;
    validate_single_endpoints(nodes)?;

    let ids: HashSet<u64> = nodes.iter().map(|n| n.id).collect();
    for node in nodes {
        validate_consume(node, &ids)?;
    }
    if has_cycle(nodes) {
        return Err(violation(
            "Pipeline nodes form a cycle",
            "The SDK walks the graph from the Input node and never finishes a loop",
            "Break the loop by changing the 'consume' list of one node in it",
        ));
    }
    Ok(())
}
