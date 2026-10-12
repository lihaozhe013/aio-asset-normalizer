//! Integration coverage for the shared asset operations used by the GUI and MCP.
use aio_asset_normalizer::modules::{
    glb::{
        batch_runner::{self, BatchProgress, BatchRequest},
        pipeline::ExportEdits,
        AnimationOutputMode, GlbBatchRecipe, GlbDocument, GlbExportPreset,
    },
    operations::{self, spec::EditSpec},
};
use serde_json::{json, Value};
use std::path::Path;
mod support;

fn material_fixture(path: &Path, unknown_extension: bool) {
    use std::borrow::Cow;
    let bytes = support::glb_bytes();
    let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let mut document: Value = serde_json::from_slice(&glb.json).unwrap();
    document["materials"] = json!([{"name":"Shared","extras":{"preserve":"material"}}]);
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
    let document = GlbDocument::load(&source).unwrap();
    let omitted = operations::glb::edit_snapshot(&document, &EditSpec::default()).unwrap();
    let explicit_spec: EditSpec = serde_json::from_value(json!({})).unwrap();
    let explicit = operations::glb::edit_snapshot(&document, &explicit_spec).unwrap();

    assert_eq!(omitted.to_bytes().unwrap(), explicit.to_bytes().unwrap());
    assert_eq!(EditSpec::default().scale_roots, 1.0);
}

#[test]
fn texture_edits_and_split_selection_produce_valid_exports() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("model with spaces.glb");
    material_fixture(&source, false);
    let image = dir.path().join("replacement.png");
    image::save_buffer(&image, &[255, 0, 0, 255], 1, 1, image::ColorType::Rgba8).unwrap();
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
    edits.textures.push(operations::spec::TextureReplacement {
        mesh: 0,
        primitive: 0,
        slot: operations::spec::TextureSlotSpec::BaseColor,
        image,
        duplicate_shared_material: true,
    });
    assert_eq!(edits.export_edits(), pending);

    let document = GlbDocument::load(&source).unwrap();
    let edited = operations::glb::edit_snapshot(&document, &edits).unwrap();
    let mut selection = edited.default_export_selection().unwrap();
    selection.preset = GlbExportPreset::CharacterPackage;
    selection
        .selected_primitives
        .insert(0, std::collections::BTreeSet::from([0]));
    selection.animation_output = AnimationOutputMode::Split;
    let output = dir.path().join("exports/model.glb");
    let jobs =
        operations::glb::prepare_export(&edited, &pending, &selection, &output, &[&source], false)
            .unwrap();

    assert!(!output.parent().unwrap().exists());
    assert!(!jobs.is_empty());
    for job in jobs {
        operations::glb::execute_job(&job, false).unwrap();
        assert!(GlbDocument::load(&job.path).is_ok());
    }
    assert_eq!(document.summary().images, 0);
}

#[test]
fn incompatible_edit_combinations_are_rejected_before_export() {
    let dir = tempfile::tempdir().unwrap();
    let (source, _) = support::write_fixtures(dir.path());
    let document = GlbDocument::load(&source).unwrap();
    let edits = EditSpec::from_export_edits(&ExportEdits {
        smart_loop: Some((0, 0.2)),
        ..Default::default()
    });
    let mut selection = document.default_export_selection().unwrap();
    selection.preset = GlbExportPreset::CharacterPackage;
    selection.remove_root_motion = true;
    let error = operations::glb::validate_edits(&selection, &edits.export_edits()).unwrap_err();

    assert_eq!(error.kind, operations::ErrorKind::Validation);
    assert!(error.message.contains("cannot be combined"));
    assert!(!dir.path().join("out.glb").exists());
}

#[test]
fn bvh_trim_is_reparsed_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let (_, source) = support::write_fixtures(dir.path());
    let document = aio_asset_normalizer::modules::bvh::BvhDocument::load(&source).unwrap();
    let edited = operations::bvh::prepare(&document, Some([0.0, 0.034])).unwrap();
    let output = dir.path().join("trimmed.bvh");
    let text = operations::bvh::validate_output(&edited, &output, &source, false).unwrap();
    let reparsed = aio_asset_normalizer::modules::bvh::BvhDocument::parse(&text).unwrap();
    assert_eq!(edited.frames, reparsed.frames);

    operations::bvh::write(&edited, &output, &source, false).unwrap();
    assert!(aio_asset_normalizer::modules::bvh::BvhDocument::load(&output).is_ok());
}

