//! Converter submission safety and an opt-in real Blender integration test.
use aio_asset_normalizer::modules::{
    atomic_file,
    operations::converter::{self, ConversionRequest},
};
use std::process::Command;
mod support;
#[test]
fn failed_validation_preserves_existing_output() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("existing.glb");
    std::fs::write(&out, b"original").unwrap();
    let staged = atomic_file::stage(&out).unwrap();
    std::fs::write(staged.path(), b"not a GLB").unwrap();
    assert!(converter::validate_and_commit(staged, &out, true).is_err());
    assert_eq!(std::fs::read(out).unwrap(), b"original");
}
#[test]
fn no_clobber_commit_protects_outputs_created_after_preflight() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("race.glb");
    let staged = atomic_file::stage(&out).unwrap();
    std::fs::write(staged.path(), support::glb_bytes()).unwrap();
    std::fs::write(&out, b"other process").unwrap();
    assert!(converter::validate_and_commit(staged, &out, false).is_err());
    assert_eq!(std::fs::read(out).unwrap(), b"other process");
}
#[test]
fn concurrent_staging_paths_are_distinct_and_failure_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("model.glb");
    let first = atomic_file::stage(&out).unwrap();
    let second = atomic_file::stage(&out).unwrap();
    assert_ne!(first.path(), second.path());
    let staged = first.path().to_path_buf();
    drop(first);
    assert!(!staged.exists());
}
#[test]
fn converter_dry_run_checks_dependencies_without_generating_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.fbx");
    std::fs::write(&input, b"preflight does not parse FBX").unwrap();
    let fake = dir.path().join("blender-placeholder");
    std::fs::write(&fake, b"").unwrap();
    let out = dir.path().join("new/model.glb");
    let result = Command::new(env!("CARGO_BIN_EXE_aio-asset-normalizer-cli"))
        .args([
            "fbx",
            "convert",
            input.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--blender",
            fake.to_str().unwrap(),
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["results"]["validation_scope"], "preflight-only");
    assert!(!out.parent().unwrap().exists());
}
#[test]
#[ignore = "Requires the fixed Blender version installed by the integration workflow"]
fn real_blender_converter_matches_cli_and_snapshot_request() {
    let blender =
        aio_asset_normalizer::modules::blender::bridge::find_blender(None)
            .expect("Blender must be installed for this opt-in test");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("cube.blend");
    let script = dir.path().join("fixture.py");
    std::fs::write(&script,"import bpy, sys\nbpy.ops.wm.read_factory_settings(use_empty=True)\nbpy.ops.mesh.primitive_cube_add()\nbpy.ops.wm.save_as_mainfile(filepath=sys.argv[-1])\n").unwrap();
    assert!(Command::new(&blender)
        .args([
            "-b",
            "-P",
            script.to_str().unwrap(),
            "--",
            input.to_str().unwrap()
        ])
        .status()
        .unwrap()
        .success());
    let cli = dir.path().join("cli.glb");
    let gui = dir.path().join("gui.glb");
    let result = Command::new(env!("CARGO_BIN_EXE_aio-asset-normalizer-cli"))
        .args([
            "fbx",
            "convert",
            input.to_str().unwrap(),
            "--out",
            cli.to_str().unwrap(),
            "--blender",
            blender.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    converter::execute(
        &ConversionRequest {
            input,
            output: gui.clone(),
            blender: Some(blender),
            overwrite: false,
        },
        1,
    )
    .unwrap();
    assert_eq!(std::fs::read(&cli).unwrap(), std::fs::read(gui).unwrap());
    let inspect = Command::new(env!("CARGO_BIN_EXE_aio-asset-normalizer-cli"))
        .args(["glb", "inspect", cli.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(inspect.status.success());
    let output = dir.path().join("standardized");
    for dry_run in [true, false] {
        let mut command =
            Command::new(env!("CARGO_BIN_EXE_aio-asset-normalizer-cli"));
        command.args([
            "glb",
            "export",
            cli.to_str().unwrap(),
            "--output-root",
            output.to_str().unwrap(),
            "--preset",
            "preserve-all",
        ]);
        if dry_run {
            command.arg("--dry-run");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
    }
    assert!(output.join("cli_full.glb").is_file());
}

#[test]
fn batch_output_collisions_are_rejected_even_with_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let requests: Vec<_> = ["hero.fbx", "hero.obj"]
        .into_iter()
        .map(|name| ConversionRequest {
            input: dir.path().join(name),
            output: dir.path().join("hero_normalized.glb"),
            blender: None,
            overwrite: true,
        })
        .collect();
    assert!(converter::validate_batch_outputs(&requests).is_err());
}
