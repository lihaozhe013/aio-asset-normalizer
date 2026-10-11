use super::*;

impl App {
    pub(super) fn run_checkpoint(&mut self, action: CheckpointAction) -> BridgeResponse {
        match action {
            CheckpointAction::Create => {
                let checkpoint = self.capture_checkpoint();
                if let Some(state) = self.automation.as_mut() {
                    state.checkpoint = Some(checkpoint);
                }
                self.commit_session_revision();
                self.session_success(Some(json!({"checkpoint":"created"})))
            }
            CheckpointAction::Delete => {
                if let Some(state) = self.automation.as_mut() {
                    state.checkpoint = None;
                }
                self.commit_session_revision();
                self.session_success(Some(json!({"checkpoint":"deleted"})))
            }
            CheckpointAction::Restore => {
                let Some(checkpoint) = self
                    .automation
                    .as_ref()
                    .and_then(|state| state.checkpoint.clone())
                else {
                    return self.session_failure(
                        "checkpoint_missing",
                        "This session has no checkpoint to restore",
                    );
                };
                let preview = checkpoint.retarget_preview_active;
                self.apply_checkpoint(checkpoint);
                if let Some(state) = self.automation.as_mut() {
                    state.restore_preview_pending = preview;
                }
                self.commit_session_revision();
                self.session_success(Some(json!({"checkpoint":"restored","playback":"paused"})))
            }
        }
    }

    pub(super) fn capture_checkpoint(&self) -> AppCheckpoint {
        let automation = self.automation.as_ref();
        AppCheckpoint {
            page: self.page,
            glb: self.glb.clone(),
            glb_path: self.glb_path.clone(),
            glb_export_selection: self.glb_export_selection.clone(),
            bvh_target_glb: self.bvh_target_glb.clone(),
            bvh_target_path: self.bvh_target_path.clone(),
            bvh_export_selection: self.bvh_export_selection.clone(),
            bvh: self.bvh.clone(),
            bvh_path: self.bvh_path.clone(),
            mapping: self.mapping.clone(),
            mapping_path: self.mapping_path.clone(),
            retarget_mapping: self.retarget_mapping.clone(),
            retarget_mapping_path: self.retarget_mapping_path.clone(),
            glb_retarget_target: self.glb_retarget_target.clone(),
            glb_retarget_target_path: self.glb_retarget_target_path.clone(),
            glb_retarget_export_selection: self.glb_retarget_export_selection.clone(),
            retarget_source_skin_index: self.retarget_source_skin_index,
            retarget_target_skin_index: self.retarget_target_skin_index,
            retarget_root_motion: self.retarget_root_motion,
            retarget_normalize_initial_heading: self.retarget_normalize_initial_heading,
            bvh_up_axis: self.bvh_up_axis.clone(),
            bvh_forward_axis: self.bvh_forward_axis.clone(),
            bvh_unit: self.bvh_unit.clone(),
            orientation_euler_degrees: self.orientation_euler_degrees,
            root_scale: self.root_scale,
            root_translation: self.root_translation,
            bake_root_transform: self.bake_root_transform,
            trim_enabled: self.trim_enabled,
            trim_animation: self.trim_animation,
            trim_start: self.trim_start,
            trim_end: self.trim_end,
            glb_animation_index: self.glb_animation_index,
            glb_animation_time: self.glb_animation_time,
            glb_animation_loop: self.glb_animation_loop,
            glb_animation_rate: self.glb_animation_rate,
            smart_loop_enabled: self.smart_loop_enabled,
            smart_loop_transition: self.smart_loop_transition,
            texture_mesh: self.texture_mesh,
            texture_primitive: self.texture_primitive,
            texture_slot: self.texture_slot,
            texture_duplicate_shared: self.texture_duplicate_shared,
            bvh_trim_start: self.bvh_trim_start,
            bvh_trim_end: self.bvh_trim_end,
            bvh_frame: self.bvh_frame,
            bvh_playback_speed: self.bvh_playback_speed,
            bvh_reduce_keys: self.bvh_reduce_keys,
            bvh_key_tolerance: self.bvh_key_tolerance,
            show_grid: self.canvas.show_grid,
            show_axes: self.canvas.show_axes,
            show_origin: self.canvas.show_origin,
            show_source_skeleton: self.canvas.show_source_skeleton,
            show_target_skeleton: self.canvas.show_target_skeleton,
            show_glb_skeleton: self.canvas.show_glb_skeleton,
            camera: self.camera.bookmark(),
            file_root: self.file_tree.root().cloned(),
            file_selection: self.file_tree.selected_files(),
            bvh_file_root: self.bvh_file_tree.root().cloned(),
            bvh_file_selection: self.bvh_file_tree.selected_file().map(Path::to_path_buf),
            converter_file_root: self.converter_file_tree.root().cloned(),
            converter_selection: self.converter_file_tree.selected_files(),
            batch_request: automation.and_then(|state| state.batch_request.clone()),
            converter_inputs: automation
                .map(|state| state.converter_inputs.clone())
                .unwrap_or_default(),
            converter_output_root: automation.and_then(|state| state.converter_output_root.clone()),
            converter_blender: automation.and_then(|state| state.converter_blender.clone()),
            converter_overwrite: automation.is_some_and(|state| state.converter_overwrite),
            retarget_job: automation.and_then(|state| state.retarget_job.clone()),
            retarget_preview_active: self.glb_retarget_preview_active,
        }
    }

