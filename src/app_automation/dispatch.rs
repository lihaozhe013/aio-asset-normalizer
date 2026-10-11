use super::*;

impl App {
    pub(super) fn handle_session_command(
        &mut self,
        context: &three_d::Context,
        command: SessionCommand,
        capture_reply: SyncSender<BridgeResponse>,
    ) -> Option<BridgeResponse> {
        let Some(state) = self.automation.as_ref() else {
            return Some(BridgeResponse::failure(
                "session_unavailable",
                "GUI session is not registered",
            ));
        };
        if let Some(expected) = expected_revision(&command) {
            if expected != state.revision {
                return Some(self.session_failure(
                    "state_conflict",
                    format!(
                        "Expected revision {expected}, current revision is {}",
                        state.revision
                    ),
                ));
            }
        }
        match command {
            SessionCommand::GetState => Some(self.session_success(None)),
            SessionCommand::Capture { scope } => {
                if let Some(state) = self.automation.as_mut() {
                    state.pending_captures.push(CaptureRequest {
                        scope,
                        reply: capture_reply,
                    });
                }
                None
            }
            SessionCommand::JobGet { job_id } => {
                let response = self
                    .automation
                    .as_ref()
                    .map(|state| state.current_job_response(self, &job_id))
                    .unwrap_or_else(|| {
                        BridgeResponse::failure(
                            "session_unavailable",
                            "GUI session is not registered",
                        )
                    });
                Some(response)
            }
            SessionCommand::JobCancel { job_id } => {
                let Some(state) = self.automation.as_mut() else {
                    return Some(BridgeResponse::failure(
                        "session_unavailable",
                        "GUI session is not registered",
                    ));
                };
                let status = state.jobs.get(&job_id).map(|job| job.status.clone());
                match status.as_deref() {
                    None => {
                        return Some(
                            self.session_failure(
                                "job_not_found",
                                format!("No job has id {job_id}"),
                            ),
                        )
                    }
                    Some("running" | "cancelling") => {
                        if let Some(job) = state.jobs.get_mut(&job_id) {
                            job.status = "cancelling".to_owned();
                            job.cancel.store(true, Ordering::Release);
                        }
                    }
                    Some(status) => {
                        return Some(self.session_failure(
                            "job_not_running",
                            format!("Job {job_id} is {status}"),
                        ))
                    }
                }
                Some(self.session_success(Some(json!({"job_id":job_id,"status":"cancelling"}))))
            }
            SessionCommand::Checkpoint { action, .. } => Some(self.run_checkpoint(action)),
            SessionCommand::OpenGlb {
                path,
                discard_changes,
                ..
            } => Some(self.open_glb_session(path, discard_changes)),
            SessionCommand::OpenBvh {
                path,
                discard_changes,
                ..
            } => Some(self.open_bvh_session(path, discard_changes)),
            SessionCommand::SetGlbEdits { edits, .. } => Some(self.set_glb_session_edits(edits)),
            SessionCommand::PreflightGlb {
                output,
                overwrite,
                selection,
                ..
            } => Some(self.start_glb_session_preflight(output, overwrite, selection)),
            SessionCommand::ExportGlb {
                output,
                overwrite,
                selection,
                ..
            } => Some(self.start_glb_session_export(output, overwrite, selection)),
            SessionCommand::ConfigureBvh {
                up_axis,
                forward_axis,
                unit,
                trim,
                reduce_keys,
                ..
            } => Some(self.configure_bvh_session(up_axis, forward_axis, unit, trim, reduce_keys)),
            SessionCommand::ExportBvh {
                output, overwrite, ..
            } => Some(self.start_bvh_session_export(output, overwrite)),
            SessionCommand::OpenRetarget {
                job,
                discard_changes,
                ..
            } => Some(self.open_retarget_session(job, discard_changes)),
            SessionCommand::SetRetargetMapping { mapping, .. } => {
                Some(self.set_retarget_session_mapping(mapping))
            }
            SessionCommand::RetargetAction {
                action,
                output,
                overwrite,
                ..
            } => Some(self.run_retarget_session_action(context, action, output, overwrite)),
            SessionCommand::ConfigureBatch { request, .. } => {
                Some(self.configure_batch_session(request))
            }
            SessionCommand::RunBatch { export, .. } => Some(self.start_batch_session_job(export)),
            SessionCommand::ConfigureConverter {
                inputs,
                output_root,
                blender,
                overwrite,
                ..
            } => Some(self.configure_converter_session(inputs, output_root, blender, overwrite)),
            SessionCommand::RunConverter { .. } => Some(self.start_converter_session_job()),
            SessionCommand::PreflightConverter { .. } => {
                Some(self.start_converter_session_preflight())
            }
            SessionCommand::SetView {
                page,
                reset_camera,
                camera,
                show_grid,
                show_axes,
                show_origin,
                show_source_skeleton,
                show_target_skeleton,
                show_glb_skeleton,
                animation_index,
                animation_time,
                bvh_frame,
                playing,
                ..
            } => Some(self.set_session_view(
                page,
                reset_camera,
                camera,
                show_grid,
                show_axes,
                show_origin,
                show_source_skeleton,
                show_target_skeleton,
                show_glb_skeleton,
                animation_index,
                animation_time,
                bvh_frame,
                playing,
            )),
        }
    }

    pub(super) fn session_success(&self, data: Option<Value>) -> BridgeResponse {
        let Some(automation) = self.automation.as_ref() else {
            return BridgeResponse::failure("session_unavailable", "GUI session is not registered");
        };
        let mut response = BridgeResponse::success(automation.state(self));
        response.data = data;
        response
    }

    pub(super) fn session_failure(&self, kind: &str, message: impl Into<String>) -> BridgeResponse {
        let mut response = BridgeResponse::failure(kind, message);
        if let Some(automation) = self.automation.as_ref() {
            response.state = Some(automation.state(self));
        }
        response
    }

    pub(super) fn commit_session_revision(&mut self) {
        let fingerprint = self.automation_fingerprint();
        if let Some(state) = self.automation.as_mut() {
            state.bump_revision();
            state.state_fingerprint = Some(fingerprint);
        }
    }

    pub(super) fn session_has_pending_edits(&self) -> bool {
        self.glb.as_ref().is_some_and(|document| document.dirty)
            || self.root_transform_active()
            || self.trim_enabled
            || self.smart_loop_enabled
            || (self.glb_animation_rate - 1.0).abs() > f32::EPSILON
            || self.bvh_trim_start > 0.0
            || self.bvh.as_ref().is_some_and(|document| {
                self.bvh_trim_end < document.duration().max(document.frame_time)
            })
    }
}
