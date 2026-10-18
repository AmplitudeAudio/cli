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

//! Tests for the pipeline service and the SDK-mirroring validation rules.

mod common;

use am::assets::generated::PipelineNodeDefinition;
use am::assets::{Asset, Pipeline, ProjectContext};
use am::services::pipeline::{
    PipelinePatch, PipelineValues, apply_patch, build_pipeline, next_node_id, parse_node_spec,
};
use common::fixtures::AssetTestFixture;

fn node(id: u64, name: &str, consume: &[u32]) -> PipelineNodeDefinition {
    PipelineNodeDefinition {
        id,
        name: Some(name.to_string()),
        consume: Some(consume.to_vec()),
        parameters: None,
    }
}

fn chain() -> Vec<PipelineNodeDefinition> {
    vec![
        node(1, "Input", &[]),
        node(2, "Reverb", &[1]),
        node(3, "Output", &[2]),
    ]
}

fn pipeline(nodes: Vec<PipelineNodeDefinition>) -> Pipeline {
    Pipeline::builder(7, "main").nodes(nodes).build()
}

fn rules(nodes: Vec<PipelineNodeDefinition>) -> Result<(), am::assets::ValidationError> {
    pipeline(nodes).validate_rules(&ProjectContext::new(std::path::PathBuf::new()))
}

fn assert_rejected(nodes: Vec<PipelineNodeDefinition>, needle: &str) {
    let err = rules(nodes).expect_err("should be rejected");
    let text = format!("{err} {} {}", err.what(), err.why());
    assert!(text.contains(needle), "expected '{needle}' in: {text}");
}

fn context(fx: &AssetTestFixture) -> ProjectContext {
    let validator = fx.create_project_validator().unwrap();
    ProjectContext::new(fx.project_root().to_path_buf()).with_validator(validator)
}

// ---------------------------------------------------------------------------
// Validation rules (each mirrors something the SDK fails on)
// ---------------------------------------------------------------------------

#[test]
fn test_valid_chain_passes() {
    rules(chain()).unwrap();
}

#[test]
fn test_mixer_takes_many_inputs() {
    rules(vec![
        node(1, "Input", &[]),
        node(2, "Clamp", &[1]),
        node(3, "HardClip", &[1]),
        node(4, "Mixer", &[2, 3]),
        node(5, "Output", &[4]),
    ])
    .unwrap();
}

#[test]
fn test_missing_nodes_is_rejected() {
    let err = Pipeline::builder(7, "main")
        .build()
        .validate_rules(&ProjectContext::new(std::path::PathBuf::new()))
        .unwrap_err();
    assert!(err.to_string().contains("nodes"), "{err}");
    assert_rejected(Vec::new(), "nodes");
}

#[test]
fn test_node_without_a_name_is_rejected() {
    let mut nodes = chain();
    nodes[1].name = None;
    assert_rejected(nodes, "name");
    let mut nodes = chain();
    nodes[1].name = Some(String::new());
    assert_rejected(nodes, "name");
}

#[test]
fn test_input_and_output_are_required_exactly_once() {
    assert_rejected(
        vec![node(2, "Reverb", &[1]), node(3, "Output", &[2])],
        "Input",
    );
    assert_rejected(
        vec![node(1, "Input", &[]), node(2, "Reverb", &[1])],
        "Output",
    );
    assert_rejected(
        vec![
            node(1, "Input", &[]),
            node(2, "Input", &[]),
            node(3, "Output", &[1]),
        ],
        "more than one",
    );
    assert_rejected(
        vec![
            node(1, "Input", &[]),
            node(2, "Output", &[1]),
            node(3, "Output", &[1]),
        ],
        "more than one",
    );
}

#[test]
fn test_duplicate_node_ids_are_rejected() {
    let mut nodes = chain();
    nodes[2].id = 2;
    assert_rejected(nodes, "unique");
}

#[test]
fn test_node_ids_must_fit_a_consume_entry() {
    let mut nodes = chain();
    nodes[1].id = u64::from(u32::MAX) + 1;
    nodes[2].consume = Some(vec![]);
    assert_rejected(nodes, "4294967295");
}

