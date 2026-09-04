use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use super::super::{
    AnimationOutputMode, AnimationRuntime, GlbDocument, GlbExportPreset,
    GlbExportSelection,
};

fn root_motion_document() -> GlbDocument {
    let mut bin = Vec::new();
    for value in [
        [0.0_f32, 0.0, 0.0],
        [1.0_f32, 0.0, 0.0],
        [0.0_f32, 1.0, 0.0],
    ] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    for time in [0.0_f32, 1.0, 2.0] {
        bin.extend_from_slice(&time.to_le_bytes());
    }
    for value in [[2.0_f32, 3.0, 4.0], [5.0, 3.0, 4.0], [8.0, 3.0, 4.0]] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    for value in [[1.0_f32, 1.0, 1.0], [1.0, 2.0, 1.0], [1.0, 3.0, 1.0]] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    for _ in 0..2 {
        for component in [
            1.0_f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ] {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    let byte_length = bin.len();
    GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"name": "Character", "nodes": [0]}],
            "nodes": [
                {"name": "Scene Root", "children": [1, 2]},
                {"name": "Character", "children": [3], "mesh": 0, "skin": 0},
                {"name": "Other"},
                {"name": "Hip", "children": [4]},
                {"name": "Knee"}
            ],
            "meshes": [{"name": "Body", "primitives": [{"attributes": {"POSITION": 0}}]}],
            "skins": [{"name": "Character Skin", "skeleton": 3, "joints": [3, 4], "inverseBindMatrices": 4}],
            "buffers": [{"byteLength": byte_length}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 12},
                {"buffer": 0, "byteOffset": 48, "byteLength": 36},
                {"buffer": 0, "byteOffset": 84, "byteLength": 36},
                {"buffer": 0, "byteOffset": 120, "byteLength": 128}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "SCALAR", "min": [0.0], "max": [2.0]},
                {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 3, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4"}
            ],
            "animations": [
                {
                    "name": "Walk",
                    "samplers": [
                        {"input": 1, "output": 2, "interpolation": "LINEAR"},
                        {"input": 1, "output": 3, "interpolation": "STEP"}
                    ],
                    "channels": [
                        {"sampler": 0, "target": {"node": 3, "path": "translation"}},
                        {"sampler": 1, "target": {"node": 4, "path": "scale"}}
                    ]
                },
                {
                    "name": "Other",
                    "samplers": [{"input": 1, "output": 2, "interpolation": "LINEAR"}],
                    "channels": [{"sampler": 0, "target": {"node": 2, "path": "translation"}}]
                }
            ]
        }),
        bin: Some(bin),
        dirty: false,
    }
}

fn skeleton_selection(animations: &[usize]) -> GlbExportSelection {
    GlbExportSelection {
        preset: GlbExportPreset::SkeletonAnimation,
        scene_index: 0,
        skin_index: Some(0),
        selected_nodes: BTreeSet::new(),
        selected_primitives: BTreeMap::new(),
        selected_animations: animations.iter().copied().collect(),
        animation_output: AnimationOutputMode::Combined,
        remove_root_motion: true,
        root_motion_node_override: None,
    }
}

fn character_selection(
    selected_nodes: &[usize],
    animations: &[usize],
    skin_index: Option<usize>,
) -> GlbExportSelection {
    GlbExportSelection {
        preset: GlbExportPreset::CharacterPackage,
        scene_index: 0,
        skin_index,
        selected_nodes: selected_nodes.iter().copied().collect(),
        selected_primitives: BTreeMap::new(),
        selected_animations: animations.iter().copied().collect(),
        animation_output: AnimationOutputMode::Combined,
        remove_root_motion: true,
        root_motion_node_override: None,
    }
}

