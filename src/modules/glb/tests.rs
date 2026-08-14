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
