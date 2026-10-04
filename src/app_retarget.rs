use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::mpsc;

use crate::app::{App, ExportTaskResult, TaskKind};
use crate::app_export::format_export_report;
use crate::modules::glb::{
    AnimationClipData, AnimationOutputMode, AnimationRuntime, GlbDocument,
    GlbExportPreset,
};
use crate::modules::logging::{next_task_id, safe_path_label};
use crate::modules::retarget::{
    self, RetargetOptions, SkeletonDescriptor, SourceKind,
};
use crate::modules::retarget_export;
use three_d::Context;

impl App {
    pub(crate) fn initialize_glb_retarget_preview_skeleton(
        &mut self,
        context: &Context,
    ) {
        let Some(target) = self.glb_retarget_target.as_ref() else {
            return;
        };
        let Ok(skin) = target.skin_data_at(self.retarget_target_skin_index)
        else {
            return;
        };
        if let Err(error) = self.canvas.update_target_skeleton_animation(
            context,
            0,
            0.0,
            &skin.joints,
        ) {
            tracing::error!(
                target: "glb_retarget",
                error = %error,
                "Target skeleton preview failed"
            );
        }
        self.canvas.set_guide_scale(
            context,
            self.canvas
                .target_skeleton
                .as_ref()
                .map(|skeleton| skeleton.metrics().height)
                .unwrap_or(1.0),
        );
    }

    pub(crate) fn import_glb_retarget_target(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB", &["glb"])
            .pick_file()
        else {
            return;
        };
        match GlbDocument::load(&path) {
            Ok(document) => {
                let mut export_selection =
                    document.default_export_selection().unwrap_or_default();
                export_selection.preset = GlbExportPreset::CharacterPackage;
                self.glb_retarget_target = Some(document);
                self.glb_retarget_target_path = Some(path.clone());
                self.glb_retarget_export_selection = export_selection;
                self.retarget_target_skin_index = 0;
                self.refresh_glb_retarget_mapping();
                tracing::info!(
                    target: "glb_retarget",
                    input = %safe_path_label(&path),
                    "Loaded target GLB"
                );
            }
            Err(error) => tracing::error!(
                target: "glb_retarget",
                error = %error,
                "Target GLB load failed"
            ),
        }
    }

    pub(crate) fn refresh_glb_retarget_mapping(&mut self) {
        let (Some(source), Some(target), Some(mapping)) = (
            self.glb.as_ref(),
            self.glb_retarget_target.as_ref(),
            self.retarget_mapping.as_ref(),
        ) else {
            self.retarget_validation = None;
            return;
        };
        let report = (|| {
            let edits =
                crate::modules::operations::spec::EditSpec::from_export_edits(
                    &self.glb_export_edits(false),
                );
            let source = crate::modules::operations::retarget::glb_source(
                source,
                self.glb_path.as_deref().and_then(Path::parent),
                self.glb_animation_index,
                self.retarget_source_skin_index,
                &edits,
                ["Y", "-Z", "m"],
            )?;
            let target = crate::modules::operations::retarget::target(
                target.clone(),
                self.retarget_target_skin_index,
                crate::modules::operations::retarget::document_fingerprint(
                    target,
                )?,
                ["Y", "-Z", "m"],
            )?;
            Ok::<_, crate::modules::operations::OperationError>(
                crate::modules::operations::retarget::validate(
                    &source, &target, mapping,
                ),
            )
        })();
        self.retarget_validation =
            Some(report.unwrap_or_else(|e| invalid_report(e.to_string())));
    }

    pub(crate) fn retarget_options(&self) -> RetargetOptions {
        RetargetOptions {
            root_motion: self.retarget_root_motion,
            normalize_initial_heading: self.retarget_normalize_initial_heading,
            ..RetargetOptions::default()
        }
    }

    fn glb_retarget_options(&self) -> Result<RetargetOptions, String> {
        let mut options = self.retarget_options();
        options.source_root_rotation =
            retarget::euler_rotation_quaternion(self.orientation_euler_degrees)
                .map_err(|error| error.to_string())?;
        options.source_root_scale = self.root_scale;
        options.source_root_translation = self.root_translation;
        Ok(options)
    }

