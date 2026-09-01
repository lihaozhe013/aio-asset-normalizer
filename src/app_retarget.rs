use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::mpsc;

use crate::app::{App, ExportTaskResult};
use crate::modules::glb::{AnimationClipData, AnimationRuntime, GlbDocument};
use crate::modules::retarget::{
    self, RetargetOptions, SkeletonDescriptor, SkeletonMapping, SourceKind,
};
use three_d::Context;

impl App {
    pub(crate) fn import_glb_retarget_target(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB", &["glb"])
            .pick_file()
        else {
            return;
        };
        match GlbDocument::load(&path) {
            Ok(document) => {
                self.glb_retarget_target = Some(document);
                self.glb_retarget_target_path = Some(path.clone());
                self.retarget_target_skin_index = 0;
                self.refresh_glb_retarget_mapping();
                self.log.append(&format!(
                    "[glb_retarget] Loaded target GLB {}",
                    path.display()
                ));
            }
            Err(error) => self.log.append(&format!(
                "[glb_retarget] Target GLB load failed: {error}"
            )),
        }
    }

    pub(crate) fn refresh_glb_retarget_mapping(&mut self) {
        let Some(source_document) = self.glb.as_ref() else {
            self.retarget_validation = None;
            return;
        };
        let Some(target_document) = self.glb_retarget_target.as_ref() else {
            self.retarget_validation = None;
            return;
        };
        let runtime = (!self.glb_retarget_preview_active)
            .then(|| self.canvas.animation_runtime())
            .flatten()
            .or_else(|| {
                source_document.to_bytes().ok().and_then(|bytes| {
                    AnimationRuntime::from_bytes_skeleton_only(
                        &bytes,
                        source_document
                            .source_path
                            .as_deref()
                            .and_then(Path::parent),
                    )
                    .ok()
                })
            });
        let Some(runtime) = runtime else {
            self.retarget_validation = None;
            return;
        };
        let source_clip = match runtime.clips.get(self.glb_animation_index) {
            Some(clip) => clip,
            None => {
                self.retarget_validation = Some(invalid_report(
                    "Selected source animation does not exist".to_owned(),
                ));
                return;
            }
        };
        if !source_clip.is_playable() {
            self.retarget_validation = Some(invalid_report(format!(
                "Selected source animation is unsupported: {}",
                source_clip.unsupported.join(", ")
            )));
            return;
        }
        let animated_nodes = source_clip
            .channels
            .iter()
            .map(|channel| channel.node)
            .collect::<HashSet<_>>();
        let source_descriptor = match SkeletonDescriptor::from_runtime(
            &runtime,
            source_document,
            self.retarget_source_skin_index,
            &animated_nodes,
            file_hash(source_document.source_path.as_deref()),
            "Y".to_owned(),
            "-Z".to_owned(),
            "m".to_owned(),
        ) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                self.retarget_validation =
                    Some(invalid_report(error.to_string()));
                return;
            }
        };
        let target_skin = match target_document
            .skin_data_at(self.retarget_target_skin_index)
        {
            Ok(skin) => skin,
            Err(error) => {
                self.retarget_validation =
                    Some(invalid_report(error.to_string()));
                return;
            }
        };
        let target_descriptor = match SkeletonDescriptor::from_skin(
            &target_skin,
            SourceKind::Glb,
            file_hash(target_document.source_path.as_deref()),
            String::new(),
            "Y",
            "-Z",
            "m",
            &HashSet::new(),
        ) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                self.retarget_validation =
                    Some(invalid_report(error.to_string()));
                return;
            }
        };
        let Some(mapping) = self.retarget_mapping.as_ref() else {
            self.retarget_validation = None;
            return;
        };
        self.retarget_validation = Some(retarget::validate_mapping(
            mapping,
            &source_descriptor,
            &target_descriptor,
        ));
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
        let source_descriptor = match SkeletonDescriptor::from_bvh(
            bvh,
            file_hash(bvh.source_path.as_deref()),
            self.bvh_up_axis.clone(),
            self.bvh_forward_axis.clone(),
            self.bvh_unit.clone(),
        ) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                self.retarget_validation =
                    Some(invalid_report(error.to_string()));
                return;
            }
        };
        let skin_index = self.retarget_target_skin_index;
        let skin = match target.skin_data_at(skin_index) {
            Ok(skin) => skin,
            Err(error) => {
                self.retarget_validation =
                    Some(invalid_report(error.to_string()));
                return;
            }
        };
        let target_descriptor = match SkeletonDescriptor::from_skin(
            &skin,
            SourceKind::Glb,
            file_hash(target.source_path.as_deref()),
            String::new(),
            "Y",
            "-Z",
            "m",
            &HashSet::new(),
        ) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                self.retarget_validation =
                    Some(invalid_report(error.to_string()));
                return;
            }
        };
        let mapping = if let Some(mapping) = self.retarget_mapping.clone() {
            mapping
        } else if let Some(legacy) = self.mapping.as_ref() {
            match retarget::from_legacy_bvh_mapping(
                legacy,
                &source_descriptor,
                &target_descriptor,
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
            &source_descriptor,
            &target_descriptor,
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
            self.log
                .append("[bvh_studio] Open a BVH before exporting GLB");
            return;
        };
        self.refresh_v2_retarget_mapping();
        let Some(mapping) = self.retarget_mapping.clone() else {
            self.log.append(
                "[bvh_studio] Load a valid Mapping JSON before exporting GLB",
            );
            return;
        };
        if self
            .retarget_validation
            .as_ref()
            .is_none_or(|report| !report.is_valid())
        {
            self.log.append("[bvh_studio] Mapping v2 validation failed");
            return;
        }
        let Some(target) = self.bvh_target_glb.clone() else {
            self.log
                .append("[bvh_studio] Load a target GLB before exporting GLB");
            return;
        };
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
            self.log.append(
                "[bvh_studio] Refusing to overwrite a source BVH or target GLB",
            );
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.task_rx = Some(receiver);
        self.task_busy = true;
        self.log.append(if clip_only {
            "[bvh_studio] Building animation clip in background"
        } else {
            "[bvh_studio] Building retargeted GLB in background"
        });
        let kind = if clip_only {
            "animation clip".to_owned()
        } else {
            "retargeted GLB".to_owned()
        };
        let reduce_keys = self.bvh_reduce_keys;
        let key_tolerance = self.bvh_key_tolerance;
        let options = self.retarget_options();
        let target_skin_index = self.retarget_target_skin_index;
        std::thread::spawn(move || {
            let result = (|| {
                let skin = target
                    .skin_data_at(target_skin_index)
                    .map_err(|error| error.to_string())?;
                let clip = retarget::retarget_bvh(
                    &source,
                    &skin,
                    &mapping,
                    options,
                    "BVH Retarget",
                )
                .map_err(|error| error.to_string())?;
                let mut clip = clip;
                if reduce_keys {
                    clip.reduce_keys(key_tolerance)
                        .map_err(|error| error.to_string())?;
                }
                let mut output = target;
                output
                    .replace_animations(&AnimationClipData {
                        name: clip.name,
                        times: clip.times,
                        channels: clip.channels,
                    })
                    .map_err(|error| error.to_string())?;
                if clip_only {
                    output.strip_render_resources();
                }
                output
                    .export_atomic(&path)
                    .map_err(|error| error.to_string())
            })();
            let _ = sender.send(ExportTaskResult { kind, path, result });
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
                self.log.append(&format!(
                    "[bvh_studio] Target Skin preview unavailable: {error}"
                ));
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
                self.log.append(&format!(
                    "[bvh_studio] Target skeleton preview unavailable: {error}"
                ));
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
            }
            Err(error) => self.log.append(&format!(
                "[bvh_studio] Target skeleton preview unavailable: {error}"
            )),
        }

        let Some(source) = self.bvh.clone() else {
            let _ = self.canvas.load_glb(context, &path);
            return;
        };
        let Some(mapping) = self.retarget_mapping.clone() else {
            let _ = self.canvas.load_glb(context, &path);
            return;
        };
        let report_valid = self
            .retarget_validation
            .as_ref()
            .is_some_and(|report| report.is_valid());
        if !report_valid {
            if let Err(error) = self.canvas.load_glb(context, &path) {
                self.log.append(&format!(
                    "[bvh_studio] Target GLB preview failed: {error}"
                ));
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
                        self.log.append(&format!(
                            "[retarget] Key reduction skipped: {error}"
                        ));
                    }
                }
                clip
            }
            Err(error) => {
                self.log
                    .append(&format!("[retarget] BVH preview failed: {error}"));
                if let Err(error) = self.canvas.load_glb(context, &path) {
                    self.log.append(&format!(
                        "[bvh_studio] Target GLB preview failed: {error}"
                    ));
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
            self.log.append(&format!(
                "[retarget] BVH preview animation failed: {error}"
            ));
            return;
        }
        let bytes = match generated.to_bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.log.append(&format!(
                    "[retarget] BVH preview serialization failed: {error}"
                ));
                return;
            }
        };
        match AnimationRuntime::from_bytes(&bytes, path.parent()) {
            Ok(runtime) => {
                if let Err(error) =
                    self.canvas.load_glb_with_runtime(context, &path, runtime)
                {
                    self.log.append(&format!(
                        "[bvh_studio] Target character preview failed: {error}"
                    ));
                } else {
                    let _ = self.canvas.update_glb_animation(
                        0,
                        self.bvh_frame as f32 * source.frame_time,
                    );
                    self.log.append(
                        "[retarget] BVH target character preview ready",
                    );
                }
            }
            Err(error) => {
                self.log.append(&format!(
                    "[bvh_studio] Target Mesh preview unavailable; using skeleton-only playback: {error}"
                ));
                match AnimationRuntime::from_bytes_skeleton_only(
                    &bytes,
                    path.parent(),
                ) {
                    Ok(runtime) => {
                        self.canvas.load_skeleton_runtime(runtime);
                        if let Err(error) = self
                            .canvas
                            .update_target_skeleton_animation(
                                context,
                                0,
                                self.bvh_frame as f32 * source.frame_time,
                                &skin.joints,
                            )
                        {
                            self.log.append(&format!(
                                "[retarget] Skeleton-only target preview failed: {error}"
                            ));
                        }
                    }
                    Err(skeleton_error) => self.log.append(&format!(
                        "[retarget] Skeleton-only target preview failed: {skeleton_error}"
                    )),
                }
            }
        }
    }

    pub(crate) fn export_retarget_mapping(&mut self) {
        let Some(mapping) = self.retarget_mapping.as_ref() else {
            self.log.append("[retarget] No Mapping v2 is available");
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
            self.log
                .append("[retarget] Refusing to overwrite the source mapping");
            return;
        }
        match retarget::save_mapping(&path, mapping) {
            Ok(()) => {
                self.file_tree.refresh();
                self.log.append(&format!(
                    "[retarget] Exported Mapping v2 {}",
                    path.display()
                ));
            }
            Err(error) => self
                .log
                .append(&format!("[retarget] Mapping export failed: {error}")),
        }
    }

    pub(crate) fn export_glb_retarget(&mut self) {
        if self.task_busy {
            return;
        }
        let Some(target) = self.glb_retarget_target.clone() else {
            self.log.append("[glb_retarget] Choose a target GLB first");
            return;
        };
        let Some(target_path) = self.glb_retarget_target_path.clone() else {
            return;
        };
        let Some(mapping) = self.retarget_mapping.clone() else {
            self.log.append("[glb_retarget] Load a Mapping v2 first");
            return;
        };
        self.refresh_glb_retarget_mapping();
        if self
            .retarget_validation
            .as_ref()
            .is_none_or(|report| !report.is_valid())
        {
            self.log.append("[glb_retarget] Mapping validation failed");
            return;
        }
        let source = match self.build_glb_retarget_source_snapshot() {
            Ok(source) => source,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
                return;
            }
        };
        let Some(source_path) = self.glb_path.clone() else {
            self.log
                .append("[glb_retarget] Source GLB path is unavailable");
            return;
        };
        let source_clip_index = self.glb_animation_index;
        let target_skin_index = self.retarget_target_skin_index;
        let options = match self.glb_retarget_options() {
            Ok(options) => options,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
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
            self.log.append(
                "[glb_retarget] Refusing to overwrite a source or target GLB",
            );
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.task_rx = Some(receiver);
        self.task_busy = true;
        self.log.append(
            "[glb_retarget] Building the selected animation in background",
        );
        std::thread::spawn(move || {
            let result = (|| {
                let source_bytes =
                    source.to_bytes().map_err(|error| error.to_string())?;
                let runtime = AnimationRuntime::from_bytes_skeleton_only(
                    &source_bytes,
                    source_path.parent(),
                )
                .map_err(|error| error.to_string())?;
                let effective_mapping = mapping_for_glb_snapshot(
                    &mapping,
                    &runtime,
                    &source,
                    source_clip_index,
                    &source_bytes,
                )?;
                let target_skin = target
                    .skin_data_at(target_skin_index)
                    .map_err(|error| error.to_string())?;
                let mut clip = retarget::retarget_glb(
                    &runtime,
                    &source,
                    source_clip_index,
                    &target_skin,
                    &effective_mapping,
                    options,
                    "GLB Retarget",
                )
                .map_err(|error| error.to_string())?;
                if reduce_keys {
                    clip.reduce_keys(key_tolerance)
                        .map_err(|error| error.to_string())?;
                }
                let mut output = target;
                output
                    .replace_animations(&AnimationClipData {
                        name: clip.name,
                        times: clip.times,
                        channels: clip.channels,
                    })
                    .map_err(|error| error.to_string())?;
                output
                    .export_atomic(&output_path)
                    .map_err(|error| error.to_string())
            })();
            let _ = sender.send(crate::app::ExportTaskResult {
                kind: "GLB retargeted animation".to_owned(),
                path: output_path,
                result,
            });
        });
    }

    pub(crate) fn preview_glb_retarget(&mut self) {
        if self.glb_retarget_preview_active {
            self.log.append(
                "[glb_retarget] Exit the current preview before rebuilding it",
            );
            return;
        }
        self.refresh_glb_retarget_mapping();
        if self
            .retarget_validation
            .as_ref()
            .is_none_or(|report| !report.is_valid())
        {
            self.log.append("[glb_retarget] Mapping validation failed");
            return;
        }
        let Some(mapping) = self.retarget_mapping.clone() else {
            self.log.append("[glb_retarget] Load a Mapping v2 first");
            return;
        };
        let Some(target) = self.glb_retarget_target.clone() else {
            self.log.append("[glb_retarget] Choose a target GLB first");
            return;
        };
        let Some(target_path) = self.glb_retarget_target_path.clone() else {
            return;
        };
        let source = match self.build_glb_retarget_source_snapshot() {
            Ok(source) => source,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
                return;
            }
        };
        let source_bytes = match source.to_bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
                return;
            }
        };
        let Some(source_path) = self.glb_path.as_deref() else {
            return;
        };
        let runtime = match AnimationRuntime::from_bytes_skeleton_only(
            &source_bytes,
            source_path.parent(),
        ) {
            Ok(runtime) => runtime,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
                return;
            }
        };
        let effective_mapping = match mapping_for_glb_snapshot(
            &mapping,
            &runtime,
            &source,
            self.glb_animation_index,
            &source_bytes,
        ) {
            Ok(mapping) => mapping,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
                return;
            }
        };
        let target_skin =
            match target.skin_data_at(self.retarget_target_skin_index) {
                Ok(skin) => skin,
                Err(error) => {
                    self.log.append(&format!("[glb_retarget] {error}"));
                    return;
                }
            };
        let options = match self.glb_retarget_options() {
            Ok(options) => options,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
                return;
            }
        };
        let clip = match retarget::retarget_glb(
            &runtime,
            &source,
            self.glb_animation_index,
            &target_skin,
            &effective_mapping,
            options,
            "GLB Retarget",
        ) {
            Ok(clip) => clip,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
                return;
            }
        };
        let mut generated = target;
        if let Err(error) = generated.replace_animations(&AnimationClipData {
            name: clip.name,
            times: clip.times,
            channels: clip.channels,
        }) {
            self.log.append(&format!("[glb_retarget] {error}"));
            return;
        }
        let generated_bytes = match generated.to_bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.log.append(&format!("[glb_retarget] {error}"));
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
                self.log.append(&format!(
                    "[glb_retarget] Target Mesh preview unavailable; using skeleton-only playback: {error}"
                ));
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
                    Err(skeleton_error) => self.log.append(&format!(
                        "[glb_retarget] Generated skeleton is not readable: {skeleton_error}"
                    )),
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

