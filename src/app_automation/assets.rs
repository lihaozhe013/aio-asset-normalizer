use super::*;

impl App {
    pub(super) fn open_glb_session(
        &mut self,
        path: PathBuf,
        discard_changes: bool,
    ) -> BridgeResponse {
        if let Err(error) = require_absolute(&path, "GLB input") {
            return self.session_failure("validation", error);
        }
        if path
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("glb"))
        {
            return self.session_failure(
                "validation",
                "Only .glb files can be opened in the GLB Editor",
            );
        }
        if self.session_has_pending_edits() && !discard_changes {
            return self.session_failure(
                "unsaved_changes",
                "Opening another asset would discard session edits; set discard_changes explicitly",
            );
        }
        let document = match GlbDocument::load(&path) {
            Ok(document) => document,
            Err(error) => return self.session_failure("validation", error.to_string()),
        };
        if let Some(parent) = path.parent() {
            self.file_tree.open_folder(parent.to_path_buf());
            self.file_tree.select_file(&path);
        }
        self.glb = Some(document);
        self.preview_glb(&path);
        self.commit_session_revision();
        self.session_success(Some(json!({"opened":path})))
    }

    pub(super) fn open_bvh_session(
        &mut self,
        path: PathBuf,
        discard_changes: bool,
    ) -> BridgeResponse {
        if let Err(error) = require_absolute(&path, "BVH input") {
            return self.session_failure("validation", error);
        }
        if path
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("bvh"))
        {
            return self.session_failure("validation", "BVH Studio requires a .bvh file");
        }
        if self.session_has_pending_edits() && !discard_changes {
            return self.session_failure(
                "unsaved_changes",
                "Opening another asset would discard session edits; set discard_changes explicitly",
            );
        }
        let document = match crate::modules::bvh::BvhDocument::load(&path) {
            Ok(document) => document,
            Err(error) => return self.session_failure("validation", error.to_string()),
        };
        self.bvh_trim_start = 0.0;
        self.bvh_trim_end = document.duration().max(document.frame_time);
        self.bvh = Some(document);
        self.bvh_path = Some(path.clone());
        self.bvh_frame = 0;
        self.bvh_playing = false;
        self.page = Page::BvhStudio;
        self.needs_bvh_skeleton_reload = true;
        self.bvh_camera_focus_pending = true;
        self.canvas.show_origin = false;
        if let Some(parent) = path.parent() {
            if self.bvh_file_tree.root().is_none_or(|root| root != parent) {
                self.bvh_file_tree.open_folder(parent.to_path_buf());
            }
        }
        self.bvh_file_tree.select_file(&path);
        self.refresh_retarget_plan();
        self.refresh_v2_retarget_mapping();
        self.commit_session_revision();
        self.session_success(Some(json!({"opened":path})))
    }

    pub(super) fn set_glb_session_edits(
        &mut self,
        edits: crate::modules::operations::spec::EditSpec,
    ) -> BridgeResponse {
        let Some(document) = self.glb.as_ref() else {
            return self.session_failure("validation", "Open a GLB before setting edits");
        };
        for texture in &edits.textures {
            if let Err(error) = require_absolute(&texture.image, "texture image") {
                return self.session_failure("validation", error);
            }
        }
        if let Err(error) = glb_ops::edit_snapshot(document, &edits) {
            return self.session_failure("validation", error.to_string());
        }
        let mut texture_snapshot = document.clone();
        for texture in &edits.textures {
            if let Err(error) = texture_snapshot.replace_texture(
                crate::modules::glb::PrimitiveTarget {
                    mesh: texture.mesh,
                    primitive: texture.primitive,
                },
                TextureSlot::from(texture.slot),
                &texture.image,
                texture.duplicate_shared_material,
            ) {
                return self.session_failure("validation", error.to_string());
            }
        }
        let animation = edits
            .trim
            .as_ref()
            .map(|value| value.animation)
            .or_else(|| edits.animation_rate.as_ref().map(|value| value.animation))
            .or_else(|| edits.smart_loop.as_ref().map(|value| value.animation));
        self.glb = Some(texture_snapshot);
        self.orientation_euler_degrees = edits.rotate_roots_degrees;
        self.root_scale = edits.scale_roots;
        self.root_translation = edits.translate_roots;
        self.bake_root_transform = edits.bake_root_transform;
        self.trim_enabled = edits.trim.is_some();
        if let Some(trim) = edits.trim {
            self.trim_animation = trim.animation;
            self.trim_start = trim.start;
            self.trim_end = trim.end;
        }
        self.glb_animation_rate = edits.animation_rate.map(|rate| rate.rate).unwrap_or(1.0);
        self.smart_loop_enabled = edits.smart_loop.is_some();
        if let Some(loop_settings) = edits.smart_loop {
            self.smart_loop_transition = loop_settings.transition_seconds;
        }
        self.pending_animation_selection = animation;
        self.mark_root_preview_dirty();
        self.request_glb_reload(GlbReloadKind::EditedModel);
        self.commit_session_revision();
        self.session_success(Some(
            json!({"edits_applied":true,"texture_replacements":edits.textures.len()}),
        ))
    }

    pub(super) fn start_glb_session_preflight(
        &mut self,
        output: PathBuf,
        overwrite: bool,
        selection_spec: Option<crate::modules::operations::spec::SelectionSpec>,
    ) -> BridgeResponse {
        if let Err(error) = require_absolute(&output, "GLB output") {
            return self.session_failure("validation", error);
        }
        if self.task_busy || self.converter_busy || self.glb_batch.is_busy() {
            return self.session_failure("busy", "Another GUI asset task is already running");
        }
        if self.glb.is_none() {
            return self.session_failure("validation", "Open a GLB before preflighting an export");
        }
        let snapshot = match self.build_glb_export_snapshot() {
            Ok(document) => document,
            Err(error) => return self.session_failure("validation", error),
        };
        let selection = if let Some(spec) = selection_spec.as_ref() {
            match glb_ops::resolve_selection(&snapshot, None, Some(spec)) {
                Ok(selection) => selection,
                Err(error) => return self.session_failure("validation", error.to_string()),
            }
        } else {
            self.glb_export_selection.clone()
        };
        let edits = self.glb_export_edits(self.bake_root_transform);
        let sources = self
            .glb_path
            .as_deref()
            .into_iter()
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        let worker =
            move |cancel: Arc<AtomicBool>, sender: mpsc::Sender<JobEvent>, job_id: String| {
                if cancel.load(Ordering::Acquire) {
                    return Err(job_error(
                        "cancelled",
                        "Preflight cancelled before it started",
                    ));
                }
                let source_refs = sources.iter().map(PathBuf::as_path).collect::<Vec<_>>();
                let jobs = glb_ops::prepare_export(
                    &snapshot,
                    &edits,
                    &selection,
                    &output,
                    &source_refs,
                    overwrite,
                )
                .map_err(operation_job_error)?;
                if cancel.load(Ordering::Acquire) {
                    return Err(job_error(
                        "cancelled",
                        "Preflight cancelled after validating the export",
                    ));
                }
                let outputs = jobs.iter().map(|job| &job.path).collect::<Vec<_>>();
                let _ = sender.send(JobEvent::Progress {
                    job_id,
                    progress: 1.0,
                    message: format!("Validated {} output(s)", outputs.len()),
                });
                Ok(json!({"ready":true,"outputs":outputs,"source":sources.first()}))
            };
        let Some(state) = self.automation.as_mut() else {
            return self.session_failure("session_unavailable", "GUI session is not registered");
        };
        match state.launch_job("glb_preflight", worker) {
            Ok(job_id) => self.session_success(Some(
                json!({"job_id":job_id,"status":"running","writes_files":false}),
            )),
            Err(error) => self.session_failure(&error.kind, error.message),
        }
    }

    pub(super) fn start_glb_session_export(
        &mut self,
        output: PathBuf,
        overwrite: bool,
        selection_spec: Option<crate::modules::operations::spec::SelectionSpec>,
    ) -> BridgeResponse {
        if let Err(error) = require_absolute(&output, "GLB output") {
            return self.session_failure("validation", error);
        }
        if self.task_busy || self.converter_busy || self.glb_batch.is_busy() {
            return self.session_failure("busy", "Another GUI asset task is already running");
        }
        if self.glb.is_none() {
            return self.session_failure("validation", "Open a GLB before exporting");
        }
        let snapshot = match self.build_glb_export_snapshot() {
            Ok(document) => document,
            Err(error) => return self.session_failure("validation", error),
        };
        let selection = if let Some(spec) = selection_spec.as_ref() {
            match glb_ops::resolve_selection(&snapshot, None, Some(spec)) {
                Ok(selection) => selection,
                Err(error) => return self.session_failure("validation", error.to_string()),
            }
        } else {
            self.glb_export_selection.clone()
        };
        let edits = self.glb_export_edits(self.bake_root_transform);
        let committed_selection = selection.clone();
        let sources = self
            .glb_path
            .as_deref()
            .into_iter()
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        let source_paths = sources.clone();
        let worker = move |cancel: Arc<AtomicBool>,
                           sender: mpsc::Sender<JobEvent>,
                           job_id: String| {
            if cancel.load(Ordering::Acquire) {
                return Err(job_error(
                    "cancelled",
                    "Job was cancelled before export started",
                ));
            }
            let source_refs = source_paths
                .iter()
                .map(PathBuf::as_path)
                .collect::<Vec<_>>();
            let jobs = glb_ops::prepare_export(
                &snapshot,
                &edits,
                &selection,
                &output,
                &source_refs,
                overwrite,
            )
            .map_err(|error| operation_job_error(error))?;
            let total = jobs.len().max(1);
            let mut outputs = Vec::with_capacity(jobs.len());
            for (index, job) in jobs.into_iter().enumerate() {
                if cancel.load(Ordering::Acquire) {
                    return Err(job_error(
                        "cancelled",
                        format!("Export stopped after {} completed output(s)", outputs.len()),
                    ));
                }
                let report = glb_ops::execute_job(&job, overwrite).map_err(operation_job_error)?;
                outputs.push(json!({"path":job.path,"report":report}));
                let _ = sender.send(JobEvent::Progress {
                    job_id: job_id.clone(),
                    progress: (index + 1) as f32 / total as f32,
                    message: format!("Wrote output {} of {total}", index + 1),
                });
            }
            Ok(json!({"outputs":outputs,"source":source_paths.first()}))
        };
        let Some(state) = self.automation.as_mut() else {
            return self.session_failure("session_unavailable", "GUI session is not registered");
        };
        let job_id = match state.launch_job("glb_export", worker) {
            Ok(job_id) => job_id,
            Err(error) => return self.session_failure(&error.kind, error.message),
        };
        if selection_spec.is_some() {
            self.glb_export_selection = committed_selection;
            self.commit_session_revision();
        }
        self.session_success(Some(
            json!({"job_id":job_id,"status":"running","outputs_are_atomic":true}),
        ))
    }

    pub(super) fn start_bvh_session_export(
        &mut self,
        output: PathBuf,
        overwrite: bool,
    ) -> BridgeResponse {
        if let Err(error) = require_absolute(&output, "BVH output") {
            return self.session_failure("validation", error);
        }
        if self.task_busy || self.converter_busy || self.glb_batch.is_busy() {
            return self.session_failure("busy", "Another GUI asset task is already running");
        }
        let Some(document) = self.bvh.clone() else {
            return self.session_failure("validation", "Open a BVH before exporting");
        };
        let input = self.bvh_path.clone().unwrap_or_default();
        let worker = move |cancel: Arc<AtomicBool>,
                           sender: mpsc::Sender<JobEvent>,
                           job_id: String| {
            if cancel.load(Ordering::Acquire) {
                return Err(job_error(
                    "cancelled",
                    "Job was cancelled before export started",
                ));
            }
            bvh_ops::write(&document, &output, &input, overwrite).map_err(operation_job_error)?;
            let _ = sender.send(JobEvent::Progress {
                job_id,
                progress: 1.0,
                message: "BVH validated and written".to_owned(),
            });
            Ok(
                json!({"output":output,"frames":document.frames.len(),"joints":document.joints.len()}),
            )
        };
        let Some(state) = self.automation.as_mut() else {
            return self.session_failure("session_unavailable", "GUI session is not registered");
        };
        match state.launch_job("bvh_export", worker) {
            Ok(job_id) => self.session_success(Some(json!({"job_id":job_id,"status":"running"}))),
            Err(error) => self.session_failure(&error.kind, error.message),
        }
    }
}
