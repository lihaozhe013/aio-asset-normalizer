//! Converter submission safety and an opt-in real Blender integration test.
use aio_asset_normalizer::modules::{
    atomic_file,
    glb::{pipeline::ExportEdits, GlbDocument},
    operations::{
        self,
        converter::{self, ConversionRequest},
    },
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
    converter::preflight(&ConversionRequest {
        input,
        output: out.clone(),
        blender: Some(fake),
        overwrite: false,
    })
    .unwrap();
    assert!(!out.parent().unwrap().exists());
}
#[test]
#[ignore = "Requires the fixed Blender version installed by the integration workflow"]
fn real_blender_converter_produces_inspectable_and_exportable_glb() {
    let blender = aio_asset_normalizer::modules::blender::bridge::find_blender(None)
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
    let converted = dir.path().join("converted.glb");
    converter::execute(
        &ConversionRequest {
            input,
            output: converted.clone(),
            blender: Some(blender),
            overwrite: false,
        },
        1,
    )
    .unwrap();
    let document = GlbDocument::load(&converted).unwrap();
    assert!(document.summary().meshes > 0);
    let output = dir.path().join("standardized");
    let selection = document.default_export_selection().unwrap();
    let jobs = operations::glb::prepare_export(
        &document,
        &ExportEdits::default(),
        &selection,
        &output,
        &[&converted],
        false,
    )
    .unwrap();
    for job in jobs {
        operations::glb::execute_job(&job, false).unwrap();
    }
    assert!(output.is_file());
    assert!(GlbDocument::load(&output).is_ok());
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
