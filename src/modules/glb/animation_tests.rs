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

fn rotating_clip_fixture(first_time: f32) -> GlbDocument {
    let mut bin = Vec::new();
    for index in 0..3 {
        let value = first_time + index as f32;
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
    let input_buffer_length = bin.len();
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
                {"buffer": 0, "byteOffset": 12, "byteLength": input_buffer_length - 12}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "SCALAR"},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC4"}
            ],
            "animations": [{
                "name": "Jump",
                "samplers": [{"input": 0, "output": 1, "interpolation": "LINEAR"}],
                "channels": [{"sampler": 0, "target": {"node": 0, "path": "rotation"}}]
            }]
        }),
        bin: Some(bin),
        dirty: false,
    }
}

#[test]
fn animation_trim_clamps_start_before_first_keyframe() {
    let mut document = rotating_clip_fixture(1.0);
    document
        .apply(EditOperation::TrimAnimation {
            animation: 0,
            start: 0.0,
            end: 1.5,
        })
        .expect("start below the first keyframe should clamp");
    let times = document.read_accessor_f32(2).unwrap();
    assert_eq!(times, vec![vec![0.0], vec![0.5]]);
    gltf::Gltf::from_slice(&document.to_bytes().unwrap()).unwrap();
}

#[test]
fn animation_trim_rejects_ranges_without_overlap() {
    let mut document = rotating_clip_fixture(1.0);
    let error = document
        .apply(EditOperation::TrimAnimation {
            animation: 0,
            start: 5.0,
            end: 6.0,
        })
        .expect_err("a range beyond the timeline must fail");
    let message = error.to_string();
    assert!(message.contains("Animation 0 sampler 0"), "{message}");
    assert!(message.contains("does not overlap"), "{message}");
    assert!(message.contains("[1, 3]"), "{message}");
}

#[test]
fn animation_time_range_intersects_sampler_timelines() {
    fn push(document: &mut GlbDocument, list: &str, value: serde_json::Value) {
        document
            .json
            .get_mut(list)
            .and_then(Value::as_array_mut)
            .expect("fixture array")
            .push(value);
    }
    let mut document = rotating_clip_fixture(1.0);
    let mut bin = document.bin.take().unwrap();
    let offset = bin.len() as u64;
    for value in [0.5_f32, 2.5] {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    for translation in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0]] {
        for value in translation {
            bin.extend_from_slice(&value.to_le_bytes());
        }
    }
    let total = bin.len();
    document.bin = Some(bin);
    document.json["buffers"][0]["byteLength"] = json!(total);
    push(
        &mut document,
        "bufferViews",
        json!({
            "buffer": 0, "byteOffset": offset, "byteLength": 8
        }),
    );
    push(
        &mut document,
        "bufferViews",
        json!({
            "buffer": 0, "byteOffset": offset + 8, "byteLength": 24
        }),
    );
    push(
        &mut document,
        "accessors",
        json!({
            "bufferView": 2, "componentType": 5126, "count": 2, "type": "SCALAR"
        }),
    );
    push(
        &mut document,
        "accessors",
        json!({
            "bufferView": 3, "componentType": 5126, "count": 2, "type": "VEC3"
        }),
    );
    document
        .json
        .get_mut("animations")
        .and_then(Value::as_array_mut)
        .expect("fixture animations")[0]
        .get_mut("samplers")
        .and_then(Value::as_array_mut)
        .expect("fixture samplers")
        .push(json!({
            "input": 2, "output": 3, "interpolation": "LINEAR"
        }));
    document
        .json
        .get_mut("animations")
        .and_then(Value::as_array_mut)
        .expect("fixture animations")[0]
        .get_mut("channels")
        .and_then(Value::as_array_mut)
        .expect("fixture channels")
        .push(json!({
            "sampler": 1, "target": {"node": 0, "path": "translation"}
        }));
    assert_eq!(document.animation_time_range(0).unwrap(), (1.0, 2.5));
}
