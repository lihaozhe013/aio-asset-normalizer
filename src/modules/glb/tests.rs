use super::*;

use std::collections::BTreeMap;

fn animation_rate_document() -> GlbDocument {
    let mut bin = Vec::new();
    for value in [0.0_f32, 1.0, 2.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for value in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    for value in [[1.0_f32, 1.0, 1.0]; 3] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }

    GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Root"}],
            "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 12},
                {"buffer": 0, "byteOffset": 12, "byteLength": 36},
                {"buffer": 0, "byteOffset": 48, "byteLength": 36}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "SCALAR", "min": [0.0], "max": [2.0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC3"}
            ],
            "animations": [
                {
                    "name": "Target",
                    "samplers": [
                        {"input": 0, "output": 1, "interpolation": "LINEAR"},
                        {"input": 0, "output": 2, "interpolation": "LINEAR"}
                    ],
                    "channels": [
                        {"sampler": 0, "target": {"node": 0, "path": "translation"}},
                        {"sampler": 1, "target": {"node": 0, "path": "scale"}}
                    ]
                },
                {
                    "name": "Untouched",
                    "samplers": [
                        {"input": 0, "output": 1, "interpolation": "LINEAR"}
                    ],
                    "channels": [
                        {"sampler": 0, "target": {"node": 0, "path": "translation"}}
                    ]
                }
            ]
        }),
        bin: Some(bin),
        dirty: false,
    }
}

#[test]
fn animation_rate_scales_shared_inputs_and_preserves_outputs() {
    let mut document = animation_rate_document();
    let original_outputs = document.bin.as_ref().unwrap()[12..].to_vec();

    document
        .apply(EditOperation::ScaleAnimationRate {
            animation: 0,
            rate: 2.0,
        })
        .unwrap();

    let first_input = document.json["animations"][0]["samplers"][0]["input"]
        .as_u64()
        .unwrap();
    let second_input = document.json["animations"][0]["samplers"][1]["input"]
        .as_u64()
        .unwrap();
    let untouched_input = document.json["animations"][1]["samplers"][0]
        ["input"]
        .as_u64()
        .unwrap();

    assert_eq!(first_input, second_input);
    assert_ne!(first_input, untouched_input);
    assert_eq!(untouched_input, 0);
    assert_eq!(
        document.read_accessor_f32(first_input as usize).unwrap(),
        vec![vec![0.0], vec![0.5], vec![1.0],]
    );
    assert_eq!(
        &document.bin.as_ref().unwrap()[12..12 + original_outputs.len()],
        original_outputs
    );

    let bytes = document.to_bytes().unwrap();
    gltf::Gltf::from_slice(&bytes).unwrap();

    let path = std::env::temp_dir().join(format!(
        "aio-asset-normalizer-animation-rate-{}.glb",
        std::process::id()
    ));
    std::fs::write(&path, bytes).unwrap();
    let runtime = AnimationRuntime::load(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(runtime.clips[0].duration, 1.0);
    assert_eq!(runtime.clips[1].duration, 2.0);
}

#[test]
fn animation_rate_slows_animation_without_changing_keyframes() {
    let mut document = animation_rate_document();
    let original_outputs = document.bin.as_ref().unwrap()[12..].to_vec();

    document
        .apply(EditOperation::ScaleAnimationRate {
            animation: 0,
            rate: 0.5,
        })
        .unwrap();

    let input = document.json["animations"][0]["samplers"][0]["input"]
        .as_u64()
        .unwrap();
    assert_eq!(
        document.read_accessor_f32(input as usize).unwrap(),
        vec![vec![0.0], vec![2.0], vec![4.0],]
    );
    assert_eq!(
        &document.bin.as_ref().unwrap()[12..12 + original_outputs.len()],
        original_outputs
    );
}

#[test]
fn animation_rate_rejects_invalid_values_and_keeps_one_x_unchanged() {
    for rate in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        let mut document = animation_rate_document();
        let original_json = document.json.clone();
        let original_bin = document.bin.clone();
        assert!(document
            .apply(EditOperation::ScaleAnimationRate { animation: 0, rate })
            .is_err());
        assert_eq!(document.json, original_json);
        assert_eq!(document.bin, original_bin);
    }

    let mut document = animation_rate_document();
    let original_json = document.json.clone();
    let original_bin = document.bin.clone();
    document
        .apply(EditOperation::ScaleAnimationRate {
            animation: 0,
            rate: 1.0,
        })
        .unwrap();
    assert_eq!(document.json, original_json);
    assert_eq!(document.bin, original_bin);
}