fn file_hash(path: Option<&Path>) -> String {
    path.and_then(|path| fs::read(path).ok())
        .map(|bytes| retarget::sha256_hex(&bytes))
        .unwrap_or_default()
}

fn mapping_for_glb_snapshot(
    mapping: &SkeletonMapping,
    runtime: &AnimationRuntime,
    source_document: &GlbDocument,
    clip_index: usize,
    source_bytes: &[u8],
) -> Result<SkeletonMapping, String> {
    let clip = runtime
        .clips
        .get(clip_index)
        .ok_or_else(|| format!("Animation {clip_index} does not exist"))?;
    let animated_nodes = clip
        .channels
        .iter()
        .map(|channel| channel.node)
        .collect::<HashSet<_>>();
    let descriptor = SkeletonDescriptor::from_runtime(
        runtime,
        source_document,
        mapping
            .source
            .skin
            .as_ref()
            .map(|skin| skin.index)
            .unwrap_or(0),
        &animated_nodes,
        retarget::sha256_hex(source_bytes),
        mapping.source.up_axis.clone(),
        mapping.source.forward_axis.clone(),
        mapping.source.unit.clone(),
    )
    .map_err(|error| error.to_string())?;
    let mut effective = mapping.clone();
    effective.source.file_sha256 = descriptor.file_sha256;
    effective.source.skeleton_sha256 = descriptor.skeleton_sha256;
    Ok(effective)
}

fn invalid_report(error: String) -> retarget::MappingValidationReport {
    retarget::MappingValidationReport {
        errors: vec![error],
        ..Default::default()
    }
}

fn same_path(path: &Path, source: Option<&Path>) -> bool {
    source.is_some_and(|source| {
        let left =
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let right = std::fs::canonicalize(source)
            .unwrap_or_else(|_| source.to_path_buf());
        #[cfg(windows)]
        {
            left.to_string_lossy()
                .eq_ignore_ascii_case(&right.to_string_lossy())
        }
        #[cfg(not(windows))]
        {
            left == right
        }
    })
}