    pub(crate) fn refresh_v2_retarget_mapping(&mut self) {
        let Some(bvh) = self.bvh.as_ref() else {
            self.retarget_validation = None;
            return;
        };
        let Some(target) = self.bvh_target_glb.as_ref() else {
            self.retarget_validation = None;
            return;
        };
        let context = (|| {
            let source = crate::modules::operations::retarget::bvh_source(
                bvh.clone(),
                [&self.bvh_up_axis, &self.bvh_forward_axis, &self.bvh_unit],
            )?;
            let target = crate::modules::operations::retarget::target(
                target.clone(),
                self.retarget_target_skin_index,
                crate::modules::operations::retarget::document_fingerprint(
                    target,
                )?,
                ["Y", "-Z", "m"],
            )?;
            Ok::<_, crate::modules::operations::OperationError>((
                source, target,
            ))
        })();
        let (source, target) = match context {
            Ok(v) => v,
            Err(e) => {
                self.retarget_validation = Some(invalid_report(e.to_string()));
                return;
            }
        };
        let source_descriptor = source.descriptor();
        let target_descriptor = &target.descriptor;
        let mapping = if let Some(mapping) = self.retarget_mapping.clone() {
            mapping
        } else if let Some(legacy) = self.mapping.as_ref() {
            match retarget::from_legacy_bvh_mapping(
                legacy,
                source_descriptor,
                target_descriptor,
            ) {
                Ok(mapping) => mapping,
                Err(error) => {
                    self.retarget_validation =
                        Some(invalid_report(error.to_string()));
                    return;
                }
            }
        } else {
            self.retarget_validation = None;
            return;
        };
        let report = retarget::validate_mapping(
            &mapping,
            source_descriptor,
            target_descriptor,
        );
        self.retarget_mapping = Some(mapping);
        self.retarget_validation = Some(report);
        self.needs_bvh_target_reload = true;
    }