#[test]
fn test_consume_must_reference_existing_nodes() {
    let mut nodes = chain();
    nodes[2].consume = Some(vec![99]);
    assert_rejected(nodes, "99");
}

#[test]
fn test_a_node_cannot_consume_itself() {
    let mut nodes = chain();
    nodes[1].consume = Some(vec![2]);
    assert_rejected(nodes, "itself");
}

#[test]
fn test_cycles_are_rejected() {
    assert_rejected(
        vec![
            node(1, "Input", &[]),
            node(2, "Clamp", &[3]),
            node(3, "HardClip", &[2]),
            node(4, "Output", &[3]),
        ],
        "cycle",
    );
}

#[test]
fn test_input_count_follows_the_node_type() {
    // Input produces, so it takes none.
    let mut nodes = chain();
    nodes[0].consume = Some(vec![3]);
    assert_rejected(nodes, "Input");
    // A single-input node takes exactly one.
    let mut nodes = chain();
    nodes[1].consume = Some(vec![]);
    assert_rejected(nodes, "Reverb");
    let mut nodes = vec![
        node(1, "Input", &[]),
        node(2, "Clamp", &[1]),
        node(3, "Reverb", &[1, 2]),
        node(4, "Output", &[3]),
    ];
    assert_rejected(nodes.clone(), "Reverb");
    // A mixer takes at least one.
    nodes[2] = node(3, "Mixer", &[]);
    assert_rejected(nodes, "Mixer");
}

#[test]
fn test_consuming_node_without_a_consume_list_is_rejected() {
    // The SDK reads the list's size without a null check.
    let mut nodes = chain();
    nodes[1].consume = None;
    assert_rejected(nodes, "consume");
}

#[test]
fn test_parameters_must_be_finite() {
    let mut nodes = chain();
    nodes[1].parameters = Some(vec![0.5, f32::NAN]);
    assert_rejected(nodes, "parameters");
}

#[test]
fn test_unknown_node_types_pass_but_are_reported() {
    let mut nodes = chain();
    nodes[1] = node(2, "MyPluginNode", &[1]);
    rules(nodes.clone()).unwrap();
    assert_eq!(pipeline(nodes).unknown_node_types(), vec!["MyPluginNode"]);
    assert!(pipeline(chain()).unknown_node_types().is_empty());
}

#[test]
fn test_all_sdk_builtin_node_types_are_known() {
    for name in [
        "Input",
        "Output",
        "Mixer",
        "StereoMixer",
        "AmbisonicMixer",
        "SpeakerPanning",
        "StereoPanning",
        "Reverb",
        "Freeverb",
        "Dattorro",
        "RoomReverb",
        "Attenuation",
        "Clamp",
        "HardClip",
        "RoundoffClip",
        "Limiter",
        "EnvironmentEffect",
        "NearFieldEffect",
        "Obstruction",
        "Occlusion",
        "Reflections",
        "AmbisonicBinauralDecoder",
        "AmbisonicPanning",
        "AmbisonicRotator",
        "AmbisonicSpeakerDecoder",
    ] {
        let nodes = vec![node(1, name, &[])];
        assert!(
            pipeline(nodes).unknown_node_types().is_empty(),
            "{name} should be a built-in node"
        );
    }
}

// ---------------------------------------------------------------------------
// Node spec syntax
// ---------------------------------------------------------------------------

#[test]
fn test_parse_node_spec_variants() {
    let n = parse_node_spec("--node", "1:Input").unwrap();
    assert_eq!((n.id, n.name.as_deref()), (1, Some("Input")));
    assert_eq!(n.consume, Some(vec![]));
    assert_eq!(n.parameters, None);

    let n = parse_node_spec("--node", "2:Reverb:1").unwrap();
    assert_eq!(n.consume, Some(vec![1]));

    let n = parse_node_spec("--node", "4:Mixer:2,3:0.5,1").unwrap();
    assert_eq!(n.consume, Some(vec![2, 3]));
    assert_eq!(n.parameters, Some(vec![0.5, 1.0]));

    // An empty consume list still lets parameters follow.
    let n = parse_node_spec("--node", "5:Reverb::0.25").unwrap();
    assert_eq!(n.consume, Some(vec![]));
    assert_eq!(n.parameters, Some(vec![0.25]));
}

