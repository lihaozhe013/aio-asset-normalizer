use super::*;
use serde_json::json;

fn fixture(drift: f32) -> GlbDocument {
    let mut bin = Vec::new();
    for value in [0.0_f32, 1.0] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for value in [[0.0_f32, 0.0, 0.0], [drift, 0.2, 0.0]] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    for value in [[0.0_f32, 0.0, 0.0, 1.0], [0.0, 0.17364818, 0.0, 0.9848077]] {
        for component in value {
            bin.extend_from_slice(&component.to_le_bytes());
        }
    }
    GlbDocument {
        source_path: None,
        json: json!({
            "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}],
            "nodes": [{"name": "Skeleton", "children": [1]}, {"name": "Joint", "translation": [0.0, 1.0, 0.0]}],
            "skins": [{"joints": [0, 1], "skeleton": 0}], "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 8}, {"buffer": 0, "byteOffset": 8, "byteLength": 24}, {"buffer": 0, "byteOffset": 32, "byteLength": 32}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 2, "type": "SCALAR"}, {"bufferView": 1, "componentType": 5126, "count": 2, "type": "VEC3"}, {"bufferView": 2, "componentType": 5126, "count": 2, "type": "VEC4"}],
            "animations": [{"name": "Walk", "samplers": [{"input": 0, "output": 1, "interpolation": "LINEAR"}, {"input": 0, "output": 2, "interpolation": "LINEAR"}], "channels": [{"sampler": 0, "target": {"node": 0, "path": "translation"}}, {"sampler": 1, "target": {"node": 1, "path": "rotation"}}]}]
        }),
        bin: Some(bin),
        dirty: false,
    }
}

#[test]
fn smart_loop_closes_small_drift_and_preserves_vertical_motion() {
    let mut document = fixture(0.01);
    let report = document
        .smart_loop_animation(0, SmartLoopOptions::default())
        .unwrap();
    assert!(!report.already_looped);
    assert!((report.drift_ratio - 0.01).abs() < 1e-5);
    assert!((report.new_duration - 1.15).abs() < 1e-5);
    let sampler = document.json["animations"][0]["samplers"][0].clone();
    let times = document
        .read_accessor_f32(sampler["input"].as_u64().unwrap() as usize)
        .unwrap();
    assert!((times.last().unwrap()[0] - 1.15).abs() < 1e-5);
    let values = read_output(
        &document,
        document
            .accessor(sampler["output"].as_u64().unwrap() as usize)
            .unwrap(),
    )
    .unwrap();
    assert!(values.last().unwrap().iter().all(|v| v.abs() < 1e-5));
    assert!((values[1][1] - 0.2).abs() < 1e-5);
    let rotation = document.json["animations"][0]["samplers"][1]["output"]
        .as_u64()
        .unwrap() as usize;
    let rotations =
        read_output(&document, document.accessor(rotation).unwrap()).unwrap();
    assert!(
        vector_distance(rotations.first().unwrap(), rotations.last().unwrap())
            < 1e-5
    );
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

#[test]
fn smart_loop_rejects_root_motion_without_mutation() {
    let mut document = fixture(0.5);
    let original = document.to_bytes().unwrap();
    let error = document
        .smart_loop_animation(0, SmartLoopOptions::default())
        .unwrap_err();
    assert!(error.to_string().contains("Root Motion"));
    assert_eq!(document.to_bytes().unwrap(), original);
    let mut document = fixture(0.01);
    document.json["animations"][0]["channels"][1]["target"]["node"] = json!(0);
    let original = document.to_bytes().unwrap();
    let error = document
        .smart_loop_animation(0, SmartLoopOptions::default())
        .unwrap_err();
    assert!(error.to_string().contains("rotation"));
    assert_eq!(document.to_bytes().unwrap(), original);
}

#[test]
fn smart_loop_rejects_invalid_inputs_without_mutation() {
    let mut document = fixture(0.01);
    let original = document.to_bytes().unwrap();
    let error = document
        .smart_loop_animation(
            0,
            SmartLoopOptions {
                transition_seconds: 0.009,
            },
        )
        .unwrap_err();
    assert!(error.to_string().contains("0.01"));
    assert_eq!(document.to_bytes().unwrap(), original);
    document.json.as_object_mut().unwrap().remove("skins");
    let original = document.to_bytes().unwrap();
    let error = document
        .smart_loop_animation(0, SmartLoopOptions::default())
        .unwrap_err();
    assert!(error.to_string().contains("Skinned GLB"));
    assert_eq!(document.to_bytes().unwrap(), original);
}
