//! Real CLI executions compared with the pure adapters used by desktop state.
use aio_asset_normalizer::modules::{
    glb::{pipeline::ExportEdits, GlbDocument, GlbExportPreset},
    operations::{
        self, retarget as rt,
        spec::{EditJobFile, EditSpec, SelectionSpec},
    },
};
use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};
mod support;
const BIN: &str = env!("CARGO_BIN_EXE_aio-asset-normalizer-cli");
fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}
fn value(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}
fn success(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {} stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    value(output)
}
fn job(dir: &Path, filename: &str, request: &Value) -> std::path::PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, serde_json::to_vec_pretty(request).unwrap()).unwrap();
    path
}
fn material_fixture(path: &Path, unknown_extension: bool) {
    use std::borrow::Cow;
    let bytes = support::glb_bytes();
    let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let mut document: Value = serde_json::from_slice(&glb.json).unwrap();
    document["materials"] =
        json!([{"name":"Shared","extras":{"preserve":"material"}}]);
    document["meshes"][0]["primitives"][0]["material"] = json!(0);
    let primitive = document["meshes"][0]["primitives"][0].clone();
    document["meshes"][0]["primitives"]
        .as_array_mut()
        .unwrap()
        .push(primitive);
    document["extras"] = json!({"preserve":"document"});
    if unknown_extension {
        document["extensionsUsed"] = json!(["VENDOR_custom"]);
        document["extensions"] = json!({"VENDOR_custom":{"payload":[1,2,3]}});
    }
    let mut json_bytes = serde_json::to_vec(&document).unwrap();
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let output = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(json_bytes),
        bin: glb.bin,
    }
    .to_vec()
    .unwrap();
    std::fs::write(path, output).unwrap();
}
#[test]
fn omitted_edits_are_identity_and_match_explicit_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let (source, _) = support::write_fixtures(dir.path());
    let omitted = dir.path().join("omitted.glb");
    let empty = dir.path().join("empty.glb");
    let first = job(
        dir.path(),
        "first.json",
        &json!({"input":source,"output":omitted}),
    );
    let second = job(
        dir.path(),
        "second.json",
        &json!({"input":source,"output":empty,"edits":{}}),
    );
    success(&run(&["glb", "edit", "--job", first.to_str().unwrap()]));
    success(&run(&["glb", "edit", "--job", second.to_str().unwrap()]));
    assert_eq!(
        std::fs::read(omitted).unwrap(),
        std::fs::read(empty).unwrap()
    );
    assert_eq!(EditSpec::default().scale_roots, 1.0);
}
#[test]
fn glb_texture_selection_and_split_match_desktop_adapters() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("model with spaces.glb");
    material_fixture(&source, false);
    let image = dir.path().join("replacement.png");
    image::save_buffer(
        &image,
        &[255, 0, 0, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let cli_out = dir.path().join("cli/result.glb");
    let gui_out = dir.path().join("gui/result.glb");
    let pending = ExportEdits {
        orientation_euler_degrees: [0.0, 90.0, 0.0],
        root_scale: 2.0,
        root_translation: [1.0, 2.0, 3.0],
        trim: Some((0, 0.0, 0.9)),
        animation_rate: Some((0, 1.5)),
        smart_loop: Some((0, 0.2)),
        ..Default::default()
    };
    let mut edits = EditSpec::from_export_edits(&pending);
    assert_eq!(edits.export_edits(), pending);
    edits.textures.push(operations::spec::TextureReplacement {
        mesh: 0,
        primitive: 0,
        slot: operations::spec::TextureSlotSpec::BaseColor,
        image: image.clone(),
        duplicate_shared_material: true,
    });
    let document = GlbDocument::load(&source).unwrap();
    let edited = operations::glb::edit_snapshot(&document, &edits).unwrap();
    let mut selection = edited.default_export_selection().unwrap();
    selection.preset = GlbExportPreset::CharacterPackage;
    selection
        .selected_primitives
        .insert(0, std::collections::BTreeSet::from([0]));
    selection.animation_output =
        aio_asset_normalizer::modules::glb::AnimationOutputMode::Split;
    let spec = SelectionSpec::from_selection(&selection);
    assert_eq!(spec.resolve(&edited).unwrap(), selection);
    let request =
        json!({"input":source,"output":cli_out,"edits":edits,"selection":spec});
    let request_path = job(dir.path(), "edit.json", &request);
    let dry = success(&run(&[
        "glb",
        "edit",
        "--job",
        request_path.to_str().unwrap(),
        "--dry-run",
    ]));
    assert!(!dir.path().join("cli").exists());
    let cli = success(&run(&[
        "glb",
        "edit",
        "--job",
        request_path.to_str().unwrap(),
    ]));
    let jobs = operations::glb::prepare_export(
        &edited,
        &pending,
        &selection,
        &gui_out,
        &[&source],
        false,
    )
    .unwrap();
    assert_eq!(
        jobs.len(),
        cli["results"]["outputs"].as_array().unwrap().len()
    );
    for (index, gui_job) in jobs.iter().enumerate() {
        let report = operations::glb::execute_job(gui_job, false).unwrap();
        let cli_path = Path::new(
            cli["results"]["outputs"][index]["path"].as_str().unwrap(),
        );
        assert_eq!(
            std::fs::read(cli_path).unwrap(),
            std::fs::read(&gui_job.path).unwrap()
        );
        assert_eq!(json!(report), cli["results"]["outputs"][index]["report"]);
        assert_eq!(
            dry["results"]["outputs"][index]["report"],
            cli["results"]["outputs"][index]["report"]
        );
    }
    assert_eq!(document.summary().images, 0);
}
#[test]
fn invalid_edit_combinations_and_job_conflicts_fail_consistently() {
    let dir = tempfile::tempdir().unwrap();
    let (source, _) = support::write_fixtures(dir.path());
    let request = job(
        dir.path(),
        "bad.json",
        &json!({"input":source,"output":dir.path().join("out.glb"),"edits":{"smart_loop":{"animation":0,"transition_seconds":0.2}},"export":{"preset":"character","remove_root_motion":true}}),
    );
    let output = run(&[
        "glb",
        "edit",
        "--job",
        request.to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(3));
    let request: EditJobFile = operations::load_json(&request, "edit").unwrap();
    let document = operations::glb::edit_snapshot(
        &GlbDocument::load(&source).unwrap(),
        &request.edits,
    )
    .unwrap();
    let selection = operations::glb::resolve_selection(
        &document,
        request.export.as_ref(),
        None,
    )
    .unwrap();
    let error = operations::glb::validate_edits(
        &selection,
        &request.edits.export_edits(),
    )
    .unwrap_err();
    assert_eq!(value(&output)["error"]["message"], json!(error.message));
    let conflicts = run(&[
        "glb",
        "export",
        "--job",
        dir.path().join("bad.json").to_str().unwrap(),
        "--preset",
        "skeleton",
    ]);
    assert_eq!(conflicts.status.code(), Some(2));
    assert_eq!(value(&conflicts)["error"]["code"], "usage");
}
#[test]
fn bvh_trim_output_matches_desktop_processing() {
    let dir = tempfile::tempdir().unwrap();
    let (_, source) = support::write_fixtures(dir.path());
    let cli_out = dir.path().join("cli.bvh");
    let gui_out = dir.path().join("gui.bvh");
    success(&run(&[
        "bvh",
        "process",
        source.to_str().unwrap(),
        "--output",
        cli_out.to_str().unwrap(),
        "--trim",
        "0",
        "0.034",
    ]));
    let document =
        aio_asset_normalizer::modules::bvh::BvhDocument::load(&source).unwrap();
    let edited =
        operations::bvh::prepare(&document, Some([0.0, 0.034])).unwrap();
    operations::bvh::write(&edited, &gui_out, &source, false).unwrap();
    assert_eq!(
        std::fs::read(cli_out).unwrap(),
        std::fs::read(gui_out).unwrap()
    );
}
fn mapping_for(
    source: &rt::Source,
    target: &rt::Target,
) -> aio_asset_normalizer::modules::retarget::SkeletonMapping {
    use aio_asset_normalizer::modules::retarget::{
        MappingBone, SkeletonMapping,
    };
    let descriptor = source.descriptor();
    let mut mapping = SkeletonMapping::new(
        descriptor.endpoint(),
        target.descriptor.endpoint(),
    );
    mapping.bones.push(MappingBone {
        source: descriptor.node_ref(descriptor.root),
        target: target.descriptor.node_ref(target.descriptor.root),
        rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
    });
    for index in &descriptor.animated_nodes {
        if *index == descriptor.root {
            continue;
        }
        if let Some(node) = target.descriptor.nodes.iter().find(|node| {
            node.name == descriptor.nodes[*index].name && node.is_skin_joint
        }) {
            mapping.bones.push(MappingBone {
                source: descriptor.node_ref(*index),
                target: target.descriptor.node_ref(node.index),
                rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
            });
        } else {
            mapping.ignored_sources.push(descriptor.node_ref(*index));
        }
    }
    mapping
}
#[test]
fn bvh_and_edited_glb_retarget_match_snapshot_export_adapter() {
    for use_bvh in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let (target, bvh) = support::write_fixtures(dir.path());
        let source = if use_bvh {
            bvh
        } else {
            let p = dir.path().join("source.glb");
            std::fs::copy(&target, &p).unwrap();
            p
        };
        let mut request = rt::RetargetJob::new(source.clone(), target.clone());
        request.out = Some(dir.path().join("cli.glb"));
        request.sample_rate = 30.0;
        request.normalize_heading = true;
        request.root_motion = false;
        request.reduce_keys = Some(0.001);
        if !use_bvh {
            request.source_edits = EditSpec::from_export_edits(&ExportEdits {
                trim: Some((0, 0.0, 0.9)),
                animation_rate: Some((0, 1.5)),
                root_scale: 1.5,
                orientation_euler_degrees: [0.0, 45.0, 0.0],
                bake_root_transform: false,
                ..Default::default()
            });
        }
        let prepared_source = rt::load_source(&request).unwrap();
        let prepared_target = rt::load_target(&request).unwrap();
        let mapping = mapping_for(&prepared_source, &prepared_target);
        let mapping_path = dir.path().join("mapping.json");
        std::fs::write(
            &mapping_path,
            serde_json::to_vec_pretty(&mapping).unwrap(),
        )
        .unwrap();
        request.mapping = Some(mapping_path);
        let request_path = job(dir.path(), "retarget.json", &json!(request));
        let prompt_request = rt::RetargetJob {
            out: Some(dir.path().join("prompt.md")),
            ..request.clone()
        };
        let prompt_path =
            job(dir.path(), "prompt.json", &json!(prompt_request));
        let prompt = success(&run(&[
            "retarget",
            "prompt",
            "--job",
            prompt_path.to_str().unwrap(),
        ]));
        let context = support::extract_first_json_block(
            prompt["results"]["prompt"].as_str().unwrap(),
        );
        assert_eq!(
            context["source"]["skeleton_sha256"],
            prepared_source.descriptor().skeleton_sha256
        );
        success(&run(&[
            "retarget",
            "validate",
            "--job",
            request_path.to_str().unwrap(),
        ]));
        let dry = success(&run(&[
            "retarget",
            "run",
            "--job",
            request_path.to_str().unwrap(),
            "--dry-run",
        ]));
        assert!(!request.out.as_ref().unwrap().exists());
        let cli = success(&run(&[
            "retarget",
            "run",
            "--job",
            request_path.to_str().unwrap(),
        ]));
        let mut selection =
            prepared_target.document.default_export_selection().unwrap();
        selection.preset = GlbExportPreset::CharacterPackage;
        let gui_request = rt::RetargetJob::for_snapshot_export(
            source.clone(),
            target.clone(),
            dir.path().join("gui.glb"),
            rt::SnapshotExportSettings {
                source_skin: request.source_skin,
                target_skin: request.skin,
                source_axes: [
                    request.source_up_axis.clone(),
                    request.source_forward_axis.clone(),
                    request.source_unit.clone().unwrap_or_else(|| {
                        if use_bvh {
                            "cm".into()
                        } else {
                            "m".into()
                        }
                    }),
                ],
                edits: request.source_edits.clone(),
                options: request.options().unwrap(),
                clip_name: request.clip_name.clone(),
                selection,
                reduce_keys: request.reduce_keys,
                overwrite: false,
            },
        );
        assert_eq!(gui_request.options().unwrap(), request.options().unwrap());
        assert_eq!(
            json!(gui_request.source_edits),
            json!(request.source_edits)
        );
        let gui_source = match &prepared_source {
            rt::Source::Bvh { document, .. } => {
                rt::bvh_source(document.clone(), ["Y", "-Z", "cm"]).unwrap()
            }
            rt::Source::Glb { document, .. } => rt::glb_source(
                document,
                source.parent(),
                request.source_animation,
                request.source_skin,
                &Default::default(),
                ["Y", "-Z", "m"],
            )
            .unwrap(),
        };
        assert_eq!(
            gui_source.descriptor().context_value(),
            prepared_source.descriptor().context_value()
        );
        let gui_job =
            rt::prepare(&gui_source, &prepared_target, &mapping, &gui_request)
                .unwrap();
        let report = operations::glb::execute_job(&gui_job, false).unwrap();
        assert_eq!(
            std::fs::read(&gui_job.path).unwrap(),
            std::fs::read(request.out.as_ref().unwrap()).unwrap()
        );
        assert_eq!(dry["results"]["report"], cli["results"]["report"]);
        for (key, expected) in json!(&report).as_object().unwrap() {
            assert_eq!(&cli["results"]["report"][key], expected);
        }
        assert_eq!(
            cli["results"]["report"]["output_glb_bytes"],
            json!(report.output_glb_bytes)
        );
    }
}
#[test]
fn capabilities_schemas_usage_and_io_are_machine_readable() {
    let capabilities = success(&run(&["capabilities"]));
    assert!(capabilities["results"]["features"]
        .as_array()
        .unwrap()
        .contains(&json!("texture-replacement")));
    for command in [
        "glb.edit",
        "glb.export",
        "bvh.process",
        "retarget.prompt",
        "retarget.validate",
        "retarget.run",
        "fbx.convert",
    ] {
        let schema = success(&run(&["schema", "--command", command]));
        assert_eq!(schema["results"]["schema"]["additionalProperties"], false);
    }
    let unknown = run(&["not-a-command"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert_eq!(value(&unknown)["error"]["code"], "usage");
    let dir = tempfile::tempdir().unwrap();
    let missing = run(&[
        "glb",
        "edit",
        "--job",
        dir.path().join("missing.json").to_str().unwrap(),
    ]);
    assert_eq!(missing.status.code(), Some(4));
    assert_eq!(value(&missing)["error"]["code"], "io");
    let batch = run(&[
        "glb",
        "export",
        dir.path().join("missing.glb").to_str().unwrap(),
        "--output-root",
        dir.path().join("out").to_str().unwrap(),
    ]);
    assert_eq!(batch.status.code(), Some(4));
}
#[test]
fn recursive_runs_exclude_previous_outputs() {
    let dir = tempfile::tempdir().unwrap();
    support::write_fixtures(dir.path());
    let out = dir.path().join("out");
    let args = [
        "glb",
        "export",
        "--input-root",
        dir.path().to_str().unwrap(),
        "--recursive",
        "--output-root",
        out.to_str().unwrap(),
    ];
    success(&run(&args));
    let mut args = args.to_vec();
    args.push("--overwrite");
    let second = success(&run(&args));
    assert_eq!(second["results"]["entries"].as_array().unwrap().len(), 1);
}
#[test]
fn preserve_all_keeps_unknown_extensions_and_compaction_fails_safely() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("custom.glb");
    material_fixture(&source, true);
    let request = job(
        dir.path(),
        "preserve.json",
        &json!({"input":source,"output":dir.path().join("preserved.glb")}),
    );
    success(&run(&["glb", "edit", "--job", request.to_str().unwrap()]));
    let bytes = std::fs::read(dir.path().join("preserved.glb")).unwrap();
    let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let document: Value = serde_json::from_slice(&glb.json).unwrap();
    assert_eq!(
        document["extensions"]["VENDOR_custom"]["payload"],
        json!([1, 2, 3])
    );
    assert_eq!(document["extras"]["preserve"], "document");
    let request = job(
        dir.path(),
        "compact.json",
        &json!({"input":source,"output":dir.path().join("compact.glb"),"export":{"preset":"character"}}),
    );
    let output = run(&["glb", "edit", "--job", request.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(3));
    assert!(!dir.path().join("compact.glb").exists());
}
#[test]
fn source_replacement_and_invalid_mapping_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (source, bvh) = support::write_fixtures(dir.path());
    let original = std::fs::read(&source).unwrap();
    let request = job(
        dir.path(),
        "source.json",
        &json!({"input":source,"output":source,"overwrite":true}),
    );
    assert_eq!(
        run(&["glb", "edit", "--job", request.to_str().unwrap()])
            .status
            .code(),
        Some(3)
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let mapping = job(dir.path(), "bad-mapping.json", &json!({"version":2}));
    let output = run(&[
        "retarget",
        "run",
        "--source",
        bvh.to_str().unwrap(),
        "--target",
        source.to_str().unwrap(),
        "--mapping",
        mapping.to_str().unwrap(),
        "--out",
        dir.path().join("bad.glb").to_str().unwrap(),
        "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(3));
    assert!(!dir.path().join("bad.glb").exists());
}

#[test]
fn partial_batch_io_failure_keeps_completed_outputs() {
    use aio_asset_normalizer::modules::glb::{
        batch_runner::{self, BatchProgress, BatchRequest},
        GlbBatchRecipe,
    };
    let dir = tempfile::tempdir().unwrap();
    let input_root = dir.path().join("inputs");
    std::fs::create_dir_all(input_root.join("second")).unwrap();
    let first = input_root.join("first.glb");
    let second = input_root.join("second/model.glb");
    std::fs::write(&first, support::glb_bytes()).unwrap();
    std::fs::write(&second, support::glb_bytes()).unwrap();
    let output_root = dir.path().join("outputs");
    let request = BatchRequest {
        inputs: vec![first, second],
        input_root,
        output_root: output_root.clone(),
        overwrite_existing: false,
        recipe: GlbBatchRecipe::default(),
        selection: None,
    };
    let (entries, valid) =
        batch_runner::run_preflight(&request, 1, &mut |_| {});
    assert!(valid);
    assert!(entries.iter().all(|e| e.error.is_none()));
    let mut completed = Vec::new();
    let result =
        batch_runner::run_export(&request, &entries, 2, &mut |event| {
            if let BatchProgress::ExportFinished {
                index,
                completed: paths,
                ..
            } = event
            {
                completed.extend(paths);
                if index == 0 {
                    std::fs::write(
                        output_root.join("second"),
                        b"blocks directory creation",
                    )
                    .unwrap();
                }
            }
        });
    assert_eq!(result.unwrap_err().kind, operations::ErrorKind::Io);
    assert_eq!(completed.len(), 1);
    assert!(completed[0].is_file());
}
#[test]
fn malformed_binary_ranges_and_non_finite_values_fail_before_writes() {
    use std::borrow::Cow;
    let dir = tempfile::tempdir().unwrap();
    for finite in [false, true] {
        let source =
            dir.path()
                .join(if finite { "range.glb" } else { "nonfinite.glb" });
        let bytes = support::glb_bytes();
        let parsed = gltf::binary::Glb::from_slice(&bytes).unwrap();
        let mut data: Value = serde_json::from_slice(&parsed.json).unwrap();
        let mut bin = parsed.bin.unwrap().into_owned();
        if finite {
            data["accessors"][0]["count"] = json!(1_000_000);
        } else {
            bin[..4].copy_from_slice(&f32::NAN.to_le_bytes());
        }
        let mut encoded = serde_json::to_vec(&data).unwrap();
        while encoded.len() % 4 != 0 {
            encoded.push(b' ');
        }
        let malformed = gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(encoded),
            bin: Some(Cow::Owned(bin)),
        }
        .to_vec()
        .unwrap();
        std::fs::write(&source, malformed).unwrap();
        let out = dir.path().join("out.glb");
        let file = job(
            dir.path(),
            "invalid.json",
            &json!({"input":source,"output":out}),
        );
        let result = run(&["glb", "edit", "--job", file.to_str().unwrap()]);
        assert_eq!(result.status.code(), Some(3));
        assert_eq!(value(&result)["error"]["code"], "validation");
        assert!(!out.exists());
        assert!(GlbDocument::load(&source).is_err());
    }
}

#[test]
fn bvh_job_matches_flags_and_rejects_task_flag_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    let (_, source) = support::write_fixtures(dir.path());
    let output = dir.path().join("job.bvh");
    let file = job(
        dir.path(),
        "bvh.json",
        &json!({"command":"bvh.process","input":source,"output":output,"trim":[0,0.034]}),
    );
    let dry = success(&run(&[
        "bvh",
        "process",
        "--job",
        file.to_str().unwrap(),
        "--dry-run",
    ]));
    assert_eq!(dry["results"]["written"], false);
    assert!(!output.exists());
    success(&run(&["bvh", "process", "--job", file.to_str().unwrap()]));
    let flags_output = dir.path().join("flags.bvh");
    success(&run(&[
        "bvh",
        "process",
        source.to_str().unwrap(),
        "--output",
        flags_output.to_str().unwrap(),
        "--trim",
        "0",
        "0.034",
    ]));
    assert_eq!(
        std::fs::read(output).unwrap(),
        std::fs::read(flags_output).unwrap()
    );
    let conflict = run(&[
        "bvh",
        "process",
        "--job",
        file.to_str().unwrap(),
        "--overwrite",
    ]);
    assert_eq!(conflict.status.code(), Some(2));
    assert_eq!(value(&conflict)["error"]["code"], "usage");
}
