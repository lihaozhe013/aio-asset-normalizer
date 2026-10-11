use super::*;

impl App {
    pub(super) fn open_retarget_session(
        &mut self,
        job: retarget_ops::RetargetJob,
        discard_changes: bool,
    ) -> BridgeResponse {
        for (path, label) in [
            (&job.source, "retarget source"),
            (&job.target, "retarget target"),
        ] {
            if let Err(error) = require_absolute(path, label) {
                return self.session_failure("validation", error);
            }
        }
        if let Some(path) = job.mapping.as_ref() {
            if let Err(error) = require_absolute(path, "mapping file") {
                return self.session_failure("validation", error);
            }
        }
        if self.session_has_pending_edits() && !discard_changes {
            return self.session_failure("unsaved_changes", "Opening a retarget setup would discard session edits; set discard_changes explicitly");
        }
        let source = match retarget_ops::load_source(&job) {
            Ok(source) => source,
            Err(error) => return self.session_failure("validation", error.to_string()),
        };
        let target = match retarget_ops::load_target(&job) {
            Ok(target) => target,
            Err(error) => return self.session_failure("validation", error.to_string()),
        };
        let mapping = if let Some(path) = job.mapping.as_deref() {
            match retarget_ops::load_mapping(path, &source, &target) {
                Ok(mapping) => Some(mapping),
                Err(error) => return self.session_failure("validation", error.to_string()),
            }
        } else {
            None
        };
        let source_is_bvh = job
            .source
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bvh"));
        let original_glb = if source_is_bvh {
            None
        } else {
            match GlbDocument::load(&job.source) {
                Ok(document) => Some(document),
                Err(error) => return self.session_failure("validation", error.to_string()),
            }
        };
        if let Some(document) = original_glb {
            self.glb = Some(document);
            self.preview_glb(&job.source);
            self.orientation_euler_degrees = job.source_edits.rotate_roots_degrees;
            self.root_scale = job.source_edits.scale_roots;
            self.root_translation = job.source_edits.translate_roots;
            self.bake_root_transform = job.source_edits.bake_root_transform;
            self.trim_enabled = job.source_edits.trim.is_some();
            if let Some(trim) = job.source_edits.trim.as_ref() {
                self.trim_animation = trim.animation;
                self.trim_start = trim.start;
                self.trim_end = trim.end;
            }
            self.glb_animation_index = job.source_animation;
            self.pending_animation_selection = Some(job.source_animation);
            self.retarget_source_skin_index = job.source_skin;
            self.page = Page::GlbEditor;
            self.glb_retarget_target = Some(target.document);
            self.glb_retarget_target_path = Some(job.target.clone());
            self.glb_retarget_export_selection = self
                .glb_retarget_target
                .as_ref()
                .and_then(|doc| doc.default_export_selection().ok())
                .unwrap_or_default();
        } else {
            let document = match crate::modules::bvh::BvhDocument::load(&job.source) {
                Ok(document) => document,
                Err(error) => return self.session_failure("validation", error.to_string()),
            };
            self.bvh_trim_start = 0.0;
            self.bvh_trim_end = document.duration().max(document.frame_time);
            self.bvh = Some(document);
            self.bvh_path = Some(job.source.clone());
            self.bvh_frame = 0;
            self.bvh_playing = false;
            self.page = Page::BvhStudio;
            self.bvh_target_glb = Some(target.document);
            self.bvh_target_path = Some(job.target.clone());
            self.bvh_export_selection = self
                .bvh_target_glb
                .as_ref()
                .and_then(|doc| doc.default_export_selection().ok())
                .unwrap_or_default();
            self.bvh_up_axis = job.source_up_axis.clone();
            self.bvh_forward_axis = job.source_forward_axis.clone();
            if let Some(unit) = job.source_unit.as_deref() {
                self.set_bvh_unit_from_ui(unit);
            }
            self.retarget_source_skin_index = job.source_skin;
        }
        self.retarget_target_skin_index = job.skin;
        self.retarget_root_motion = job.root_motion;
        self.retarget_normalize_initial_heading = job.normalize_heading;
        self.retarget_mapping = mapping;
        self.retarget_mapping_path = job.mapping.clone();
        self.retarget_validation = None;
        if let Some(state) = self.automation.as_mut() {
            state.retarget_job = Some(job.clone());
        }
        if source_is_bvh {
            self.needs_bvh_skeleton_reload = true;
            self.bvh_camera_focus_pending = true;
            self.needs_bvh_target_reload = true;
            self.refresh_v2_retarget_mapping();
        } else {
            self.refresh_glb_retarget_mapping();
        }
        self.commit_session_revision();
        self.session_success(Some(json!({"configured":true,"source":job.source,"target":job.target,"mapping_loaded":self.retarget_mapping.is_some()})))
    }

