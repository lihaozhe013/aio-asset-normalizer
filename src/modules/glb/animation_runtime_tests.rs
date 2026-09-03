use super::*;
use std::borrow::Cow;

fn meshless_hierarchy_glb(
    include_skin: bool,
    animation_interpolation: Option<&str>,
) -> Vec<u8> {
    let mut bin = Vec::new();
    if animation_interpolation.is_some() {
        for value in [0.0_f32, 1.0] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
        let half_turn = std::f32::consts::FRAC_1_SQRT_2;
        for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, half_turn, half_turn] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut json = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [
            {"name": "Root", "children": [1]},
            {"name": "Spine", "translation": [0.0, 1.0, 0.0], "children": [2]},
            {"name": "Hand", "translation": [0.0, 1.0, 0.0]},
            {"name": "OutsideScene"}
        ]
    });
    if include_skin {
        json["skins"] = serde_json::json!([{"name": "Rig", "joints": [2]}]);
    }
    if animation_interpolation.is_some() {
        json["buffers"] = serde_json::json!([{"byteLength": bin.len()}]);
        json["bufferViews"] = serde_json::json!([
            {"buffer": 0, "byteOffset": 0, "byteLength": 8},
            {"buffer": 0, "byteOffset": 8, "byteLength": 32}
        ]);
        json["accessors"] = serde_json::json!([
            {"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0.0], "max": [1.0]},
            {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC4"}
        ]);
        json["animations"] = serde_json::json!([{
            "name": "Wave",
            "samplers": [{
                "input": 0,
                "output": 1,
                "interpolation": animation_interpolation.unwrap()
            }],
            "channels": [{"sampler": 0, "target": {"node": 1, "path": "rotation"}}]
        }]);
    }
    let mut json_bytes = serde_json::to_vec(&json).unwrap();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(json_bytes),
        bin: (!bin.is_empty()).then_some(Cow::Owned(bin)),
    }
    .to_vec()
    .unwrap()
}

fn static_skinned_glb(root_scale: [f32; 3]) -> Vec<u8> {
    let mut bin = Vec::new();
    for value in [
        [0.0_f32, 0.0, 0.0],
        [1.0_f32, 0.0, 0.0],
        [0.0_f32, 1.0, 0.0],
    ]
    .into_iter()
    .flatten()
    {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for _ in 0..3 {
        bin.extend_from_slice(&[0, 0, 0, 0]);
    }
    for _ in 0..3 {
        for value in [1.0_f32, 0.0, 0.0, 0.0] {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    for value in [
        0.5_f32, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -1.5,
        -1.0, 0.0, 1.0,
    ] {
        bin.extend_from_slice(&value.to_le_bytes());
    }

    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [
            {"name": "Root", "translation": [3.0, 0.0, 0.0], "scale": root_scale, "children": [1, 2]},
            {"name": "Mesh", "mesh": 0, "skin": 0},
            {"name": "Joint", "translation": [0.0, 1.0, 0.0]}
        ],
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0, "JOINTS_0": 1, "WEIGHTS_0": 2},
            "mode": 4
        }]}],
        "skins": [{"joints": [2], "inverseBindMatrices": 3}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 12},
            {"buffer": 0, "byteOffset": 48, "byteLength": 48},
            {"buffer": 0, "byteOffset": 96, "byteLength": 64}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
            {"bufferView": 1, "componentType": 5121, "count": 3, "type": "VEC4"},
            {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4"},
            {"bufferView": 3, "componentType": 5126, "count": 1, "type": "MAT4"}
        ]
    });
    let mut json_bytes = serde_json::to_vec(&json).unwrap();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Owned(bin)),
    }
    .to_vec()
    .unwrap()
}

#[test]
fn meshless_runtime_uses_first_scene_nodes_and_samples_animation() {
    let bytes = meshless_hierarchy_glb(false, Some("LINEAR"));
    let runtime = AnimationRuntime::from_bytes(&bytes, None).unwrap();

    assert!(runtime.primitives.is_empty());
    assert!(!runtime.preview_uses_skin());
    assert_eq!(runtime.preview_skeleton_nodes(), vec![0, 1, 2]);

    let rest = runtime.rest_node_poses().unwrap();
    assert_eq!(rest[0].world_translation, [0.0, 0.0, 0.0]);
    assert_eq!(rest[1].world_translation, [0.0, 1.0, 0.0]);
    assert_eq!(rest[2].world_translation, [0.0, 2.0, 0.0]);

    let pose = runtime.sample(0, 1.0).unwrap();
    assert_eq!(pose.node_poses.len(), 4);
    assert!((pose.node_poses[2].world_translation[0] + 1.0).abs() < 1e-5);
    assert!((pose.node_poses[2].world_translation[1] - 1.0).abs() < 1e-5);
}