#[test]
fn matrix_multiplication_preserves_identity() {
    assert_eq!(multiply(identity(), identity()), identity());
}

#[test]
fn root_preview_matrix_composes_translation_scale_and_rotation() {
    let matrix = RootTransformPreview {
        euler_degrees: [0.0, 90.0, 0.0],
        scale: 2.0,
        translation: [3.0, 4.0, 5.0],
    }
    .to_matrix()
    .unwrap();

    assert!((matrix[0][3] - 3.0).abs() < 1e-5);
    assert!((matrix[1][3] - 4.0).abs() < 1e-5);
    assert!((matrix[2][3] - 5.0).abs() < 1e-5);
    assert!(matrix[0][0].abs() < 1e-5);
    assert!((matrix[0][2] - 2.0).abs() < 1e-5);
    assert!((matrix[2][0] + 2.0).abs() < 1e-5);
}

#[test]
fn euler_preview_rotates_each_axis_independently() {
    let x = RootTransformPreview {
        euler_degrees: [90.0, 0.0, 0.0],
        ..RootTransformPreview::default()
    }
    .to_matrix()
    .unwrap();
    assert!((x[1][2] + 1.0).abs() < 1e-5);
    assert!((x[2][1] - 1.0).abs() < 1e-5);

    let y = RootTransformPreview {
        euler_degrees: [0.0, 90.0, 0.0],
        ..RootTransformPreview::default()
    }
    .to_matrix()
    .unwrap();
    assert!((y[0][2] - 1.0).abs() < 1e-5);
    assert!((y[2][0] + 1.0).abs() < 1e-5);

    let z = RootTransformPreview {
        euler_degrees: [0.0, 0.0, 90.0],
        ..RootTransformPreview::default()
    }
    .to_matrix()
    .unwrap();
    assert!((z[0][1] + 1.0).abs() < 1e-5);
    assert!((z[1][0] - 1.0).abs() < 1e-5);
}

#[test]
fn euler_preview_uses_z_y_x_composition_order() {
    let matrix = RootTransformPreview {
        euler_degrees: [90.0, 90.0, 0.0],
        ..RootTransformPreview::default()
    }
    .to_matrix()
    .unwrap();

    assert!(matrix[0][0].abs() < 1e-5);
    assert!((matrix[0][1] - 1.0).abs() < 1e-5);
    assert!((matrix[1][2] + 1.0).abs() < 1e-5);
    assert!((matrix[2][0] + 1.0).abs() < 1e-5);
}

#[test]
fn root_preview_matrix_rejects_invalid_values() {
    let invalid_scale = RootTransformPreview {
        scale: 0.0,
        ..RootTransformPreview::default()
    };
    assert!(invalid_scale.to_matrix().is_err());

    let invalid_translation = RootTransformPreview {
        translation: [f32::NAN, 0.0, 0.0],
        ..RootTransformPreview::default()
    };
    assert!(invalid_translation.to_matrix().is_err());

    let invalid_euler = RootTransformPreview {
        euler_degrees: [f32::NAN, 0.0, 0.0],
        ..RootTransformPreview::default()
    };
    assert!(invalid_euler.to_matrix().is_err());
}

#[test]
fn rejects_non_glb_extension() {
    let error = GlbDocument::load(Path::new("model.fbx")).unwrap_err();
    assert!(error.to_string().contains("Only .glb"));
}

#[test]
fn root_rotation_writes_column_major_glb_matrix() {
    let mut document = GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"translation": [1.0, 0.0, 0.0]}]
        }),
        bin: None,
        dirty: false,
    };
    document
        .apply(EditOperation::RotateRoots {
            euler_degrees: [0.0, 90.0, 0.0],
        })
        .unwrap();
    let bytes = document.to_bytes().unwrap();
    let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let json: Value = serde_json::from_slice(&glb.json).unwrap();
    let matrix = json["nodes"][0]["matrix"].as_array().unwrap();
    assert!((matrix[12].as_f64().unwrap() - 0.0).abs() < 1e-5);
    assert!((matrix[14].as_f64().unwrap() + 1.0).abs() < 1e-5);
    gltf::Gltf::from_slice(&bytes).unwrap();
}

