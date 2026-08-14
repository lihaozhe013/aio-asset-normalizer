use super::*;

#[test]
fn matrix_multiplication_preserves_identity() {
    assert_eq!(multiply(identity(), identity()), identity());
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
            axis: [0.0, 1.0, 0.0],
            degrees: 90.0,
        })
        .unwrap();
    let bytes = document.to_bytes().unwrap();
    let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let json: Value = serde_json::from_slice(&glb.json).unwrap();
    let matrix = json["nodes"][0]["matrix"].as_array().unwrap();
    assert!((matrix[12].as_f64().unwrap() - 0.0).abs() < 1e-5);
    assert!((matrix[14].as_f64().unwrap() + 1.0).abs() < 1e-5);
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