#[test]
fn meshless_runtime_prefers_skin_joints_and_includes_ancestors() {
    let bytes = meshless_hierarchy_glb(true, None);
    let runtime = AnimationRuntime::from_bytes(&bytes, None).unwrap();

    assert!(runtime.preview_uses_skin());
    assert_eq!(runtime.preview_skeleton_nodes(), vec![0, 1, 2]);
}

#[test]
fn static_meshless_runtime_exposes_rest_pose_without_animation() {
    let bytes = meshless_hierarchy_glb(false, None);
    let runtime = AnimationRuntime::from_bytes(&bytes, None).unwrap();

    assert!(runtime.clips.is_empty());
    assert_eq!(runtime.rest_node_poses().unwrap().len(), 4);
    assert_eq!(runtime.preview_skeleton_nodes(), vec![0, 1, 2]);
}

#[test]
fn static_skinned_runtime_samples_rest_pose_in_mesh_local_space() {
    let bytes = static_skinned_glb([2.0, 2.0, 2.0]);
    let runtime = AnimationRuntime::from_bytes(&bytes, None).unwrap();

    assert!(runtime.clips.is_empty());
    let pose = runtime.sample_rest().unwrap();
    let positions = pose.skinned_positions[0].as_ref().unwrap();
    let expected_local = [
        [-1.5_f32, 0.0, 0.0],
        [-1.0_f32, 0.0, 0.0],
        [-1.5_f32, 0.5, 0.0],
    ];
    for (actual, expected) in positions.iter().zip(expected_local) {
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-5);
        }
    }

    let mesh_world = pose.node_world[1];
    let expected_world = [
        [0.0_f32, 0.0, 0.0],
        [1.0_f32, 0.0, 0.0],
        [0.0_f32, 1.0, 0.0],
    ];
    for (position, expected) in positions.iter().zip(expected_world) {
        let world = transform_point(mesh_world, *position);
        for (actual, expected) in world.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-5);
        }
    }
}

#[test]
fn static_skinned_runtime_rejects_non_invertible_mesh_transform() {
    let bytes = static_skinned_glb([0.0, 2.0, 2.0]);
    let runtime = AnimationRuntime::from_bytes(&bytes, None).unwrap();

    assert!(matches!(
        runtime.sample_rest(),
        Err(RuntimeError::Invalid(message))
            if message.contains("Mesh node transform is non-invertible")
    ));
}

#[test]
fn affine_inverse_handles_rotation_scale_and_translation() {
    let matrix =
        compose([3.0, -2.0, 1.0], [0.3, -0.2, 0.4, 0.8], [2.0, 3.0, 4.0]);
    let inverse = invert_affine(matrix).unwrap();
    let product = multiply(matrix, inverse);
    for (index, value) in product.iter().enumerate() {
        let expected = if index % 5 == 0 { 1.0 } else { 0.0 };
        assert!((value - expected).abs() < 1e-5);
    }
}

#[test]
fn unsupported_meshless_animation_keeps_rest_pose_available() {
    let bytes = meshless_hierarchy_glb(false, Some("CUBICSPLINE"));
    let runtime = AnimationRuntime::from_bytes(&bytes, None).unwrap();

    assert!(!runtime.clips[0].is_playable());
    assert!(runtime.rest_node_poses().is_ok());
    assert!(runtime.sample_rest().is_ok());
    assert!(matches!(
        runtime.sample_nodes(0, 0.0),
        Err(RuntimeError::Unsupported(_))
    ));
}