#[test]
fn test_parse_node_spec_errors_name_the_flag() {
    for bad in [
        "",
        "Input",
        "x:Input",
        "1:",
        "1:Input:a",
        "1:Input:1:z",
        "1:A:1:2:3",
    ] {
        let err = parse_node_spec("--add-node", bad).unwrap_err();
        assert!(format!("{err:#}").contains("--add-node"), "{bad}: {err:#}");
    }
}

// ---------------------------------------------------------------------------
// Build / patch
// ---------------------------------------------------------------------------

#[test]
fn test_build_generates_a_unique_nonzero_id() {
    let fx = AssetTestFixture::new("pipe_build").unwrap();
    let built = build_pipeline(
        PipelineValues {
            name: "main".to_string(),
            nodes: chain(),
        },
        &context(&fx),
    )
    .unwrap();
    assert_ne!(built.id, 0);
    assert_eq!(built.name.as_deref(), Some("main"));
    assert_eq!(built.nodes.unwrap().len(), 3);
}

#[test]
fn test_build_rejects_an_invalid_graph() {
    let fx = AssetTestFixture::new("pipe_build_bad").unwrap();
    let result = build_pipeline(
        PipelineValues {
            name: "main".to_string(),
            nodes: vec![node(1, "Input", &[])],
        },
        &context(&fx),
    );
    assert!(result.is_err());
}

#[test]
fn test_next_node_id_follows_the_highest() {
    assert_eq!(next_node_id(&[]), 1);
    assert_eq!(next_node_id(&chain()), 4);
}

#[test]
fn test_patch_add_and_remove_leave_the_original_untouched() {
    let existing = pipeline(chain());
    let patched = apply_patch(
        &existing,
        &PipelinePatch {
            add: vec![node(4, "Clamp", &[2])],
            ..PipelinePatch::default()
        },
    )
    .unwrap();
    assert_eq!(patched.nodes.as_ref().unwrap().len(), 4);
    assert_eq!(existing.nodes.as_ref().unwrap().len(), 3);

    let patched = apply_patch(
        &patched,
        &PipelinePatch {
            remove: vec![4],
            ..PipelinePatch::default()
        },
    )
    .unwrap();
    assert_eq!(patched.nodes.unwrap().len(), 3);
}

#[test]
fn test_patch_remove_of_a_consumed_node_names_the_consumers() {
    let err = apply_patch(
        &pipeline(chain()),
        &PipelinePatch {
            remove: vec![2],
            ..PipelinePatch::default()
        },
    )
    .unwrap_err();
    let text = format!("{err} {} {}", err.what(), err.why());
    assert!(text.contains("Output"), "{text}");
    assert!(text.contains('3'), "{text}");
}

#[test]
fn test_patch_remove_together_with_its_consumers_is_fine() {
    let patched = apply_patch(
        &pipeline(chain()),
        &PipelinePatch {
            remove: vec![2, 3],
            ..PipelinePatch::default()
        },
    )
    .unwrap();
    assert_eq!(patched.nodes.unwrap().len(), 1);
}

#[test]
fn test_patch_remove_unknown_id_errors() {
    let err = apply_patch(
        &pipeline(chain()),
        &PipelinePatch {
            remove: vec![42],
            ..PipelinePatch::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("42"), "{err}");
}

#[test]
fn test_patch_replace_swaps_the_whole_list() {
    let patched = apply_patch(
        &pipeline(chain()),
        &PipelinePatch {
            replace: Some(vec![node(1, "Input", &[]), node(2, "Output", &[1])]),
            ..PipelinePatch::default()
        },
    )
    .unwrap();
    assert_eq!(patched.nodes.unwrap().len(), 2);
}

#[test]
fn test_patch_add_with_a_taken_id_errors() {
    let err = apply_patch(
        &pipeline(chain()),
        &PipelinePatch {
            add: vec![node(2, "Clamp", &[1])],
            ..PipelinePatch::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains('2'), "{err}");
}

#[test]
fn test_empty_patch_is_empty() {
    assert!(PipelinePatch::default().is_empty());
    assert!(
        !PipelinePatch {
            remove: vec![1],
            ..PipelinePatch::default()
        }
        .is_empty()
    );
}