#[test]
fn root_rotation_updates_every_scene_root() {
    let mut document = GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0, 1]}],
            "nodes": [
                {"translation": [1.0, 0.0, 0.0]},
                {"translation": [0.0, 0.0, 1.0]}
            ]
        }),
        bin: None,
        dirty: false,
    };
    document
        .apply(EditOperation::RotateRoots {
            euler_degrees: [0.0, 90.0, 0.0],
        })
        .unwrap();

    let bytes = document.to_bytes().unwrap();
    let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let json: Value = serde_json::from_slice(&glb.json).unwrap();
    let first = json["nodes"][0]["matrix"].as_array().unwrap();
    let second = json["nodes"][1]["matrix"].as_array().unwrap();
    assert!((first[12].as_f64().unwrap() - 0.0).abs() < 1e-5);
    assert!((first[14].as_f64().unwrap() + 1.0).abs() < 1e-5);
    assert!((second[12].as_f64().unwrap() - 1.0).abs() < 1e-5);
    assert!((second[14].as_f64().unwrap() - 0.0).abs() < 1e-5);
    gltf::Gltf::from_slice(&bytes).unwrap();
}

#[test]
fn appended_animation_round_trips_through_gltf_validation() {
    let mut document = GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Root"}],
            "buffers": [{"byteLength": 0}],
            "bufferViews": [],
            "accessors": []
        }),
        bin: Some(Vec::new()),
        dirty: false,
    };
    document
        .append_animation(&AnimationClipData {
            name: "Test Clip".to_owned(),
            times: vec![0.0, 1.0],
            channels: vec![AnimationChannelData {
                node: 0,
                rotations: vec![[0.0, 0.0, 0.0, 1.0]; 2],
                translations: None,
            }],
        })
        .unwrap();
    let bytes = document.to_bytes().unwrap();
    gltf::Gltf::from_slice(&bytes).unwrap();
}

#[test]
fn appended_animation_converts_matrix_nodes_to_trs() {
    let mut document = GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{
                "name": "Root",
                "matrix": [
                    1.0, 0.0, 0.0, 0.0,
                    0.0, 1.0, 0.0, 0.0,
                    0.0, 0.0, 1.0, 0.0,
                    2.0, 3.0, 4.0, 1.0
                ]
            }],
            "buffers": [{"byteLength": 0}],
            "bufferViews": [],
            "accessors": []
        }),
        bin: Some(Vec::new()),
        dirty: false,
    };
    document
        .append_animation(&AnimationClipData {
            name: "Matrix Node Clip".to_owned(),
            times: vec![0.0, 1.0],
            channels: vec![AnimationChannelData {
                node: 0,
                rotations: vec![[0.0, 0.0, 0.0, 1.0]; 2],
                translations: None,
            }],
        })
        .unwrap();
    let node = &document.json["nodes"][0];
    assert!(node.get("matrix").is_none());
    assert_eq!(node["translation"], json!([2.0, 3.0, 4.0]));
    assert_eq!(node["rotation"], json!([0.0, 0.0, 0.0, 1.0]));
    assert_eq!(node["scale"], json!([1.0, 1.0, 1.0]));
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

#[test]
fn texture_replacement_embeds_png_and_duplicates_shared_material() {
    let mut document = GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [
                {"attributes": {"POSITION": 0}, "material": 0},
                {"attributes": {"POSITION": 0}, "material": 0}
            ]}],
            "materials": [{"name": "Shared", "pbrMetallicRoughness": {}}],
            "textures": [],
            "images": [],
            "buffers": [{"byteLength": 0}],
            "bufferViews": [{"buffer": 0, "byteLength": 36}],
            "accessors": [{
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 0.0]
            }]
        }),
        bin: Some(vec![0; 36]),
        dirty: false,
    };
    let path = std::env::temp_dir().join(format!(
        "aio-asset-normalizer-texture-{}.png",
        std::process::id()
    ));
    image::save_buffer(&path, &[255, 0, 0, 255], 1, 1, image::ColorType::Rgba8)
        .unwrap();

    document
        .replace_texture(
            PrimitiveTarget {
                mesh: 0,
                primitive: 0,
            },
            TextureSlot::BaseColor,
            &path,
            true,
        )
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(document.summary().materials, 2);
    assert_eq!(document.json["meshes"][0]["primitives"][0]["material"], 1);
    assert_eq!(document.json["meshes"][0]["primitives"][1]["material"], 0);
    assert_eq!(document.summary().images, 1);
    assert_eq!(document.summary().extensions, Vec::<String>::new());
    let bytes = document.to_bytes().unwrap();
    gltf::Gltf::from_slice(&bytes).unwrap();
}

