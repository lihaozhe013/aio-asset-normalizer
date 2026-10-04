#![allow(dead_code)]
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
// ---- fixture helpers -------------------------------------------------------

pub fn extract_first_json_block(text: &str) -> Value {
    let start = text.find("```json\n").expect("prompt has a json block") + 8;
    let rest = &text[start..];
    let end = rest.find("\n```").expect("prompt json block is closed");
    serde_json::from_str(&rest[..end]).expect("prompt json block parses")
}

pub fn mapping_from_context(context: &Value) -> Value {
    let source = &context["source"];
    let target = &context["target"];
    let node = |value: &Value| {
        let name = value
            .get("name")
            .or_else(|| value.get("node"))
            .cloned()
            .unwrap_or(Value::Null);
        json!({
            "node": name,
            "path": value["path"],
            "index": value["index"],
        })
    };
    let source_node = |index: usize| node(&source["nodes"][index]);
    let target_node = |index: usize| node(&target["nodes"][index]);
    json!({
        "schema": "com.aio-asset-normalizer.skeleton-mapping",
        "version": 2,
        "source": {
            "kind": "bvh",
            "file_sha256": source["file_sha256"],
            "skeleton_sha256": source["skeleton_sha256"],
            "skin": Value::Null,
            "root": source["root"],
            "up_axis": source["up_axis"],
            "forward_axis": source["forward_axis"],
            "unit": source["unit"],
        },
        "target": {
            "kind": "glb",
            "file_sha256": target["file_sha256"],
            "skeleton_sha256": target["skeleton_sha256"],
            "skin": target["skin"],
            "root": target["root"],
            "up_axis": target["up_axis"],
            "forward_axis": target["forward_axis"],
            "unit": target["unit"],
        },
        "bones": [
            { "source": source_node(0), "target": target_node(1), "rotation_offset_xyzw": [0.0, 0.0, 0.0, 1.0] },
            { "source": source_node(1), "target": target_node(2), "rotation_offset_xyzw": [0.0, 0.0, 0.0, 1.0] },
        ],
        "ignored_sources": [],
        "root_motion": Value::Null,
    })
}

pub fn align4(mut data: Vec<u8>) -> Vec<u8> {
    while data.len() % 4 != 0 {
        data.push(0);
    }
    data
}

pub fn glb_bytes() -> Vec<u8> {
    let mut bin = Vec::new();
    let mut offsets = Vec::new();
    let mut push = |data: Vec<u8>| {
        let offset = bin.len();
        bin.extend_from_slice(&align4(data));
        offsets.push(offset);
        offset
    };
    let positions = [
        (-0.5_f32, 0.0_f32, 0.0_f32),
        (0.5, 0.0, 0.0),
        (0.0, 2.0, 0.0),
    ]
    .iter()
    .flat_map(|(x, y, z)| {
        [x.to_le_bytes(), y.to_le_bytes(), z.to_le_bytes()].concat()
    })
    .collect::<Vec<u8>>();
    let position_offset = push(positions.clone());
    let joints = (0..3).flat_map(|_| [0_u8, 0, 0, 0]).collect::<Vec<u8>>();
    let joints_offset = push(joints.clone());
    let weights = (0..3)
        .flat_map(|_| 1.0_f32.to_le_bytes().into_iter().chain([0_u8; 12]))
        .collect::<Vec<u8>>();
    let weights_offset = push(weights.clone());
    let identity = [
        1.0_f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        -1.0, 0.0, 1.0,
    ];
    let mut inverse_bind = Vec::new();
    for matrix in [identity, identity] {
        for value in matrix {
            inverse_bind.extend_from_slice(&value.to_le_bytes());
        }
    }
    let inverse_bind_offset = push(inverse_bind.clone());
    let times = [0.0_f32, 0.5, 1.0]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<u8>>();
    let times_offset = push(times.clone());
    let rotations = [
        0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 0.866, 0.0, 0.0, 0.0, 1.0,
    ]
    .iter()
    .flat_map(|value| value.to_le_bytes())
    .collect::<Vec<u8>>();
    let rotations_offset = push(rotations.clone());

    let json = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "name": "Main", "nodes": [0] }],
        "nodes": [
            { "name": "Root", "mesh": 0, "skin": 0, "children": [1] },
            { "name": "Pelvis", "children": [2], "translation": [0.0, 1.0, 0.0] },
            { "name": "Spine", "translation": [0.0, 0.5, 0.0] }
        ],
        "skins": [{ "name": "Armature", "skeleton": 1, "joints": [1, 2], "inverseBindMatrices": 3 }],
        "meshes": [{ "name": "Body", "primitives": [{ "attributes": { "POSITION": 0, "JOINTS_0": 1, "WEIGHTS_0": 2 } }] }],
        "animations": [{
            "name": "Idle",
            "samplers": [{ "input": 4, "output": 5, "interpolation": "LINEAR" }],
            "channels": [{ "sampler": 0, "target": { "node": 1, "path": "rotation" } }]
        }],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [-0.5, 0.0, 0.0], "max": [0.5, 2.0, 0.0] },
            { "bufferView": 1, "componentType": 5121, "count": 3, "type": "VEC4" },
            { "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4" },
            { "bufferView": 3, "componentType": 5126, "count": 2, "type": "MAT4" },
            { "bufferView": 4, "componentType": 5126, "count": 3, "type": "SCALAR", "min": [0.0], "max": [1.0] },
            { "bufferView": 5, "componentType": 5126, "count": 3, "type": "VEC4" }
        ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": position_offset, "byteLength": positions.len() },
            { "buffer": 0, "byteOffset": joints_offset, "byteLength": joints.len() },
            { "buffer": 0, "byteOffset": weights_offset, "byteLength": weights.len() },
            { "buffer": 0, "byteOffset": inverse_bind_offset, "byteLength": inverse_bind.len() },
            { "buffer": 0, "byteOffset": times_offset, "byteLength": times.len() },
            { "buffer": 0, "byteOffset": rotations_offset, "byteLength": rotations.len() }
        ],
        "buffers": [{ "byteLength": bin.len() }],
    });

    let mut json_bytes = serde_json::to_vec(&json).unwrap();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let bin = align4(bin);
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();

    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2_u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&bin);
    glb
}

pub fn bvh_text() -> &'static str {
    "HIERARCHY\n\
ROOT Hips\n\
{\n\
  OFFSET 0.00 0.00 0.00\n\
  CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation\n\
  JOINT Spine\n\
  {\n\
    OFFSET 0.00 10.00 0.00\n\
    CHANNELS 3 Zrotation Xrotation Yrotation\n\
    End Site\n\
    {\n\
      OFFSET 0.00 10.00 0.00\n\
    }\n\
  }\n\
}\n\
MOTION\n\
Frames: 3\n\
Frame Time: 0.0333333\n\
0.0 0.0 0.0 0.0 0.0 0.0 0.0 0.0 0.0\n\
0.0 0.0 0.0 0.0 5.0 0.0 0.0 5.0 0.0\n\
0.0 0.0 0.0 0.0 0.0 0.0 0.0 0.0 0.0\n"
}

pub fn write_fixtures(dir: &Path) -> (PathBuf, PathBuf) {
    let glb = dir.join("character.glb");
    let bvh = dir.join("walk.bvh");
    std::fs::write(&glb, glb_bytes()).unwrap();
    std::fs::write(&bvh, bvh_text()).unwrap();
    (glb, bvh)
}