#[test]
fn preserve_all_keeps_unknown_extensions_and_compaction_fails_safely() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("custom.glb");
    material_fixture(&source, true);
    let document = GlbDocument::load(&source).unwrap();
    let selection = document.default_export_selection().unwrap();
    let output = dir.path().join("preserved.glb");
    let jobs = operations::glb::prepare_export(
        &document,
        &ExportEdits::default(),
        &selection,
        &output,
        &[&source],
        false,
    )
    .unwrap();
    for job in jobs {
        operations::glb::execute_job(&job, false).unwrap();
    }

    let bytes = std::fs::read(&output).unwrap();
    let glb = gltf::binary::Glb::from_slice(&bytes).unwrap();
    let value: Value = serde_json::from_slice(&glb.json).unwrap();
    assert_eq!(
        value["extensions"]["VENDOR_custom"]["payload"],
        json!([1, 2, 3])
    );
    assert_eq!(value["extras"]["preserve"], "document");

    let mut compact = selection;
    compact.preset = GlbExportPreset::CharacterPackage;
    let error = operations::glb::prepare_export(
        &document,
        &ExportEdits::default(),
        &compact,
        &dir.path().join("compact.glb"),
        &[&source],
        false,
    )
    .unwrap_err();
    assert_eq!(error.kind, operations::ErrorKind::Validation);
    assert!(!dir.path().join("compact.glb").exists());
}

#[test]
fn source_replacement_is_rejected_even_when_overwrite_is_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let (source, _) = support::write_fixtures(dir.path());
    let original = std::fs::read(&source).unwrap();
    let error = operations::check_output(&source, &[&source], true).unwrap_err();

    assert_eq!(error.kind, operations::ErrorKind::Validation);
    assert_eq!(std::fs::read(&source).unwrap(), original);
}

#[test]
fn batch_preflight_and_export_write_the_planned_output() {
    let dir = tempfile::tempdir().unwrap();
    let (source, _) = support::write_fixtures(dir.path());
    let output_root = dir.path().join("out");
    let request = BatchRequest {
        inputs: vec![source],
        input_root: dir.path().to_path_buf(),
        output_root: output_root.clone(),
        overwrite_existing: false,
        recipe: GlbBatchRecipe::default(),
        selection: None,
    };
    let (entries, valid) = batch_runner::run_preflight(&request, 1, &mut |_| {});
    assert!(valid);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].error.is_none());

    let output_path = entries[0].outputs[0].path.clone();
    batch_runner::run_export(&request, &entries, 2, &mut |_| {}).unwrap();
    assert!(output_path.is_file());
}

#[test]
fn partial_batch_failure_keeps_completed_outputs() {
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
    let (entries, valid) = batch_runner::run_preflight(&request, 1, &mut |_| {});
    assert!(valid);
    assert!(entries.iter().all(|entry| entry.error.is_none()));
    let mut completed = Vec::new();
    let result = batch_runner::run_export(&request, &entries, 2, &mut |event| {
        if let BatchProgress::ExportFinished {
            index,
            completed: paths,
            ..
        } = event
        {
            completed.extend(paths);
            if index == 0 {
                std::fs::write(output_root.join("second"), b"blocks directory creation").unwrap();
            }
        }
    });

    assert_eq!(result.unwrap_err().kind, operations::ErrorKind::Io);
    assert_eq!(completed.len(), 1);
    assert!(completed[0].is_file());
}

#[test]
fn malformed_glb_ranges_and_non_finite_values_fail_before_writes() {
    use std::borrow::Cow;
    let dir = tempfile::tempdir().unwrap();
    for finite in [false, true] {
        let source = dir
            .path()
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

        assert!(GlbDocument::load(&source).is_err());
        assert!(!dir.path().join("out.glb").exists());
    }
}