fn output_values(
    document: &GlbDocument,
    animation_index: usize,
    channel_index: usize,
) -> Vec<Vec<f32>> {
    let animation = &document.json["animations"][animation_index];
    let sampler_index = animation["channels"][channel_index]["sampler"]
        .as_u64()
        .unwrap() as usize;
    let output_index = animation["samplers"][sampler_index]["output"]
        .as_u64()
        .unwrap() as usize;
    let accessor = document.accessor(output_index).unwrap();
    let count = accessor["count"].as_u64().unwrap() as usize;
    let bytes = document.accessor_bytes(accessor, 12).unwrap();
    (0..count)
        .map(|index| {
            (0..3)
                .map(|component| {
                    let offset = index * 12 + component * 4;
                    f32::from_le_bytes(
                        bytes[offset..offset + 4].try_into().unwrap(),
                    )
                })
                .collect()
        })
        .collect()
}

#[test]
fn freezes_linear_and_step_translation_at_the_first_keyframe() {
    for interpolation in ["LINEAR", "STEP"] {
        let source = root_motion_document();
        let original_root = output_values(&source, 0, 0);
        let original_scale = output_values(&source, 0, 1);
        let mut output = source.clone();
        output.json["animations"][0]["samplers"][0]["interpolation"] =
            json!(interpolation);

        let report =
            output.prune_for_export(&skeleton_selection(&[0])).unwrap();

        assert_eq!(report.root_motion_channels_modified, 1);
        assert_eq!(output_values(&output, 0, 0), vec![vec![2.0, 3.0, 4.0]; 3]);
        assert_eq!(output_values(&output, 0, 1), original_scale);
        assert_ne!(original_root, output_values(&output, 0, 0));
        let input_index = output.json["animations"][0]["samplers"][0]["input"]
            .as_u64()
            .unwrap() as usize;
        assert_eq!(
            output.read_accessor_f32(input_index).unwrap(),
            vec![vec![0.0], vec![1.0], vec![2.0]]
        );
        gltf::Gltf::from_slice(&output.to_bytes().unwrap()).unwrap();
        AnimationRuntime::from_bytes_skeleton_only(
            &output.to_bytes().unwrap(),
            None,
        )
        .unwrap();
    }
}

