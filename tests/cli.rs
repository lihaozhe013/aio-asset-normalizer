//! End-to-end tests for the `aio-asset-normalizer-cli` binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_aio-asset-normalizer-cli");

fn workspace(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aio-cli-tests-{}-{label}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout was not JSON: {error}\nstdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn assert_envelope(value: &Value, command: &str) {
    assert_eq!(value["schema_version"], json!(1));
    assert_eq!(value["command"], json!(command));
    assert!(value["version"]["commit"].is_string());
}

fn assert_success(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    stdout_json(output)
}

fn write_fixtures(dir: &Path) -> (PathBuf, PathBuf) {
    let glb = dir.join("character.glb");
    let bvh = dir.join("walk.bvh");
    std::fs::write(&glb, glb_bytes()).unwrap();
    std::fs::write(&bvh, bvh_text()).unwrap();
    (glb, bvh)
}

#[test]
fn inspect_reports_the_documented_envelope() {
    let dir = workspace("inspect");
    let (glb, _) = write_fixtures(&dir);

    let output = run(&["glb", "inspect", glb.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    let value = stdout_json(&output);
    assert_envelope(&value, "glb.inspect");
    assert_eq!(value["ok"], json!(true));
    let file = &value["results"]["files"][0];
    assert_eq!(file["ok"], json!(true));
    assert_eq!(file["summary"]["nodes"], json!(3));
    assert_eq!(file["skins"][0]["name"], json!("Armature"));
    assert_eq!(file["catalog"]["animations"][0]["name"], json!("Idle"));
}

#[test]
fn inspect_reports_io_failures_with_exit_code_four() {
    let dir = workspace("inspect-missing");
    let missing = dir.join("missing.glb");
    let output = run(&["glb", "inspect", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(4));
    let value = stdout_json(&output);
    assert_eq!(value["ok"], json!(false));
    assert_eq!(value["error"]["code"], json!("io"));
}

#[test]
fn export_writes_outputs_and_protects_existing_files() {
    let dir = workspace("export");
    let (glb, _) = write_fixtures(&dir);
    let out = dir.join("out");

    let args = [
        "glb",
        "export",
        glb.to_str().unwrap(),
        "--output-root",
        out.to_str().unwrap(),
        "--preset",
        "character",
    ];
    let output = run(&args);
    assert_eq!(output.status.code(), Some(0));
    let value = stdout_json(&output);
    assert_envelope(&value, "glb.export");
    let written = out.join("character_character.glb");
    assert!(written.is_file(), "expected {}", written.display());

    let rerun = run(&args);
    assert_eq!(rerun.status.code(), Some(3));
    let value = stdout_json(&rerun);
    assert_eq!(value["ok"], json!(false));
    assert_eq!(value["error"]["code"], json!("validation"));

    let overwrite = run(&[
        "glb",
        "export",
        glb.to_str().unwrap(),
        "--output-root",
        out.to_str().unwrap(),
        "--preset",
        "character",
        "--overwrite",
    ]);
    assert_eq!(overwrite.status.code(), Some(0));
}

#[test]
fn export_recursive_scans_the_input_tree() {
    let dir = workspace("export-recursive");
    let (glb, _) = write_fixtures(&dir);
    let nested = dir.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::copy(&glb, nested.join("copy.glb")).unwrap();
    let out = dir.join("out");

    let output = run(&[
        "glb",
        "export",
        "--input-root",
        dir.to_str().unwrap(),
        "--recursive",
        "--output-root",
        out.to_str().unwrap(),
        "--preset",
        "skeleton",
    ]);
    let value = assert_success(&output);
    let entries = value["results"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(out.join("character_skeleton.glb").is_file());
    assert!(out.join("nested/copy_skeleton.glb").is_file());
}

#[test]
fn bvh_inspect_and_process_trim_a_file() {
    let dir = workspace("bvh");
    let (_, bvh) = write_fixtures(&dir);

    let inspect = run(&["bvh", "inspect", bvh.to_str().unwrap()]);
    assert_eq!(inspect.status.code(), Some(0));
    let value = stdout_json(&inspect);
    assert_envelope(&value, "bvh.inspect");
    assert_eq!(value["results"]["files"][0]["frame_count"], json!(3));
    assert_eq!(value["results"]["files"][0]["joints"][0]["name"], json!("Hips"));

    let trimmed = dir.join("trimmed.bvh");
    let process = run(&[
        "bvh",
        "process",
        bvh.to_str().unwrap(),
        "--output",
        trimmed.to_str().unwrap(),
    ]);
    assert_eq!(process.status.code(), Some(0));
    assert!(trimmed.is_file());
}

#[test]
fn retarget_prompt_validate_and_run_complete_the_agent_workflow() {
    let dir = workspace("retarget");
    let (glb, bvh) = write_fixtures(&dir);

    let prompt = dir.join("prompt.md");
    let output = run(&[
        "retarget",
        "prompt",
        "--source",
        bvh.to_str().unwrap(),
        "--target",
        glb.to_str().unwrap(),
        "--out",
        prompt.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert!(prompt.is_file());
    let text = std::fs::read_to_string(&prompt).unwrap();
    let context = extract_first_json_block(&text);

    let mapping = mapping_from_context(&context);
    let mapping_path = dir.join("mapping.json");
    std::fs::write(&mapping_path, mapping.to_string()).unwrap();

    let validate = run(&[
        "retarget",
        "validate",
        "--source",
        bvh.to_str().unwrap(),
        "--target",
        glb.to_str().unwrap(),
        "--mapping",
        mapping_path.to_str().unwrap(),
    ]);
    let value = assert_success(&validate);
    assert_envelope(&value, "retarget.validate");
    assert_eq!(value["results"]["valid"], json!(true));

    let retargeted = dir.join("retargeted.glb");
    let run_output = run(&[
        "retarget",
        "run",
        "--source",
        bvh.to_str().unwrap(),
        "--target",
        glb.to_str().unwrap(),
        "--mapping",
        mapping_path.to_str().unwrap(),
        "--preset",
        "skeleton",
        "--out",
        retargeted.to_str().unwrap(),
    ]);
    assert_eq!(run_output.status.code(), Some(0));
    assert!(retargeted.is_file());

    let inspect = run(&["glb", "inspect", retargeted.to_str().unwrap()]);
    let value = stdout_json(&inspect);
    assert_eq!(value["results"]["files"][0]["summary"]["animations"], json!(1));
}

#[test]
fn usage_errors_exit_with_two() {
    let output = run(&["glb", "inspect", "--not-a-flag"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn docs_raw_prints_the_embedded_markdown() {
    let output = run(&["docs", "--raw"]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.starts_with("# AIO Asset Normalizer CLI"));
    assert!(text.contains("aio-asset-normalizer-cli"));
}

// ---- fixture helpers -------------------------------------------------------

fn extract_first_json_block(text: &str) -> Value {
    let start = text.find("```json\n").expect("prompt has a json block") + 8;
    let rest = &text[start..];
    let end = rest.find("\n```").expect("prompt json block is closed");
    serde_json::from_str(&rest[..end]).expect("prompt json block parses")
}

fn mapping_from_context(context: &Value) -> Value {
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

fn align4(mut data: Vec<u8>) -> Vec<u8> {
    while data.len() % 4 != 0 {
        data.push(0);
    }
    data
}

fn glb_bytes() -> Vec<u8> {
    let mut bin = Vec::new();
    let mut offsets = Vec::new();
    let mut push = |data: Vec<u8>| {
        let offset = bin.len();
        bin.extend_from_slice(&align4(data));
        offsets.push(offset);
        offset
    };
    let positions = [(-0.5_f32, 0.0_f32, 0.0_f32), (0.5, 0.0, 0.0), (0.0, 2.0, 0.0)]
        .iter()
        .flat_map(|(x, y, z)| {
            [x.to_le_bytes(), y.to_le_bytes(), z.to_le_bytes()].concat()
        })
        .collect::<Vec<u8>>();
    let position_offset = push(positions.clone());
    let joints = (0..3)
        .flat_map(|_| [0_u8, 0, 0, 0])
        .collect::<Vec<u8>>();
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
    let rotations = [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 0.866, 0.0, 0.0, 0.0, 1.0]
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

fn bvh_text() -> &'static str {
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