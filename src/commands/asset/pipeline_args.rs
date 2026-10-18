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

//! Command-line flags of `pipeline create` / `pipeline update` and their
//! resolution into nodes.
//!
//! A node has the fields of `PipelineNodeDefinition`: `id`, `name` (the node
//! type), `consume` (ids of the nodes it reads from) and `parameters`. The
//! compact flag syntax is `ID:Type[:CONSUMED_IDS[:PARAMETERS]]`.

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;

use crate::assets::generated::PipelineNodeDefinition;
use crate::common::errors::{CliError, codes};
use crate::services::pipeline::{PipelinePatch, parse_node_spec};

const NODE_HELP: &str = "A pipeline node as ID:Type[:CONSUMED_IDS[:PARAMETERS]] (repeatable).
ID is the node's unique number, Type the node type (e.g. Input, Reverb, Mixer, Output),
CONSUMED_IDS the comma-separated ids of the nodes it reads from (none for Input, one for
most nodes, one or more for Mixer) and PARAMETERS comma-separated numbers.
Example: --node 1:Input --node 2:Reverb:1:0.5,0.25 --node 3:Output:2";

/// The ways of giving a whole node list; shared by `create` and `update`.
#[derive(Args, Debug, Default, Clone)]
pub struct NodeArgs {
    /// A pipeline node (repeatable); on update the list replaces all nodes
    #[arg(long, value_name = "ID:TYPE[:CONSUMED[:PARAMS]]", long_help = NODE_HELP, conflicts_with = "from_json")]
    pub node: Vec<String>,

    /// JSON file with the node list: an array of nodes, or an object with a "nodes" array
    #[arg(long, value_name = "FILE")]
    pub from_json: Option<PathBuf>,
}

/// Flags of `pipeline update`: a whole new list, or incremental changes.
#[derive(Args, Debug, Default, Clone)]
pub struct UpdateArgs {
    #[command(flatten)]
    pub nodes: NodeArgs,

    /// Add a node, same syntax as --node (repeatable)
    #[arg(long, value_name = "ID:TYPE[:CONSUMED[:PARAMS]]")]
    pub add_node: Vec<String>,

    /// Remove the node with this id (repeatable); refused while other nodes consume it
    #[arg(long, value_name = "ID")]
    pub remove_node: Vec<u64>,
}

/// The error for a non-interactive create that was given no nodes.
pub fn missing_nodes_error() -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FIELD,
        "Missing required --node",
        "The pipeline's nodes are required and cannot be prompted for in non-interactive or JSON mode",
    )
    .with_suggestion(
        "Pass --node ID:Type[:CONSUMED_IDS[:PARAMETERS]] for each node, or --from-json <file> (see 'am asset pipeline create --help')",
    )
}

fn from_json_error(path: &Path, why: String) -> CliError {
    CliError::new(
        codes::ERR_VALIDATION_FORMAT,
        format!("Cannot use --from-json file {}", path.display()),
        why,
    )
    .with_suggestion(
        "Pass a JSON array of nodes, e.g. [{\"id\": 1, \"name\": \"Input\", \"consume\": []}]",
    )
}

/// Reads the node list of a `--from-json` file (array, or object with `nodes`).
pub fn read_nodes_file(path: &Path) -> Result<Vec<PipelineNodeDefinition>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| from_json_error(path, format!("The file cannot be read: {e}")))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| from_json_error(path, format!("The file is not valid JSON: {e}")))?;
    let list = match value {
        serde_json::Value::Array(_) => value,
        serde_json::Value::Object(mut fields) => fields.remove("nodes").ok_or_else(|| {
            from_json_error(path, "The object has no \"nodes\" array".to_string())
        })?,
        _ => {
            return Err(from_json_error(
                path,
                "Expected an array of nodes or an object with a \"nodes\" array".to_string(),
            )
            .into());
        }
    };
    serde_json::from_value(list)
        .map_err(|e| from_json_error(path, format!("The nodes are malformed: {e}")).into())
}

fn parse_specs(flag: &str, specs: &[String]) -> Result<Vec<PipelineNodeDefinition>> {
    specs
        .iter()
        .map(|spec| parse_node_spec(flag, spec).map_err(Into::into))
        .collect()
}

/// The whole node list the flags give, if any.
pub fn nodes_from_flags(args: &NodeArgs) -> Result<Option<Vec<PipelineNodeDefinition>>> {
    if let Some(path) = &args.from_json {
        return read_nodes_file(path).map(Some);
    }
    if args.node.is_empty() {
        return Ok(None);
    }
    parse_specs("--node", &args.node).map(Some)
}

/// Turns the `update` flags into a patch.
pub fn resolve_patch(args: &UpdateArgs) -> Result<PipelinePatch> {
    Ok(PipelinePatch {
        replace: nodes_from_flags(&args.nodes)?,
        add: parse_specs("--add-node", &args.add_node)?,
        remove: args.remove_node.clone(),
    })
}