    pub(super) fn apply_checkpoint(&mut self, checkpoint: AppCheckpoint) {
        self.page = checkpoint.page;
        self.glb = checkpoint.glb;
        self.glb_path = checkpoint.glb_path;
        self.glb_export_selection = checkpoint.glb_export_selection;
        self.bvh_target_glb = checkpoint.bvh_target_glb;
        self.bvh_target_path = checkpoint.bvh_target_path;
        self.bvh_export_selection = checkpoint.bvh_export_selection;
        self.bvh = checkpoint.bvh;
        self.bvh_path = checkpoint.bvh_path;
        self.mapping = checkpoint.mapping;
        self.mapping_path = checkpoint.mapping_path;
        self.retarget_mapping = checkpoint.retarget_mapping;
        self.retarget_mapping_path = checkpoint.retarget_mapping_path;
        self.glb_retarget_target = checkpoint.glb_retarget_target;
        self.glb_retarget_target_path = checkpoint.glb_retarget_target_path;
        self.glb_retarget_export_selection = checkpoint.glb_retarget_export_selection;
        self.retarget_source_skin_index = checkpoint.retarget_source_skin_index;
        self.retarget_target_skin_index = checkpoint.retarget_target_skin_index;
        self.retarget_root_motion = checkpoint.retarget_root_motion;
        self.retarget_normalize_initial_heading = checkpoint.retarget_normalize_initial_heading;
        self.bvh_up_axis = checkpoint.bvh_up_axis;
        self.bvh_forward_axis = checkpoint.bvh_forward_axis;
        self.bvh_unit = checkpoint.bvh_unit;
        self.orientation_euler_degrees = checkpoint.orientation_euler_degrees;
        self.root_scale = checkpoint.root_scale;
        self.root_translation = checkpoint.root_translation;
        self.bake_root_transform = checkpoint.bake_root_transform;
        self.trim_enabled = checkpoint.trim_enabled;
        self.trim_animation = checkpoint.trim_animation;
        self.trim_start = checkpoint.trim_start;
        self.trim_end = checkpoint.trim_end;
        self.glb_animation_index = checkpoint.glb_animation_index;
        self.glb_animation_time = checkpoint.glb_animation_time;
        self.glb_animation_playing = false;
        self.glb_animation_loop = checkpoint.glb_animation_loop;
        self.glb_animation_rate = checkpoint.glb_animation_rate;
        self.smart_loop_enabled = checkpoint.smart_loop_enabled;
        self.smart_loop_transition = checkpoint.smart_loop_transition;
        self.texture_mesh = checkpoint.texture_mesh;
        self.texture_primitive = checkpoint.texture_primitive;
        self.texture_slot = checkpoint.texture_slot;
        self.texture_duplicate_shared = checkpoint.texture_duplicate_shared;
        self.bvh_trim_start = checkpoint.bvh_trim_start;
        self.bvh_trim_end = checkpoint.bvh_trim_end;
        self.bvh_frame = checkpoint.bvh_frame;
        self.bvh_playing = false;
        self.bvh_playback_speed = checkpoint.bvh_playback_speed;
        self.bvh_reduce_keys = checkpoint.bvh_reduce_keys;
        self.bvh_key_tolerance = checkpoint.bvh_key_tolerance;
        self.canvas.show_grid = checkpoint.show_grid;
        self.canvas.show_axes = checkpoint.show_axes;
        self.canvas.show_origin = checkpoint.show_origin;
        self.canvas.show_source_skeleton = checkpoint.show_source_skeleton;
        self.canvas.show_target_skeleton = checkpoint.show_target_skeleton;
        self.canvas.show_glb_skeleton = checkpoint.show_glb_skeleton;
        self.glb_retarget_preview_active = false;
        self.pending_glb_retarget_runtime = None;
        self.camera.restore_bookmark(checkpoint.camera);
        restore_file_tree(
            &mut self.file_tree,
            checkpoint.file_root,
            &checkpoint.file_selection,
        );
        if let Some(root) = checkpoint.bvh_file_root {
            self.bvh_file_tree.open_folder(root);
        } else {
            self.bvh_file_tree.clear();
        }
        if let Some(path) = checkpoint.bvh_file_selection {
            self.bvh_file_tree.select_file(&path);
        }
        restore_file_tree(
            &mut self.converter_file_tree,
            checkpoint.converter_file_root,
            &checkpoint.converter_selection,
        );
        self.reload_request = Some(GlbReloadKind::EditedModel);
        self.pending_animation_selection = Some(self.glb_animation_index);
        self.root_preview_dirty = true;
        self.needs_bvh_skeleton_reload = self.bvh.is_some();
        self.bvh_camera_focus_pending = false;
        self.needs_bvh_target_reload = self.page == Page::BvhStudio;
        if let Some(state) = self.automation.as_mut() {
            state.batch_request = checkpoint.batch_request;
            state.converter_inputs = checkpoint.converter_inputs;
            state.converter_output_root = checkpoint.converter_output_root;
            state.converter_blender = checkpoint.converter_blender;
            state.converter_overwrite = checkpoint.converter_overwrite;
            state.retarget_job = checkpoint.retarget_job;
        }
    }