    pub(super) fn set_retarget_session_mapping(
        &mut self,
        mapping: SkeletonMapping,
    ) -> BridgeResponse {
        if let Err(error) = mapping.validate_schema() {
            return self.session_failure("validation", error.to_string());
        }
        let Some(job) = self
            .automation
            .as_ref()
            .and_then(|state| state.retarget_job.clone())
        else {
            return self.session_failure(
                "validation",
                "Open a retarget source and target before setting a Mapping",
            );
        };
        let source = match retarget_ops::load_source(&job) {
            Ok(source) => source,
            Err(error) => return self.session_failure("validation", error.to_string()),
        };
        let target = match retarget_ops::load_target(&job) {
            Ok(target) => target,
            Err(error) => return self.session_failure("validation", error.to_string()),
        };
        let report = retarget_ops::validate(&source, &target, &mapping);
        if !report.is_valid() {
            return self.session_failure("mapping_invalid", report.errors.join("; "));
        }
        self.retarget_mapping = Some(mapping);
        self.retarget_mapping_path = None;
        if job
            .source
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bvh"))
        {
            self.refresh_v2_retarget_mapping();
        } else {
            self.refresh_glb_retarget_mapping();
        }
        self.commit_session_revision();
        self.session_success(Some(json!({"mapping_valid":true,"mapped_count":report.mapped_count,"warnings":report.warnings})))
    }