#[test]
fn linear_and_step_curves_sample_expected_values() {
    let linear = AnimationCurve {
        path: AnimationPath::Translation,
        times: vec![0.0, 1.0],
        values: vec![[0.0, 0.0, 0.0, 0.0], [2.0, 4.0, 6.0, 0.0]],
        interpolation: Interpolation::Linear,
    };
    assert_eq!(linear.sample(0.5), [1.0, 2.0, 3.0, 0.0]);

    let step = AnimationCurve {
        interpolation: Interpolation::Step,
        ..linear.clone()
    };
    assert_eq!(step.sample(0.999), [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(step.sample(1.0), [2.0, 4.0, 6.0, 0.0]);
}

#[test]
fn quaternion_linear_sampling_uses_shortest_slerp() {
    let curve = AnimationCurve {
        path: AnimationPath::Rotation,
        times: vec![0.0, 1.0],
        values: vec![[0.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 0.0]],
        interpolation: Interpolation::Linear,
    };
    let value = curve.sample(0.5);
    assert!((value[1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
    assert!((value[3] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
}

#[test]
fn cpu_skinning_applies_joint_matrix_and_reports_zero_weight_vertices() {
    let mut joint = identity();
    joint[12] = 1.0;
    joint[13] = 2.0;
    joint[14] = 3.0;
    let (position, total) = blend_point(
        [0.0, 0.0, 0.0],
        [0, 0, 0, 0],
        [1.0, 0.0, 0.0, 0.0],
        &[joint],
    )
    .unwrap();
    assert_eq!(position, [1.0, 2.0, 3.0]);
    assert_eq!(total, 1.0);

    let (fallback, total) = blend_point(
        [4.0, 5.0, 6.0],
        [0, 0, 0, 0],
        [0.0, 0.0, 0.0, 0.0],
        &[joint],
    )
    .unwrap();
    assert_eq!(fallback, [0.0, 0.0, 0.0]);
    assert_eq!(total, 0.0);
}

#[test]
fn runtime_loads_and_samples_a_translation_clip() {
    let mut bin = Vec::new();
    for value in [0.0_f32, 0.0, 0.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for value in [0.0_f32, 1.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"name": "Root", "mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 12},
            {"buffer": 0, "byteOffset": 12, "byteLength": 8},
            {"buffer": 0, "byteOffset": 20, "byteLength": 24}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 1, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [0.0, 0.0, 0.0]},
            {"bufferView": 1, "componentType": 5126, "count": 2, "type": "SCALAR"},
            {"bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC3"}
        ],
        "animations": [{
            "name": "Move",
            "samplers": [{"input": 1, "output": 2, "interpolation": "LINEAR"}],
            "channels": [{"sampler": 0, "target": {"node": 0, "path": "translation"}}]
        }]
    });
    let mut json_bytes = serde_json::to_vec(&json).unwrap();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let bytes = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Owned(bin)),
    }
    .to_vec()
    .unwrap();
    let path = std::env::temp_dir().join(format!(
        "aio-asset-normalizer-animation-runtime-{}.glb",
        std::process::id()
    ));
    std::fs::write(&path, bytes).unwrap();
    let runtime = AnimationRuntime::load(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(runtime.primitives.len(), 1);
    assert_eq!(runtime.clips.len(), 1);
    assert_eq!(runtime.clips[0].duration, 1.0);
    let pose = runtime.sample(0, 0.5).unwrap();
    assert!((pose.node_world[0][12] - 0.5).abs() < 1e-5);
    let final_pose = runtime.sample(0, 2.0).unwrap();
    assert!((final_pose.node_world[0][12] - 1.0).abs() < 1e-5);
}

#[test]
fn runtime_reads_skin_attributes_and_deforms_a_vertex() {
    let mut bin = Vec::new();
    for value in [[1.0_f32, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 0.0]] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    bin.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    for _ in 0..3 {
        bin.extend_from_slice(
            &[1.0_f32, 0.0, 0.0, 0.0]
                .map(|value| value.to_le_bytes())
                .concat(),
        );
    }
    for value in [
        1.0_f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0, 1.0,
    ] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for value in [0.0_f32, 1.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for value in [
        0.0_f32,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    ] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0, 1]}],
        "nodes": [
            {"name": "Mesh", "mesh": 0, "skin": 0},
            {"name": "Joint"}
        ],
        "meshes": [{"primitives": [{
            "attributes": {"POSITION": 0, "JOINTS_0": 1, "WEIGHTS_0": 2}
        }]}],
        "skins": [{"joints": [1], "inverseBindMatrices": 3}],
        "buffers": [{"byteLength": bin.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36},
            {"buffer": 0, "byteOffset": 36, "byteLength": 12},
            {"buffer": 0, "byteOffset": 48, "byteLength": 48},
            {"buffer": 0, "byteOffset": 96, "byteLength": 64},
            {"buffer": 0, "byteOffset": 160, "byteLength": 8},
            {"buffer": 0, "byteOffset": 168, "byteLength": 32}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
            {"bufferView": 1, "componentType": 5121, "count": 3, "type": "VEC4"},
            {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4"},
            {"bufferView": 3, "componentType": 5126, "count": 1, "type": "MAT4"},
            {"bufferView": 4, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0.0], "max": [1.0]},
            {"bufferView": 5, "componentType": 5126, "count": 2, "type": "VEC4"}
        ],
        "animations": [{
            "name": "Turn",
            "samplers": [{"input": 4, "output": 5, "interpolation": "LINEAR"}],
            "channels": [{"sampler": 0, "target": {"node": 1, "path": "rotation"}}]
        }]
    });
    let mut json_bytes = serde_json::to_vec(&json).unwrap();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    let bytes = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Owned(bin)),
    }
    .to_vec()
    .unwrap();
    let path = std::env::temp_dir().join(format!(
        "aio-asset-normalizer-skin-runtime-{}.glb",
        std::process::id()
    ));
    std::fs::write(&path, bytes).unwrap();
    let runtime = AnimationRuntime::load(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    let pose = runtime.sample(0, 1.0).unwrap();
    let position = pose.skinned_positions[0].as_ref().unwrap()[0];
    assert!(position[0].abs() < 1e-5);
    assert!((position[1] - 1.0).abs() < 1e-5);
}