#[test]
fn texture_replacement_rejects_non_png_or_jpeg() {
    let mut document = GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {}, "material": 0}]}],
            "materials": [{"pbrMetallicRoughness": {}}],
            "buffers": [{"byteLength": 0}],
            "bufferViews": [],
            "accessors": []
        }),
        bin: Some(Vec::new()),
        dirty: false,
    };
    let path = std::env::temp_dir().join(format!(
        "aio-asset-normalizer-invalid-texture-{}.bin",
        std::process::id()
    ));
    std::fs::write(&path, b"not an image").unwrap();
    let error = document
        .replace_texture(
            PrimitiveTarget {
                mesh: 0,
                primitive: 0,
            },
            TextureSlot::Normal,
            &path,
            true,
        )
        .unwrap_err();
    let _ = std::fs::remove_file(&path);
    assert!(error.to_string().contains("format could not be detected"));
}

#[test]
fn animation_trim_interpolates_boundaries_and_rebases_time() {
    let mut bin = Vec::new();
    for value in [0.0_f32, 1.0, 2.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for rotation in [
        [0.0_f32, 0.0, 0.0, 1.0],
        [0.0, 0.70710677, 0.0, 0.70710677],
        [0.0, 1.0, 0.0, 0.0],
    ] {
        for value in rotation {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut document = GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Root"}],
            "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 12},
                {"buffer": 0, "byteOffset": 12, "byteLength": 48}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "SCALAR", "min": [0.0], "max": [2.0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC4"}
            ],
            "animations": [{
                "name": "Turn",
                "samplers": [{"input": 0, "output": 1, "interpolation": "LINEAR"}],
                "channels": [{"sampler": 0, "target": {"node": 0, "path": "rotation"}}]
            }]
        }),
        bin: Some(std::mem::take(&mut bin)),
        dirty: false,
    };
    document
        .apply(EditOperation::TrimAnimation {
            animation: 0,
            start: 0.5,
            end: 1.5,
        })
        .unwrap();

    let times = document.read_accessor_f32(2).unwrap();
    assert_eq!(times, vec![vec![0.0], vec![0.5], vec![1.0]]);
    let output = document.accessor(3).unwrap();
    let output_bytes = document.accessor_bytes(output, 16).unwrap();
    let boundary = (0..4)
        .map(|component| {
            f32::from_le_bytes(
                output_bytes[component * 4..4 + component * 4]
                    .try_into()
                    .unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert!((boundary[1] - 0.38268343).abs() < 1e-4);
    assert!((boundary[3] - 0.9238795).abs() < 1e-4);
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

fn compact_export_fixture() -> GlbDocument {
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
    for value in [0.0_f32, 1.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for value in [[0.0_f32, 0.0, 0.0], [1.0_f32, 0.0, 0.0]] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    for matrix in [
        [
            1.0_f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
        [
            1.0_f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ],
    ] {
        for component in matrix {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    bin.extend_from_slice(&[0_u8, 0, 0, 0]);
    let byte_length = bin.len();
    GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [
                {"name": "Character", "nodes": [0]},
                {"name": "Unused", "nodes": [5]}
            ],
            "nodes": [
                {"name": "Root", "children": [1, 2, 3], "extras": {"keep": true}},
                {"name": "Body", "mesh": 0, "skin": 0},
                {"name": "Unused mesh", "mesh": 1},
                {"name": "Hip", "children": [4]},
                {"name": "Knee"},
                {"name": "Camera node", "camera": 0}
            ],
            "meshes": [
                {"name": "Body mesh", "primitives": [{"attributes": {"POSITION": 0}, "material": 0}]},
                {"name": "Unused mesh", "primitives": [{"attributes": {"POSITION": 0}, "material": 1}]}
            ],
            "materials": [
                {"name": "Body material", "pbrMetallicRoughness": {"baseColorFactor": [1.0, 0.0, 0.0, 1.0]}},
                {"name": "Unused material", "pbrMetallicRoughness": {"baseColorFactor": [0.0, 1.0, 0.0, 1.0]}}
            ],
            "cameras": [{"name": "Unused camera", "type": "perspective", "perspective": {"yfov": 1.0, "znear": 0.1}}],
            "skins": [{"name": "Character skin", "skeleton": 3, "joints": [3, 4], "inverseBindMatrices": 3}],
            "buffers": [{"byteLength": byte_length}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": 0, "byteLength": 36},
                {"buffer": 0, "byteOffset": 36, "byteLength": 8},
                {"buffer": 0, "byteOffset": 44, "byteLength": 24},
                {"buffer": 0, "byteOffset": 68, "byteLength": 128},
                {"buffer": 0, "byteOffset": 196, "byteLength": 4}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
                {"bufferView": 1, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0.0], "max": [1.0]},
                {"bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC3"},
                {"bufferView": 3, "componentType": 5126, "count": 2, "type": "MAT4"},
                {"bufferView": 4, "componentType": 5126, "count": 1, "type": "SCALAR"}
            ],
            "animations": [
                {"name": "Walk", "samplers": [{"input": 1, "output": 2, "interpolation": "LINEAR"}], "channels": [{"sampler": 0, "target": {"node": 3, "path": "translation"}}]},
                {"name": "Unused", "samplers": [{"input": 1, "output": 2, "interpolation": "LINEAR"}], "channels": [{"sampler": 0, "target": {"node": 2, "path": "translation"}}]}
            ]
        }),
        bin: Some(bin),
        dirty: false,
    }
}

fn character_export_selection() -> GlbExportSelection {
    GlbExportSelection {
        preset: GlbExportPreset::CharacterPackage,
        scene_index: 0,
        skin_index: Some(0),
        selected_nodes: BTreeSet::from([1]),
        selected_primitives: BTreeMap::from([(0, BTreeSet::from([0]))]),
        selected_animations: BTreeSet::from([0]),
        animation_output: AnimationOutputMode::Combined,
    }
}

#[test]
fn character_package_prunes_scene_graph_resources_and_reindexes_references() {
    let mut document = compact_export_fixture();
    let original = document.to_bytes().unwrap();
    let report = document
        .prune_for_export(&character_export_selection())
        .unwrap();

    assert_eq!(report.source.scenes, 2);
    assert_eq!(report.output.scenes, 1);
    assert_eq!(report.output.nodes, 4);
    assert_eq!(report.output.meshes, 1);
    assert_eq!(report.output.materials, 1);
    assert_eq!(report.output.skins, 1);
    assert_eq!(report.output.animations, 1);
    assert!(report.output_bin_bytes < report.source_bin_bytes);
    assert!(report.output_glb_bytes < report.source_glb_bytes);
    assert_eq!(document.json["scene"], 0);
    assert_eq!(document.json["scenes"].as_array().unwrap().len(), 1);
    assert_eq!(document.json["nodes"][0]["children"], json!([1, 2]));
    assert_eq!(document.json["nodes"][1]["mesh"], 0);
    assert_eq!(document.json["nodes"][1]["skin"], 0);
    assert!(document.json["nodes"][2].get("mesh").is_none());
    assert_eq!(
        document.json["animations"][0]["channels"][0]["target"]["node"],
        2
    );
    assert_eq!(
        document.json["buffers"][0]["byteLength"],
        report.output_bin_bytes
    );
    assert!(report.output_bin_bytes.is_multiple_of(4));
    assert!(document.json["bufferViews"].as_array().unwrap().iter().all(
        |view| {
            view.get("byteOffset")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .is_multiple_of(4)
        }
    ));
    assert_eq!(document.json["nodes"][0]["extras"]["keep"], true);
    assert!(document.json.get("cameras").is_none());
    assert_eq!(document.json["meshes"][0]["name"], "Body mesh");
    assert_eq!(document.json["materials"][0]["name"], "Body material");
    assert!(document.to_bytes().unwrap() != original);
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
    assert_eq!(document.to_bytes().unwrap().len(), report.output_glb_bytes);
    let runtime = AnimationRuntime::from_bytes_skeleton_only(
        &document.to_bytes().unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(runtime.clips.len(), 1);
    let mesh_runtime =
        AnimationRuntime::from_bytes(&document.to_bytes().unwrap(), None)
            .unwrap();
    assert_eq!(mesh_runtime.primitives.len(), 1);
}

#[test]
fn character_package_without_animations_is_a_model_and_skin_package() {
    let mut document = compact_export_fixture();
    let mut selection = character_export_selection();
    selection.selected_animations.clear();
    let report = document.prune_for_export(&selection).unwrap();

    assert_eq!(report.output.animations, 0);
    assert_eq!(report.output.meshes, 1);
    assert_eq!(report.output.skins, 1);
    assert!(document.json.get("animations").is_none());
    assert!(document.json.get("bufferViews").is_some());
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

#[test]
fn preserve_all_keeps_the_document_and_reports_serialized_size() {
    let mut document = compact_export_fixture();
    let original = document.to_bytes().unwrap();
    let selection = GlbExportSelection::default();

    let report = document.prune_for_export(&selection).unwrap();

    assert_eq!(document.to_bytes().unwrap(), original);
    assert_eq!(report.source, report.output);
    assert_eq!(report.source_glb_bytes, original.len());
    assert_eq!(report.output_glb_bytes, original.len());
}

#[test]
fn character_package_prunes_selected_mesh_primitives() {
    let mut document = compact_export_fixture();
    document.json["meshes"][0]["primitives"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "attributes": {"POSITION": 0},
            "material": 0
        }));

    let report = document
        .prune_for_export(&character_export_selection())
        .unwrap();

    assert_eq!(report.source.meshes, 2);
    assert_eq!(
        document.json["meshes"][0]["primitives"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

#[test]
fn compact_export_combines_selected_animations_and_reachable_targets() {
    let mut document = compact_export_fixture();
    let mut selection = character_export_selection();
    selection.selected_animations = BTreeSet::from([0, 1]);

    let report = document.prune_for_export(&selection).unwrap();

    assert_eq!(report.output.animations, 2);
    assert_eq!(report.output.nodes, 5);
    assert!(document.json["nodes"][2].get("mesh").is_none());
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

#[test]
fn skeleton_animation_removes_render_resources_but_keeps_skin_and_animation_targets(
) {
    let mut document = compact_export_fixture();
    let mut selection = character_export_selection();
    selection.preset = GlbExportPreset::SkeletonAnimation;
    selection.selected_nodes.clear();
    let report = document.prune_for_export(&selection).unwrap();

    assert_eq!(report.output.meshes, 0);
    assert_eq!(report.output.materials, 0);
    assert_eq!(report.output.skins, 1);
    assert_eq!(report.output.animations, 1);
    assert_eq!(report.output.scenes, 1);
    assert!(document.json.get("meshes").is_none());
    assert_eq!(document.json["scenes"][0]["nodes"], json!([0]));
    assert_eq!(document.json["nodes"][0]["children"], json!([1]));
    assert!(document.json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|node| {
            node.get("mesh").is_none() && node.get("camera").is_none()
        }));
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

#[test]
fn compact_export_rejects_a_selected_node_from_another_skin() {
    let mut document = compact_export_fixture();
    document.json["nodes"][1]["skin"] = json!(1);
    document.json["skins"] = json!([
        {"name": "Character skin", "skeleton": 3, "joints": [3, 4], "inverseBindMatrices": 3},
        {"name": "Other skin", "skeleton": 3, "joints": [3, 4], "inverseBindMatrices": 3}
    ]);

    let error = document
        .prune_for_export(&character_export_selection())
        .unwrap_err();

    assert!(error.to_string().contains("references Skin 1"));
}

#[test]
fn compact_export_removes_cameras_and_punctual_lights() {
    let mut document = compact_export_fixture();
    document.json["extensionsUsed"] = json!(["KHR_lights_punctual"]);
    document.json["extensions"] =
        json!({"KHR_lights_punctual": {"lights": [{"type": "point"}]} });
    document.json["nodes"][0]["extensions"] =
        json!({"KHR_lights_punctual": {"light": 0}});

    document
        .prune_for_export(&character_export_selection())
        .unwrap();

    assert!(document.json.get("cameras").is_none());
    assert!(document.json.get("extensions").is_none());
    assert!(document.json.get("extensionsUsed").is_none());
    assert!(document.json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|node| node
            .get("extensions")
            .and_then(Value::as_object)
            .is_none_or(
                |extensions| !extensions.contains_key("KHR_lights_punctual")
            )));
}

#[test]
fn compact_export_rejects_external_buffers_and_unknown_extensions() {
    let mut external_buffer = compact_export_fixture();
    external_buffer.json["buffers"][0]["uri"] = json!("mesh.bin");
    let error = external_buffer
        .prune_for_export(&character_export_selection())
        .unwrap_err();
    assert!(error.to_string().contains("embedded GLB buffer"));

    let mut unknown_extension = compact_export_fixture();
    unknown_extension.json["nodes"][0]["extensions"] =
        json!({"VENDOR_node_extension": {"index": 0}});
    let error = unknown_extension
        .prune_for_export(&character_export_selection())
        .unwrap_err();
    assert!(error.to_string().contains("VENDOR_node_extension"));

    let mut extras_extension = compact_export_fixture();
    extras_extension.json["nodes"][0]["extras"]["metadata"] =
        json!({"extensions": {"VENDOR_data": {"index": 99}}});
    extras_extension
        .prune_for_export(&character_export_selection())
        .unwrap();
    assert_eq!(
        extras_extension.json["nodes"][0]["extras"]["metadata"]["extensions"]
            ["VENDOR_data"]["index"],
        99
    );

    let mut unknown_material_extension = compact_export_fixture();
    unknown_material_extension.json["materials"][0]["extensions"] =
        json!({"KHR_materials_future": {"value": true}});
    let error = unknown_material_extension
        .prune_for_export(&character_export_selection())
        .unwrap_err();
    assert!(error.to_string().contains("KHR_materials_future"));
}

#[test]
fn compact_export_validates_empty_and_out_of_range_selection() {
    let document = compact_export_fixture();
    let mut empty_nodes = character_export_selection();
    empty_nodes.selected_nodes.clear();
    let validation = document.validate_export_selection(&empty_nodes);
    assert!(!validation.is_valid());
    assert!(validation.errors[0].contains("at least one selected node"));

    let mut invalid_primitive = character_export_selection();
    invalid_primitive
        .selected_primitives
        .insert(0, BTreeSet::from([9]));
    let validation = document.validate_export_selection(&invalid_primitive);
    assert!(!validation.is_valid());
    assert!(validation.errors[0].contains("Primitive 9"));

    let mut invalid_split = character_export_selection();
    invalid_split.selected_animations.clear();
    invalid_split.animation_output = AnimationOutputMode::Split;
    let mut document = compact_export_fixture();
    let error = document.prune_for_export(&invalid_split).unwrap_err();
    assert!(error.to_string().contains("Split animation output"));
}

#[test]
fn compact_export_reindexes_material_texture_image_and_sampler_references() {
    let mut document = compact_export_fixture();
    document.json["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] =
        json!({"index": 1});
    document.json["textures"] = json!([
        {"name": "Unused texture", "source": 0, "sampler": 0},
        {"name": "Body texture", "source": 1, "sampler": 1}
    ]);
    document.json["images"] = json!([
        {"name": "Unused image", "uri": "data:image/png;base64,AA=="},
        {"name": "Body image", "uri": "data:image/png;base64,AA=="}
    ]);
    document.json["samplers"] =
        json!([{"magFilter": 9728}, {"magFilter": 9729}]);

    let report = document
        .prune_for_export(&character_export_selection())
        .unwrap();

    assert_eq!(report.output.materials, 1);
    assert_eq!(report.output.images, 1);
    assert_eq!(
        document.json["materials"][0]["pbrMetallicRoughness"]
            ["baseColorTexture"]["index"],
        0
    );
    assert_eq!(document.json["textures"][0]["name"], "Body texture");
    assert_eq!(document.json["textures"][0]["source"], 0);
    assert_eq!(document.json["textures"][0]["sampler"], 0);
    assert_eq!(document.json["images"][0]["name"], "Body image");
    assert_eq!(document.json["samplers"][0]["magFilter"], 9729);
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

#[test]
fn compact_export_rejects_external_image_uris() {
    let mut document = compact_export_fixture();
    document.json["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] =
        json!({"index": 0});
    document.json["textures"] = json!([{"source": 0}]);
    document.json["images"] = json!([{"uri": "textures/body.png"}]);

    let error = document
        .prune_for_export(&character_export_selection())
        .unwrap_err();
    assert!(error.to_string().contains("external URI"));
}

#[test]
fn skeleton_animation_rejects_morph_target_channels() {
    let mut document = compact_export_fixture();
    document.json["animations"][0]["channels"][0]["target"]["path"] =
        json!("weights");
    let mut selection = character_export_selection();
    selection.preset = GlbExportPreset::SkeletonAnimation;
    selection.selected_nodes.clear();

    let error = document.prune_for_export(&selection).unwrap_err();

    assert!(error.to_string().contains("Morph Target"));
}
