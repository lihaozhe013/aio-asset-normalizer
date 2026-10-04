//! End-to-end tests for the `aio-asset-normalizer-cli` binary.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::{json, Value};

mod support;
use support::*;

const BIN: &str = env!("CARGO_BIN_EXE_aio-asset-normalizer-cli");

fn workspace(label: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("aio-cli-tests-{}-{label}", std::process::id()));
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
    assert_eq!(
        value["results"]["files"][0]["joints"][0]["name"],
        json!("Hips")
    );

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
    assert_eq!(
        value["results"]["files"][0]["summary"]["animations"],
        json!(1)
    );
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