    pub(super) fn run_retarget_session_action(
        &mut self,
        context: &three_d::Context,
        action: RetargetSessionAction,
        output: Option<PathBuf>,
        overwrite: bool,
    ) -> BridgeResponse {
        let Some(mut job) = self
            .automation
            .as_ref()
            .and_then(|state| state.retarget_job.clone())
        else {
            return self
                .session_failure("validation", "Configure retarget source and target first");
        };
        let source = match retarget_ops::load_source(&job) {
            Ok(source) => source,
            Err(error) => return self.session_failure("validation", error.to_string()),
        };
        let target = match retarget_ops::load_target(&job) {
            Ok(target) => target,
            Err(error) => return self.session_failure("validation", error.to_string()),
        };
        let mapping = self.retarget_mapping.clone();
        match action {
            RetargetSessionAction::Prompt => {
                let prompt = match retarget_ops::prompt(&source, &target, mapping.as_ref()) {
                    Ok(value) => value,
                    Err(error) => return self.session_failure("validation", error.to_string()),
                };
                self.session_success(Some(json!({"action":"prompt","prompt":prompt})))
            }
            RetargetSessionAction::Suggest => {
                let retarget_ops::Source::Bvh { document, .. } = &source else {
                    return self.session_failure(
                        "validation",
                        "Name suggestions require a BVH source; GLB sources use prompt context",
                    );
                };
                let suggestions=document.suggest_mapping(&target.skin).iter().map(|item|json!({"source_joint":item.source_joint,"target_node":item.target_node,"confidence":format!("{:?}",item.confidence).to_ascii_lowercase()})).collect::<Vec<_>>();
                self.session_success(Some(json!({"action":"suggest","suggestions":suggestions})))
            }
            RetargetSessionAction::Validate => {
                let Some(mapping) = mapping else {
                    return self
                        .session_failure("validation", "Set a Mapping v2 before validating");
                };
                let report = retarget_ops::validate(&source, &target, &mapping);
                self.retarget_validation = Some(report.clone());
                self.session_success(Some(json!({"action":"validate","valid":report.is_valid(),"mapped_count":report.mapped_count,"unmapped_source_nodes":report.unmapped_source_nodes,"errors":report.errors,"warnings":report.warnings})))
            }
            RetargetSessionAction::Preview => {
                let Some(mapping) = mapping else {
                    return self.session_failure(
                        "validation",
                        "Set a Mapping v2 before previewing retargeting",
                    );
                };
                let report = retarget_ops::validate(&source, &target, &mapping);
                if !report.is_valid() {
                    return self.session_failure("mapping_invalid", report.errors.join("; "));
                }
                if job
                    .source
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("bvh"))
                {
                    self.page = Page::BvhStudio;
                    self.needs_bvh_target_reload = true;
                    self.refresh_v2_retarget_mapping();
                } else {
                    self.page = Page::GlbEditor;
                    self.preview_glb_retarget();
                    if self.pending_glb_retarget_runtime.is_none() {
                        return self.session_failure("preview_failed","Retarget preview could not be built; inspect GUI logs and mapping warnings");
                    }
                }
                self.commit_session_revision();
                self.session_success(Some(
                    json!({"action":"preview","status":"queued","warnings":report.warnings}),
                ))
            }
            RetargetSessionAction::ExitPreview => {
                if self.page == Page::GlbEditor && self.glb_retarget_preview_active {
                    self.exit_glb_retarget_preview();
                } else if let Some(path) = self.bvh_target_path.as_deref() {
                    if let Err(error) = self.canvas.load_glb_for_target_preview(context, path) {
                        return self.session_failure("preview_failed", error);
                    }
                    self.needs_bvh_target_reload = false;
                }
                self.glb_animation_playing = false;
                self.bvh_playing = false;
                self.commit_session_revision();
                self.session_success(Some(json!({"action":"exit_preview","status":"ready"})))
            }
            RetargetSessionAction::Export => {
                let Some(mapping) = mapping else {
                    return self.session_failure("validation", "Set a Mapping v2 before exporting");
                };
                let Some(output) = output else {
                    return self.session_failure(
                        "validation",
                        "Retarget export requires an absolute output path",
                    );
                };
                if let Err(error) = require_absolute(&output, "retarget output") {
                    return self.session_failure("validation", error);
                }
                job.out = Some(output);
                job.overwrite = overwrite;
                let cancelable_job = job.clone();
                let worker = move |cancel: Arc<AtomicBool>,
                                   sender: mpsc::Sender<JobEvent>,
                                   job_id: String| {
                    if cancel.load(Ordering::Acquire) {
                        return Err(job_error(
                            "cancelled",
                            "Retarget export cancelled before preparation",
                        ));
                    }
                    let source =
                        retarget_ops::load_source(&cancelable_job).map_err(operation_job_error)?;
                    let target =
                        retarget_ops::load_target(&cancelable_job).map_err(operation_job_error)?;
                    let export = retarget_ops::prepare(&source, &target, &mapping, &cancelable_job)
                        .map_err(operation_job_error)?;
                    if cancel.load(Ordering::Acquire) {
                        return Err(job_error(
                            "cancelled",
                            "Retarget export cancelled before atomic output commit",
                        ));
                    }
                    let report = glb_ops::execute_job(&export, cancelable_job.overwrite)
                        .map_err(operation_job_error)?;
                    let _ = sender.send(JobEvent::Progress {
                        job_id,
                        progress: 1.0,
                        message: "Retargeted GLB validated and written".to_owned(),
                    });
                    Ok(json!({"output":export.path,"report":report}))
                };
                let Some(state) = self.automation.as_mut() else {
                    return self
                        .session_failure("session_unavailable", "GUI session is not registered");
                };
                match state.launch_job("retarget_export", worker) {
                    Ok(job_id) => {
                        self.session_success(Some(json!({"job_id":job_id,"status":"running"})))
                    }
                    Err(error) => self.session_failure(&error.kind, error.message),
                }
            }
        }
    }
}