    pub(crate) fn export_bvh_glb(&mut self, clip_only: bool) {
        if self.task_busy {
            return;
        }
        let Some(source) = self.bvh.clone() else {
            tracing::warn!(
                target: "bvh_studio",
                "Open a BVH before exporting GLB"
            );
            return;
        };
        self.refresh_v2_retarget_mapping();
        let Some(mapping) = self.retarget_mapping.clone() else {
            tracing::warn!(
                target: "bvh_studio",
                "Load a valid Mapping JSON before exporting GLB"
            );
            return;
        };
        if self
            .retarget_validation
            .as_ref()
            .is_none_or(|report| !report.is_valid())
        {
            tracing::error!(
                target: "bvh_studio",
                "Mapping v2 validation failed"
            );
            return;
        }
        let Some(target) = self.bvh_target_glb.clone() else {
            tracing::warn!(
                target: "bvh_studio",
                "Load a target GLB before exporting GLB"
            );
            return;
        };
        let mut export_selection = self.bvh_export_selection.clone();
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB", &["glb"])
            .set_file_name(if clip_only {
                "bvh_animation_clip.glb"
            } else {
                "bvh_retargeted.glb"
            })
            .save_file()
        else {
            return;
        };
        if same_path(&path, self.bvh_target_path.as_deref())
            || same_path(&path, self.bvh_path.as_deref())
        {
            tracing::error!(
                target: "bvh_studio",
                output = %safe_path_label(&path),
                "Refusing to overwrite a source BVH or target GLB"
            );
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.task_rx = Some(receiver);
        self.task_busy = true;
        let task_id = next_task_id();
        self.active_task_id = Some(task_id);
        tracing::info!(
            target: "bvh_studio",
            task_id,
            clip_only,
            "Building retargeted GLB in background"
        );
        let reduce_keys = self.bvh_reduce_keys;
        let key_tolerance = self.bvh_key_tolerance;
        let options = self.retarget_options();
        let target_skin_index = self.retarget_target_skin_index;
        export_selection.skin_index = Some(target_skin_index);
        export_selection.preset = if clip_only {
            GlbExportPreset::SkeletonAnimation
        } else {
            GlbExportPreset::CharacterPackage
        };
        export_selection.selected_animations = BTreeSet::from([0]);
        export_selection.animation_output = AnimationOutputMode::Combined;
        let request = crate::modules::operations::retarget::RetargetJob::for_snapshot_export(
            self.bvh_path.clone().unwrap_or_default(), self.bvh_target_path.clone().unwrap_or_default(), path.clone(),
            crate::modules::operations::retarget::SnapshotExportSettings {
                source_skin: 0, target_skin: target_skin_index, source_axes: [self.bvh_up_axis.clone(),self.bvh_forward_axis.clone(),self.bvh_unit.clone()],
                edits: Default::default(), options, clip_name: "BVH Retarget".into(), selection: export_selection,
                reduce_keys: reduce_keys.then_some(key_tolerance), overwrite: self.output_overwrite,
            });
        std::thread::spawn(move || {
            let mut paths = Vec::new();
            let mut details = Vec::new();
            let result = (|| {
                let source = crate::modules::operations::retarget::bvh_source(
                    source,
                    [
                        &request.source_up_axis,
                        &request.source_forward_axis,
                        request.source_unit.as_deref().unwrap_or("cm"),
                    ],
                )
                .map_err(|e| e.to_string())?;
                let fingerprint =
                    crate::modules::operations::retarget::document_fingerprint(
                        &target,
                    )
                    .map_err(|e| e.to_string())?;
                let target = crate::modules::operations::retarget::target(
                    target,
                    target_skin_index,
                    fingerprint,
                    ["Y", "-Z", "m"],
                )
                .map_err(|e| e.to_string())?;
                let job = crate::modules::operations::retarget::prepare(
                    &source, &target, &mapping, &request,
                )
                .map_err(|e| e.to_string())?;
                let report = crate::modules::operations::glb::execute_job(
                    &job,
                    request.overwrite,
                )
                .map_err(|e| e.to_string())?;
                details.push(format_export_report(&report));
                paths.push(path.clone());
                Ok(())
            })();
            let _ = sender.send(ExportTaskResult {
                task_id,
                kind: TaskKind::BvhExport,
                paths,
                details,
                result,
            });
        });
    }

    pub(crate) fn reload_bvh_target_preview(&mut self, context: &Context) {
        if !self.needs_bvh_target_reload {
            return;
        }
        self.needs_bvh_target_reload = false;
        let Some(path) = self.bvh_target_path.clone() else {
            self.canvas.clear_glb();
            self.canvas.clear_target_skeleton();
            return;
        };
        let Some(target) = self.bvh_target_glb.clone() else {
            return;
        };
        let skin_index = self.retarget_target_skin_index;
        let skin = match target.skin_data_at(skin_index) {
            Ok(skin) => skin,
            Err(error) => {
                tracing::warn!(
                    target: "bvh_studio",
                    error = %error,
                    "Target Skin preview unavailable"
                );
                self.canvas.clear_glb();
                self.canvas.clear_target_skeleton();
                return;
            }
        };
        let descriptor = match SkeletonDescriptor::from_skin(
            &skin,
            SourceKind::Glb,
            String::new(),
            String::new(),
            "Y",
            "-Z",
            "m",
            &HashSet::new(),
        ) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                tracing::warn!(
                    target: "bvh_studio",
                    error = %error,
                    "Target skeleton preview unavailable"
                );
                return;
            }
        };
        match descriptor.rest_world_transforms() {
            Ok(transforms) => {
                let positions =
                    transforms.iter().map(|value| value.0).collect::<Vec<_>>();
                let parents = descriptor
                    .nodes
                    .iter()
                    .map(|node| node.parent)
                    .collect::<Vec<_>>();
                self.canvas.set_target_skeleton_filtered(
                    context,
                    &positions,
                    &parents,
                    &skin.joints,
                );
                self.canvas.set_guide_scale(
                    context,
                    self.canvas
                        .target_skeleton
                        .as_ref()
                        .map(|skeleton| skeleton.metrics().height)
                        .unwrap_or(1.0),
                );
            }
            Err(error) => tracing::warn!(
                target: "bvh_studio",
                error = %error,
                "Target skeleton preview unavailable"
            ),
        }