    pub(super) fn set_session_view(
        &mut self,
        page: Option<SessionPage>,
        reset_camera: bool,
        camera: Option<CameraView>,
        show_grid: Option<bool>,
        show_axes: Option<bool>,
        show_origin: Option<bool>,
        show_source_skeleton: Option<bool>,
        show_target_skeleton: Option<bool>,
        show_glb_skeleton: Option<bool>,
        animation_index: Option<usize>,
        animation_time: Option<f32>,
        bvh_frame: Option<usize>,
        playing: Option<bool>,
    ) -> BridgeResponse {
        if camera.is_some_and(|view| {
            view.target.iter().any(|value| !value.is_finite())
                || !view.radius.is_finite()
                || view.radius <= 0.0
                || !view.theta.is_finite()
                || !view.phi.is_finite()
        }) {
            return self.session_failure(
                "validation",
                "Camera target and angles must be finite and radius must be positive",
            );
        }
        if let Some(frame) = bvh_frame {
            let Some(document) = self.bvh.as_ref() else {
                return self.session_failure("validation", "Open a BVH before selecting a frame");
            };
            if frame >= document.frames.len() {
                return self.session_failure(
                    "validation",
                    format!("BVH frame {frame} is outside the available frame range"),
                );
            }
        }
        if let Some(index) = animation_index {
            if self
                .canvas
                .animation_clips()
                .get(index)
                .is_none_or(|clip| !clip.is_playable())
            {
                return self.session_failure(
                    "validation",
                    format!("Animation {index} is unavailable or unsupported"),
                );
            }
        }
        if animation_time.is_some_and(|time| !time.is_finite() || time < 0.0) {
            return self.session_failure(
                "validation",
                "Animation time must be finite and non-negative",
            );
        }
        if let Some(page) = page {
            self.page = match page {
                SessionPage::GlbEditor => Page::GlbEditor,
                SessionPage::BvhStudio => Page::BvhStudio,
                SessionPage::FbxConverter => Page::FbxConverter,
            };
        }
        if reset_camera {
            self.camera.reset();
        }
        if let Some(view) = camera {
            self.camera.restore_bookmark(CameraBookmark {
                target: view.target,
                radius: view.radius,
                theta: view.theta,
                phi: view.phi,
            });
        }
        if let Some(value) = show_grid {
            self.canvas.show_grid = value;
        }
        if let Some(value) = show_axes {
            self.canvas.show_axes = value;
        }
        if let Some(value) = show_origin {
            self.canvas.show_origin = value;
        }
        if let Some(value) = show_source_skeleton {
            self.canvas.show_source_skeleton = value;
        }
        if let Some(value) = show_target_skeleton {
            self.canvas.show_target_skeleton = value;
        }
        if let Some(value) = show_glb_skeleton {
            self.canvas.show_glb_skeleton = value;
        }
        if let Some(index) = animation_index {
            self.select_glb_animation(index);
        }
        if let Some(time) = animation_time {
            self.set_glb_animation_time(time);
        }
        if let Some(frame) = bvh_frame {
            self.set_bvh_frame(frame);
        }
        if let Some(playing) = playing {
            if self.page == Page::BvhStudio {
                self.bvh_playing = playing;
            } else {
                self.glb_animation_playing = playing;
            }
        }
        self.commit_session_revision();
        self.session_success(Some(json!({"view_updated":true})))
    }
}
