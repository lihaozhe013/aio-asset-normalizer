use super::*;

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