        let Some(source) = self.bvh.clone() else {
            let _ = self.canvas.load_glb_for_target_preview(context, &path);
            return;
        };
        let Some(mapping) = self.retarget_mapping.clone() else {
            let _ = self.canvas.load_glb_for_target_preview(context, &path);
            return;
        };
        let report_valid = self
            .retarget_validation
            .as_ref()
            .is_some_and(|report| report.is_valid());
        if !report_valid {
            if let Err(error) =
                self.canvas.load_glb_for_target_preview(context, &path)
            {
                tracing::error!(
                    target: "bvh_studio",
                    error = %error,
                    "Target GLB preview failed"
                );
            }
            return;
        }
        let clip = match retarget::retarget_bvh(
            &source,
            &skin,
            &mapping,
            self.retarget_options(),
            "BVH Retarget",
        ) {
            Ok(mut clip) => {
                if self.bvh_reduce_keys {
                    if let Err(error) = clip.reduce_keys(self.bvh_key_tolerance)
                    {
                        tracing::warn!(
                            target: "retarget",
                            error = %error,
                            "Key reduction skipped"
                        );
                    }
                }
                clip
            }
            Err(error) => {
                tracing::error!(
                    target: "retarget",
                    error = %error,
                    "BVH preview failed"
                );
                if let Err(error) =
                    self.canvas.load_glb_for_target_preview(context, &path)
                {
                    tracing::error!(
                        target: "bvh_studio",
                        error = %error,
                        "Target GLB preview failed"
                    );
                }
                return;
            }
        };
        let mut generated = target;
        if let Err(error) = generated.replace_animations(&AnimationClipData {
            name: clip.name.clone(),
            times: clip.times.clone(),
            channels: clip.channels.clone(),
        }) {
            tracing::error!(
                target: "retarget",
                error = %error,
                "BVH preview animation failed"
            );
            return;
        }
        let bytes = match generated.to_bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::error!(
                    target: "retarget",
                    error = %error,
                    "BVH preview serialization failed"
                );
                return;
            }
        };
        match AnimationRuntime::from_bytes(&bytes, path.parent()) {
            Ok(runtime) => {
                if let Err(error) =
                    self.canvas.load_glb_with_runtime_for_target_preview(
                        context, &path, runtime,
                    )
                {
                    tracing::error!(
                        target: "bvh_studio",
                        error = %error,
                        "Target character preview failed"
                    );
                } else {
                    let _ = self.canvas.update_glb_animation(
                        0,
                        self.bvh_frame as f32 * source.frame_time,
                    );
                    tracing::info!(
                        target: "retarget",
                        "BVH target character preview ready"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    target: "bvh_studio",
                    error = %error,
                    "Target Mesh preview unavailable; using skeleton-only playback"
                );
                match AnimationRuntime::from_bytes_skeleton_only(
                    &bytes,
                    path.parent(),
                ) {
                    Ok(runtime) => {
                        self.canvas.load_skeleton_runtime(runtime);
                        if let Err(error) =
                            self.canvas.update_target_skeleton_animation(
                                context,
                                0,
                                self.bvh_frame as f32 * source.frame_time,
                                &skin.joints,
                            )
                        {
                            tracing::error!(
                                target: "retarget",
                                error = %error,
                                "Skeleton-only target preview failed"
                            );
                        }
                    }
                    Err(skeleton_error) => tracing::error!(
                        target: "retarget",
                        error = %skeleton_error,
                        "Skeleton-only target preview failed"
                    ),
                }
            }
        }
    }

    pub(crate) fn export_retarget_mapping(&mut self) {
        let Some(mapping) = self.retarget_mapping.as_ref() else {
            tracing::warn!(target: "retarget", "No Mapping v2 is available");
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Mapping JSON", &["json"])
            .set_file_name("skeleton-mapping-v2.json")
            .save_file()
        else {
            return;
        };
        let original_mapping_path = self
            .retarget_mapping_path
            .as_deref()
            .or(self.mapping_path.as_deref());
        if same_path(&path, original_mapping_path) {
            tracing::error!(
                target: "retarget",
                output = %safe_path_label(&path),
                "Refusing to overwrite the source mapping"
            );
            return;
        }
        let sources: Vec<_> = [
            self.bvh_path.as_deref(),
            self.bvh_target_path.as_deref(),
            self.glb_path.as_deref(),
            self.glb_retarget_target_path.as_deref(),
            original_mapping_path,
        ]
        .into_iter()
        .flatten()
        .collect();
        let result = (|| {
            crate::modules::operations::check_output(
                &path,
                &sources,
                self.output_overwrite,
            )
            .map_err(|e| e.to_string())?;
            let bytes = serde_json::to_vec_pretty(mapping)
                .map_err(|e| e.to_string())?;
            crate::modules::atomic_file::write(
                &path,
                &bytes,
                self.output_overwrite,
            )
            .map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => {
                self.file_tree.refresh();
                self.bvh_file_tree.refresh();
                tracing::info!(
                    target: "retarget",
                    output = %safe_path_label(&path),
                    "Exported Mapping v2"
                );
            }
            Err(error) => tracing::error!(
                target: "retarget",
                error = %error,
                "Mapping export failed"
            ),
        }
    }

    pub(crate) fn export_glb_retarget(&mut self) {
        if self.task_busy {
            return;
        }
        let Some(target) = self.glb_retarget_target.clone() else {
            tracing::warn!(
                target: "glb_retarget",
                "Choose a target GLB first"
            );
            return;
        };
        let Some(target_path) = self.glb_retarget_target_path.clone() else {
            return;
        };
        let Some(mapping) = self.retarget_mapping.clone() else {
            tracing::warn!(
                target: "glb_retarget",
                "Load a Mapping v2 first"
            );
            return;
        };
        self.refresh_glb_retarget_mapping();
        if self
            .retarget_validation
            .as_ref()
            .is_none_or(|report| !report.is_valid())
        {
            tracing::error!(
                target: "glb_retarget",
                "Mapping validation failed"
            );
            return;
        }
        let source = match self.build_glb_retarget_source_snapshot() {
            Ok(source) => source,
            Err(error) => {
                tracing::error!(
                    target: "glb_retarget",
                    error = %error,
                    "Failed to build retarget source snapshot"
                );
                return;
            }
        };
        let Some(source_path) = self.glb_path.clone() else {
            tracing::error!(
                target: "glb_retarget",
                "Source GLB path is unavailable"
            );
            return;
        };
        let source_clip_index = self.glb_animation_index;
        let target_skin_index = self.retarget_target_skin_index;
        let mut export_selection = self.glb_retarget_export_selection.clone();
        export_selection.skin_index = Some(target_skin_index);
        export_selection.selected_animations = BTreeSet::from([0]);
        export_selection.animation_output = AnimationOutputMode::Combined;
        let options = match self.glb_retarget_options() {
            Ok(options) => options,
            Err(error) => {
                tracing::error!(
                    target: "glb_retarget",
                    error = %error,
                    "Invalid GLB retarget options"
                );
                return;
            }
        };
        let reduce_keys = self.bvh_reduce_keys;
        let key_tolerance = self.bvh_key_tolerance;
        let Some(output_path) = rfd::FileDialog::new()
            .add_filter("GLB", &["glb"])
            .set_file_name("retargeted-animation.glb")
            .save_file()
        else {
            return;
        };
        if same_path(&output_path, Some(&source_path))
            || same_path(&output_path, Some(&target_path))
        {
            tracing::error!(
                target: "glb_retarget",
                output = %safe_path_label(&output_path),
                "Refusing to overwrite a source or target GLB"
            );
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.task_rx = Some(receiver);
        self.task_busy = true;
        let task_id = next_task_id();
        self.active_task_id = Some(task_id);
        tracing::info!(
            target: "glb_retarget",
            task_id,
            "Building the selected animation in background"
        );
        let mut request = crate::modules::operations::retarget::RetargetJob::for_snapshot_export(
            source_path.clone(), target_path.clone(), output_path.clone(),
            crate::modules::operations::retarget::SnapshotExportSettings {
                source_skin: self.retarget_source_skin_index, target_skin: target_skin_index, source_axes: ["Y".into(),"-Z".into(),"m".into()],
                edits: crate::modules::operations::spec::EditSpec::from_export_edits(&self.glb_export_edits(false)), options,
                clip_name: "GLB Retarget".into(), selection: export_selection, reduce_keys: reduce_keys.then_some(key_tolerance), overwrite: self.output_overwrite,
            });
        request.source_animation = source_clip_index;
        std::thread::spawn(move || {
            let mut paths = Vec::new();
            let mut details = Vec::new();
            let result = (|| {
                let source = crate::modules::operations::retarget::glb_source(
                    &source,
                    source_path.parent(),
                    source_clip_index,
                    request.source_skin,
                    &Default::default(),
                    ["Y", "-Z", "m"],
                )
                .map_err(|e| e.to_string())?;
                let fingerprint =
                    crate::modules::operations::retarget::document_fingerprint(
                        &target,
                    )
                    .map_err(|e| e.to_string())?;
                let target = crate::modules::operations::retarget::target(
                    target,
                    target_skin_index,
                    fingerprint,
                    ["Y", "-Z", "m"],
                )
                .map_err(|e| e.to_string())?;
                let job = crate::modules::operations::retarget::prepare(
                    &source, &target, &mapping, &request,
                )
                .map_err(|e| e.to_string())?;
                let report = crate::modules::operations::glb::execute_job(
                    &job,
                    request.overwrite,
                )
                .map_err(|e| e.to_string())?;
                details.push(format_export_report(&report));
                paths.push(output_path.clone());
                Ok(())
            })();
            let _ = sender.send(crate::app::ExportTaskResult {
                task_id,
                kind: TaskKind::GlbRetarget,
                paths,
                details,
                result,
            });
        });
    }

    pub(crate) fn preview_glb_retarget(&mut self) {
        if self.glb_retarget_preview_active {
            tracing::warn!(
                target: "glb_retarget",
                "Exit the current preview before rebuilding it"
            );
            return;
        }
        self.refresh_glb_retarget_mapping();
        if self
            .retarget_validation
            .as_ref()
            .is_none_or(|report| !report.is_valid())
        {
            tracing::error!(
                target: "glb_retarget",
                "Mapping validation failed"
            );
            return;
        }
        let Some(mapping) = self.retarget_mapping.clone() else {
            tracing::warn!(
                target: "glb_retarget",
                "Load a Mapping v2 first"
            );
            return;
        };
        let Some(target) = self.glb_retarget_target.clone() else {
            tracing::warn!(
                target: "glb_retarget",
                "Choose a target GLB first"
            );
            return;
        };
        let Some(target_path) = self.glb_retarget_target_path.clone() else {
            return;
        };
        let source = match self.build_glb_retarget_source_snapshot() {
            Ok(source) => source,
            Err(error) => {
                tracing::error!(
                    target: "glb_retarget",
                    error = %error,
                    "Failed to build retarget source snapshot"
                );
                return;
            }
        };
        let Some(source_path) = self.glb_path.as_deref() else {
            return;
        };
        let target_skin =
            match target.skin_data_at(self.retarget_target_skin_index) {
                Ok(skin) => skin,
                Err(error) => {
                    tracing::error!(
                        target: "glb_retarget",
                        error = %error,
                        "Target Skin is unavailable"
                    );
                    return;
                }
            };
        let options = match self.glb_retarget_options() {
            Ok(options) => options,
            Err(error) => {
                tracing::error!(
                    target: "glb_retarget",
                    error = %error,
                    "Invalid GLB retarget options"
                );
                return;
            }
        };
        let clip = match retarget_export::retarget_clip_from_glb(
            &source,
            source_path.parent(),
            self.glb_animation_index,
            &target_skin,
            &mapping,
            options,
            "GLB Retarget",
            None,
        ) {
            Ok(clip) => clip,
            Err(error) => {
                tracing::error!(
                    target: "glb_retarget",
                    error = %error,
                    "GLB retarget preview failed"
                );
                return;
            }
        };
        let mut generated = target;
        if let Err(error) =
            retarget_export::apply_retarget_clip(&mut generated, clip)
        {
            tracing::error!(
                target: "glb_retarget",
                error = %error,
                "GLB retarget preview animation failed"
            );
            return;
        }
        let generated_bytes = match generated.to_bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::error!(
                    target: "glb_retarget",
                    error = %error,
                    "GLB retarget preview serialization failed"
                );
                return;
            }
        };
        match AnimationRuntime::from_bytes(
            &generated_bytes,
            target_path.parent(),
        ) {
            Ok(runtime) => {
                self.pending_glb_retarget_runtime = Some(runtime);
                self.glb_retarget_preview_active = true;
            }
            Err(error) => {
                tracing::warn!(
                    target: "glb_retarget",
                    error = %error,
                    "Target Mesh preview unavailable; using skeleton-only playback"
                );
                match AnimationRuntime::from_bytes_skeleton_only(
                    &generated_bytes,
                    target_path.parent(),
                ) {
                    Ok(runtime) => {
                        self.canvas.load_skeleton_runtime(runtime);
                        self.glb_retarget_preview_active = true;
                        self.glb_animation_index = 0;
                        self.glb_animation_time = 0.0;
                        self.glb_animation_playing = false;
                    }
                    Err(skeleton_error) => tracing::error!(
                        target: "glb_retarget",
                        error = %skeleton_error,
                        "Generated skeleton is not readable"
                    ),
                }
            }
        }
    }

    pub(crate) fn exit_glb_retarget_preview(&mut self) {
        if !self.glb_retarget_preview_active {
            return;
        }
        self.glb_retarget_preview_active = false;
        self.pending_glb_retarget_runtime = None;
        self.request_glb_reload(crate::reload::GlbReloadKind::OpenModel);
    }
}

fn invalid_report(error: String) -> retarget::MappingValidationReport {
    retarget::MappingValidationReport {
        errors: vec![error],
        ..Default::default()
    }
}

fn same_path(path: &Path, source: Option<&Path>) -> bool {
    source.is_some_and(|source| {
        crate::modules::operations::same_path(path, source)
    })
}