#[test]
fn only_selected_animation_is_modified_and_other_channels_remain_intact() {
    let source = root_motion_document();
    let untouched_animation = source.json["animations"][1].clone();
    let mut output = source.clone();

    let report = output.prune_for_export(&skeleton_selection(&[0])).unwrap();

    assert_eq!(report.root_motion_channels_modified, 1);
    assert_eq!(output.json["animations"].as_array().unwrap().len(), 1);
    assert_eq!(source.json["animations"][1], untouched_animation);
    assert_eq!(
        source.json["animations"][0]["channels"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn resolves_skin_skeleton_common_ancestor_and_manual_override() {
    let document = root_motion_document();
    let info = document
        .root_motion_info(&skeleton_selection(&[0]))
        .unwrap();
    assert_eq!(info.resolved_node, Some(3));
    assert_eq!(info.candidates, vec![3]);
    assert!(info.animations_without_track.is_empty());

    let mut common_ancestor_document = root_motion_document();
    common_ancestor_document.json["nodes"][1]
        .as_object_mut()
        .unwrap()
        .remove("skin");
    common_ancestor_document.json["animations"][0]["channels"][0]["target"]
        ["node"] = json!(0);
    let common_selection = character_selection(&[1, 2], &[0], None);
    let info = common_ancestor_document
        .root_motion_info(&common_selection)
        .unwrap();
    assert_eq!(info.resolved_node, Some(0));
    let mut common_output = common_ancestor_document;
    let report = common_output.prune_for_export(&common_selection).unwrap();
    assert_eq!(report.root_motion_channels_modified, 1);
    assert_eq!(
        output_values(&common_output, 0, 0),
        vec![vec![2.0, 3.0, 4.0]; 3]
    );

    let mut ambiguous = root_motion_document();
    ambiguous.json["nodes"][0]["children"] = json!([1]);
    ambiguous.json["scenes"][0]["nodes"] = json!([0, 2]);
    ambiguous.json["nodes"][1]
        .as_object_mut()
        .unwrap()
        .remove("skin");
    let ambiguous_selection = character_selection(&[0, 2], &[0], None);
    let validation = ambiguous.validate_export_selection(&ambiguous_selection);
    assert!(validation
        .errors
        .iter()
        .any(|error| error.contains("ambiguous")));

    let mut manual_selection = ambiguous_selection;
    manual_selection.root_motion_node_override = Some(3);
    let report = ambiguous.prune_for_export(&manual_selection).unwrap();
    assert_eq!(report.root_motion_channels_modified, 1);
}

#[test]
fn missing_translation_channel_is_a_warning_and_no_op() {
    let mut output = root_motion_document();
    let original = output.to_bytes().unwrap();
    let original_values = output_values(&output, 1, 0);
    let selection = skeleton_selection(&[1]);
    let validation = output.validate_export_selection(&selection);
    assert!(validation.is_valid());
    assert!(!validation.warnings.is_empty());

    let report = output.prune_for_export(&selection).unwrap();
    assert_eq!(report.root_motion_channels_modified, 0);
    assert_eq!(output_values(&output, 0, 0), original_values);
    assert_ne!(output.to_bytes().unwrap(), original);
}

#[test]
fn shared_sampler_is_copied_before_root_translation_is_rewritten() {
    let mut output = root_motion_document();
    output.json["animations"][0]["samplers"] = json!([
        {"input": 1, "output": 2, "interpolation": "LINEAR"}
    ]);
    output.json["animations"][0]["channels"][1]["sampler"] = json!(0);

    let report = output.prune_for_export(&skeleton_selection(&[0])).unwrap();

    assert_eq!(report.root_motion_channels_modified, 1);
    let root_sampler = output.json["animations"][0]["channels"][0]["sampler"]
        .as_u64()
        .unwrap();
    let scale_sampler = output.json["animations"][0]["channels"][1]["sampler"]
        .as_u64()
        .unwrap();
    assert_ne!(root_sampler, scale_sampler);
    assert_eq!(output_values(&output, 0, 0), vec![vec![2.0, 3.0, 4.0]; 3]);
    assert_eq!(
        output_values(&output, 0, 1),
        vec![
            vec![2.0, 3.0, 4.0],
            vec![5.0, 3.0, 4.0],
            vec![8.0, 3.0, 4.0]
        ]
    );
    gltf::Gltf::from_slice(&output.to_bytes().unwrap()).unwrap();
}

fn assert_rejected(mut document: GlbDocument) {
    let original_json = document.json.clone();
    let original_bin = document.bin.clone();
    assert!(document
        .prune_for_export(&skeleton_selection(&[0]))
        .is_err());
    assert_eq!(document.json, original_json);
    assert_eq!(document.bin, original_bin);
}

#[test]
fn unsupported_root_translation_accessors_are_rejected_without_mutation() {
    let mut non_float = root_motion_document();
    non_float.json["accessors"][2]["componentType"] = json!(5123);
    assert_rejected(non_float);

    let mut non_vec3 = root_motion_document();
    non_vec3.json["accessors"][2]["type"] = json!("VEC4");
    assert_rejected(non_vec3);

    let mut sparse = root_motion_document();
    sparse.json["accessors"][2]["sparse"] = json!({});
    assert_rejected(sparse);

    let mut interleaved = root_motion_document();
    interleaved.json["bufferViews"][2]["byteStride"] = json!(12);
    assert_rejected(interleaved);

    let mut damaged = root_motion_document();
    damaged.json["bufferViews"][2]["byteLength"] = json!(12);
    assert_rejected(damaged);

    let mut cubic = root_motion_document();
    cubic.json["animations"][0]["samplers"][0]["interpolation"] =
        json!("CUBICSPLINE");
    assert_rejected(cubic);
}

#[test]
fn preserve_all_ignores_root_motion_configuration() {
    let mut document = root_motion_document();
    let original = document.to_bytes().unwrap();
    let mut selection = skeleton_selection(&[0]);
    selection.preset = GlbExportPreset::PreserveAll;
    selection.root_motion_node_override = Some(3);

    let report = document.prune_for_export(&selection).unwrap();

    assert_eq!(report.root_motion_channels_modified, 0);
    assert_eq!(document.to_bytes().unwrap(), original);
}
