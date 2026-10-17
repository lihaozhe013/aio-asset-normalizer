use std::{
    collections::HashMap,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError},
        Arc,
    },
    thread,
    time::Instant,
};

use crate::{
    app::App,
    app_glb_batch::GlbInspectorScope,
    modules::{
        automation::{
            self, AutomationBatchRequest, BridgeResponse, CameraView, CaptureScope,
            CheckpointAction, GuiRequest, RetargetSessionAction, SessionCommand, SessionPage,
            SessionState,
        },
        glb::{batch_runner, GlbDocument, GlbExportSelection, TextureSlot},
        operations::{
            self, bvh as bvh_ops, converter as converter_ops, glb as glb_ops,
            retarget as retarget_ops,
        },
        retarget::SkeletonMapping,
        task_lock::{AssetTaskGuard, AssetTaskLockError},
        ui::menu_bar::Page,
        viewport::camera::CameraBookmark,
    },
    reload::GlbReloadKind,
};
use serde_json::{json, Value};
use three_d::{RenderTarget, Viewport};

mod assets;
mod checkpoint_view;
mod dispatch;
mod helpers;
mod retarget;
mod workflows;

use self::helpers::*;

#[derive(Clone)]
struct AppCheckpoint {
    page: Page,
    glb: Option<GlbDocument>,
    glb_path: Option<PathBuf>,
    glb_export_selection: GlbExportSelection,
    bvh_target_glb: Option<GlbDocument>,
    bvh_target_path: Option<PathBuf>,
    bvh_export_selection: GlbExportSelection,
    bvh: Option<crate::modules::bvh::BvhDocument>,
    bvh_path: Option<PathBuf>,
    mapping: Option<crate::modules::bvh::MappingFile>,
    mapping_path: Option<PathBuf>,
    retarget_mapping: Option<SkeletonMapping>,
    retarget_mapping_path: Option<PathBuf>,
    glb_retarget_target: Option<GlbDocument>,
    glb_retarget_target_path: Option<PathBuf>,
    glb_retarget_export_selection: GlbExportSelection,
    retarget_source_skin_index: usize,
    retarget_target_skin_index: usize,
    retarget_root_motion: bool,
    retarget_normalize_initial_heading: bool,
    bvh_up_axis: String,
    bvh_forward_axis: String,
    bvh_unit: String,
    orientation_euler_degrees: [f32; 3],
    root_scale: f32,
    root_translation: [f32; 3],
    bake_root_transform: bool,
    trim_enabled: bool,
    trim_animation: usize,
    trim_start: f32,
    trim_end: f32,
    glb_animation_index: usize,
    glb_animation_time: f32,
    glb_animation_loop: bool,
    glb_animation_rate: f32,
    smart_loop_enabled: bool,
    smart_loop_transition: f32,
    texture_mesh: usize,
    texture_primitive: usize,
    texture_slot: TextureSlot,
    texture_duplicate_shared: bool,
    bvh_trim_start: f32,
    bvh_trim_end: f32,
    bvh_frame: usize,
    bvh_playback_speed: f32,
    bvh_reduce_keys: bool,
    bvh_key_tolerance: f32,
    show_grid: bool,
    show_axes: bool,
    show_origin: bool,
    show_source_skeleton: bool,
    show_target_skeleton: bool,
    show_glb_skeleton: bool,
    camera: CameraBookmark,
    file_root: Option<PathBuf>,
    file_selection: Vec<PathBuf>,
    bvh_file_root: Option<PathBuf>,
    bvh_file_selection: Option<PathBuf>,
    converter_file_root: Option<PathBuf>,
    converter_selection: Vec<PathBuf>,
    batch_request: Option<AutomationBatchRequest>,
    converter_inputs: Vec<PathBuf>,
    converter_output_root: Option<PathBuf>,
    converter_blender: Option<PathBuf>,
    converter_overwrite: bool,
    retarget_job: Option<retarget_ops::RetargetJob>,
    retarget_preview_active: bool,
}

struct CaptureRequest {
    scope: CaptureScope,
    reply: SyncSender<BridgeResponse>,
}

#[derive(Clone)]
struct JobRecord {
    kind: String,
    status: String,
    progress: f32,
    message: Option<String>,
    result: Option<Value>,
    error: Option<automation::BridgeError>,
    cancel: Arc<AtomicBool>,
}

enum JobEvent {
    Progress {
        job_id: String,
        progress: f32,
        message: String,
    },
    Finished {
        job_id: String,
        result: Result<Value, automation::BridgeError>,
    },
}

pub(crate) struct AutomationState {
    _bridge: automation::BridgeServer,
    inbox: Receiver<GuiRequest>,
    session_id: String,
    revision: u64,
    frame_id: u64,
    state_fingerprint: Option<String>,
    checkpoint: Option<AppCheckpoint>,
    pending_captures: Vec<CaptureRequest>,
    last_request_at: Option<Instant>,
    batch_request: Option<AutomationBatchRequest>,
    converter_inputs: Vec<PathBuf>,
    converter_output_root: Option<PathBuf>,
    converter_blender: Option<PathBuf>,
    converter_overwrite: bool,
    retarget_job: Option<retarget_ops::RetargetJob>,
    jobs: HashMap<String, JobRecord>,
    active_job: Option<String>,
    job_tx: mpsc::Sender<JobEvent>,
    job_rx: Receiver<JobEvent>,
    restore_preview_pending: bool,
}

impl AutomationState {
    fn new(bridge: automation::BridgeServer, inbox: Receiver<GuiRequest>) -> Self {
        let (job_tx, job_rx) = mpsc::channel();
        let session_id = bridge.manifest.session_id.clone();
        Self {
            _bridge: bridge,
            inbox,
            session_id,
            revision: 0,
            frame_id: 0,
            state_fingerprint: None,
            checkpoint: None,
            pending_captures: Vec::new(),
            last_request_at: None,
            batch_request: None,
            converter_inputs: Vec::new(),
            converter_output_root: None,
            converter_blender: None,
            converter_overwrite: false,
            retarget_job: None,
            jobs: HashMap::new(),
            active_job: None,
            job_tx,
            job_rx,
            restore_preview_pending: false,
        }
    }

    fn state(&self, app: &App) -> SessionState {
        SessionState {
            session_id: self.session_id.clone(),
            revision: self.revision,
            page: match app.page {
                Page::GlbEditor => SessionPage::GlbEditor,
                Page::BvhStudio => SessionPage::BvhStudio,
                Page::FbxConverter => SessionPage::FbxConverter,
                Page::ConnectAi => SessionPage::ConnectAi,
            },
            glb_path: app.glb_path.clone(),
            bvh_path: app.bvh_path.clone(),
            retarget_source: self.retarget_job.as_ref().map(|job| job.source.clone()),
            retarget_target: self.retarget_job.as_ref().map(|job| job.target.clone()),
            converter_inputs: self.converter_inputs.clone(),
            glb_dirty: app.glb.as_ref().is_some_and(|document| document.dirty)
                || app.root_transform_active()
                || app.trim_enabled
                || app.smart_loop_enabled
                || (app.glb_animation_rate - 1.0).abs() > f32::EPSILON,
            bvh_dirty: app.bvh_trim_start > 0.0
                || app.bvh.as_ref().is_some_and(|document| {
                    app.bvh_trim_end < document.duration().max(document.frame_time)
                }),
            asset_busy: self.active_job.is_some()
                || app.task_busy
                || app.converter_busy
                || app.glb_batch.is_busy(),
            glb_animation_index: app.glb_animation_index,
            glb_animation_time: app.glb_animation_time,
            glb_animation_playing: app.glb_animation_playing,
            bvh_frame: app.bvh_frame,
            bvh_playing: app.bvh_playing,
            retarget_preview_active: app.glb_retarget_preview_active,
            has_checkpoint: self.checkpoint.is_some(),
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }

    pub(crate) fn has_active_job(&self) -> bool {
        self.active_job.is_some()
    }

    fn current_job_response(&self, app: &App, job_id: &str) -> BridgeResponse {
        match self.jobs.get(job_id) {
            Some(job) => {
                let mut response = BridgeResponse::success(self.state(app));
                response.data = Some(json!({
                    "job_id":job_id,
                    "kind":job.kind,
                    "status":job.status,
                    "progress":job.progress,
                    "message":job.message,
                    "result":job.result,
                    "error":job.error,
                }));
                response
            }
            None => BridgeResponse::failure("job_not_found", format!("No job has id {job_id}")),
        }
    }

    fn launch_job<F>(&mut self, kind: &str, work: F) -> Result<String, automation::BridgeError>
    where
        F: FnOnce(
                Arc<AtomicBool>,
                mpsc::Sender<JobEvent>,
                String,
            ) -> Result<Value, automation::BridgeError>
            + Send
            + 'static,
    {
        if self.active_job.is_some() {
            return Err(automation::BridgeError {
                kind: "busy".to_owned(),
                message: "Another session asset job is already running".to_owned(),
            });
        }
        let host_task_lock = match AssetTaskGuard::try_acquire() {
            Ok(guard) => guard,
            Err(AssetTaskLockError::Busy) => {
                return Err(automation::BridgeError {
                    kind: "busy".to_owned(),
                    message: "Another asset computation is active for this user".to_owned(),
                });
            }
            Err(error) => {
                return Err(automation::BridgeError {
                    kind: "external".to_owned(),
                    message: error.to_string(),
                });
            }
        };
        let job_id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.insert(
            job_id.clone(),
            JobRecord {
                kind: kind.to_owned(),
                status: "running".to_owned(),
                progress: 0.0,
                message: Some("Preparing immutable job snapshot".to_owned()),
                result: None,
                error: None,
                cancel: cancel.clone(),
            },
        );
        self.active_job = Some(job_id.clone());
        let sender = self.job_tx.clone();
        let worker_id = job_id.clone();
        let target = if kind.contains("converter") {
            "fbx_converter"
        } else {
            "automation_bridge"
        };
        let task_id = crate::modules::logging::next_task_id();
        tracing::info!(target, task_id, job_id = %job_id, job_kind = kind, "Started GUI session job");
        thread::Builder::new()
            .name(format!("session-job-{kind}"))
            .spawn(move || {
                let _host_task_lock = host_task_lock;
                let result = work(cancel, sender.clone(), worker_id.clone());
                let _ = sender.send(JobEvent::Finished {
                    job_id: worker_id,
                    result,
                });
            })
            .map_err(|error| {
                self.active_job = None;
                self.jobs.remove(&job_id);
                automation::BridgeError {
                    kind: "external".to_owned(),
                    message: error.to_string(),
                }
            })?;
        Ok(job_id)
    }

    fn poll_jobs(&mut self) {
        loop {
            match self.job_rx.try_recv() {
                Ok(JobEvent::Progress {
                    job_id,
                    progress,
                    message,
                }) => {
                    if let Some(job) = self.jobs.get_mut(&job_id) {
                        job.progress = progress.clamp(0.0, 1.0);
                        job.message = Some(message);
                    }
                }
                Ok(JobEvent::Finished { job_id, result }) => {
                    if self.active_job.as_deref() == Some(job_id.as_str()) {
                        self.active_job = None;
                    }
                    if let Some(job) = self.jobs.get_mut(&job_id) {
                        match result {
                            Ok(value) => {
                                job.status = "succeeded".to_owned();
                                job.progress = 1.0;
                                job.message = Some("Job completed".to_owned());
                                job.result = Some(value);
                                job.error = None;
                            }
                            Err(error) if error.kind == "cancelled" => {
                                job.status = "cancelled".to_owned();
                                job.message = Some(error.message.clone());
                                job.error = Some(error);
                            }
                            Err(error) => {
                                job.status = "failed".to_owned();
                                job.message = Some(error.message.clone());
                                job.error = Some(error);
                            }
                        }
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
    }
}

impl Drop for AutomationState {
    fn drop(&mut self) {
        for job in self.jobs.values() {
            if job.status == "running" || job.status == "cancelling" {
                job.cancel.store(true, Ordering::Release);
            }
        }
    }
}

impl App {
    pub(crate) fn note_manual_session_mutation(&mut self) {
        self.commit_session_revision();
    }

    pub(crate) fn attach_automation(
        &mut self,
        bridge: automation::BridgeServer,
        inbox: Receiver<GuiRequest>,
    ) {
        self.automation = Some(AutomationState::new(bridge, inbox));
        let fingerprint = self.automation_fingerprint();
        if let Some(state) = self.automation.as_mut() {
            tracing::info!(
                target: "automation_bridge",
                session_id = %state.session_id,
                protocol_version = automation::BRIDGE_PROTOCOL_VERSION,
                "Registered local GUI session"
            );
            state.state_fingerprint = Some(fingerprint);
        }
    }

    pub(crate) fn process_automation_requests(&mut self, context: &three_d::Context) {
        let Some(state) = self.automation.as_mut() else {
            return;
        };
        state.poll_jobs();
        let requests = state.inbox.try_iter().collect::<Vec<_>>();
        if self.shutdown_pending {
            for request in requests {
                let _ = request.reply.send(BridgeResponse::failure(
                    "shutting_down",
                    "The GUI session is closing and no longer accepts commands",
                ));
            }
            return;
        }
        for request in requests {
            if let Some(state) = self.automation.as_mut() {
                state.last_request_at = Some(Instant::now());
            }
            let response =
                self.handle_session_command(context, request.command, request.reply.clone());
            if let Some(response) = response {
                let _ = request.reply.send(response);
            }
        }
        if let Some(state) = self.automation.as_mut() {
            if state.restore_preview_pending {
                state.restore_preview_pending = false;
                if self.page == Page::GlbEditor {
                    self.preview_glb_retarget();
                } else if self.page == Page::BvhStudio {
                    self.needs_bvh_target_reload = true;
                }
            }
        }
    }

    pub(crate) fn observe_manual_automation_state(&mut self) {
        let fingerprint = self.automation_fingerprint();
        let Some(state) = self.automation.as_mut() else {
            return;
        };
        if state
            .state_fingerprint
            .as_deref()
            .is_some_and(|last| last != fingerprint)
        {
            state.bump_revision();
        }
        state.state_fingerprint = Some(fingerprint);
    }

    pub(crate) fn capture_after_render(
        &mut self,
        target: &RenderTarget<'_>,
        viewport: Viewport,
        screen: Viewport,
        device_pixel_ratio: f32,
    ) {
        let Some((request, revision, frame_id)) = self.automation.as_mut().and_then(|state| {
            state.frame_id = state.frame_id.saturating_add(1);
            if state.pending_captures.is_empty() {
                None
            } else {
                let request = state.pending_captures.remove(0);
                Some((request, state.revision, state.frame_id))
            }
        }) else {
            return;
        };
        if request.scope == CaptureScope::Viewport && !self.page.uses_viewport() {
            let _ = request.reply.send(BridgeResponse::failure(
                "viewport_unavailable",
                "The active page has no 3D viewport",
            ));
            return;
        }
        let rect = if request.scope == CaptureScope::Viewport {
            viewport
        } else {
            screen
        };
        if rect.width == 0
            || rect.height == 0
            || rect.width > 8192
            || rect.height > 8192
            || (rect.width as u64).saturating_mul(rect.height as u64) > 32_000_000
        {
            let _ = request.reply.send(BridgeResponse::failure(
                "capture_unavailable",
                "The render target is minimized or exceeds the capture size limit",
            ));
            return;
        }
        let pixels = if request.scope == CaptureScope::Viewport {
            target.read_color_partially::<[u8; 4]>(rect.into())
        } else {
            target.read_color::<[u8; 4]>()
        };
        let mut rgba = Vec::with_capacity(pixels.len() * 4);
        for pixel in pixels {
            rgba.extend_from_slice(&pixel);
        }
        let width = rect.width;
        let height = rect.height;
        let Some(session_state) = self.automation.as_ref().map(|state| state.state(self)) else {
            return;
        };
        let metadata = json!({
            "scope":request.scope,
            "revision":revision,
            "frame_id":frame_id,
            "animation_time":self.glb_animation_time,
            "bvh_frame":self.bvh_frame,
            "camera":self.camera.bookmark(),
            "viewport":{"x":rect.x,"y":rect.y,"width":width,"height":height},
            "device_pixel_ratio":device_pixel_ratio,
        });
        let sender = request.reply;
        thread::Builder::new()
            .name("session-capture-encode".to_owned())
            .spawn(move || {
                let encoded = encode_png(width, height, rgba);
                let response = match encoded {
                    Ok(png) => {
                        use base64::Engine;
                        BridgeResponse {
                            ok: true,
                            error: None,
                            data: Some(metadata),
                            state: Some(session_state),
                            png_base64: Some(base64::engine::general_purpose::STANDARD.encode(png)),
                            warnings: Vec::new(),
                        }
                    }
                    Err(message) => BridgeResponse::failure("capture_encode_failed", message),
                };
                let _ = sender.send(response);
            })
            .ok();
    }

    fn automation_fingerprint(&self) -> String {
        json!({
            "page":format!("{:?}",self.page),
            "glb_path":self.glb_path,
            "glb_dirty":self.glb.as_ref().is_some_and(|document| document.dirty),
            "glb_selection":format!("{:?}",self.glb_export_selection),
            "bvh_path":self.bvh_path,
            "bvh_shape":self.bvh.as_ref().map(|document| (document.frames.len(),document.frame_time)),
            "bvh_axes":[self.bvh_up_axis,self.bvh_forward_axis,self.bvh_unit],
            "root_rotation":self.orientation_euler_degrees,
            "root_scale":self.root_scale,
            "root_translation":self.root_translation,
            "bake_root":self.bake_root_transform,
            "trim":[self.trim_enabled,self.trim_animation,self.trim_start,self.trim_end],
            "glb_animation":[self.glb_animation_index,if self.glb_animation_playing { 0.0 } else { self.glb_animation_time },self.glb_animation_loop,self.glb_animation_rate],
            "smart_loop":[self.smart_loop_enabled,self.smart_loop_transition],
            "bvh_settings":[self.bvh_trim_start,self.bvh_trim_end,if self.bvh_playing { 0 } else { self.bvh_frame },self.bvh_playback_speed,self.bvh_reduce_keys,self.bvh_key_tolerance],
            "view":[self.canvas.show_grid,self.canvas.show_axes,self.canvas.show_origin,self.canvas.show_source_skeleton,self.canvas.show_target_skeleton,self.canvas.show_glb_skeleton],
            "camera":format!("{:?}",self.camera.bookmark()),
            "files":self.file_tree.selected_files(),
            "converter_files":self.converter_file_tree.selected_files(),
            "mapping":self.retarget_mapping.as_ref().and_then(|mapping| serde_json::to_string(mapping).ok()),
        }).to_string()
    }

    pub(crate) fn automation_status(&self) -> Option<(String, Option<String>, bool)> {
        let state = self.automation.as_ref()?;
        let connected = state
            .last_request_at
            .is_some_and(|instant| instant.elapsed() < std::time::Duration::from_secs(30));
        let task = state.active_job.as_ref().and_then(|id| {
            state
                .jobs
                .get(id)
                .map(|job| format!("{}: {}%", job.kind, (job.progress * 100.0).round() as u32))
        });
        Some((state.session_id.chars().take(8).collect(), task, connected))
    }

    pub(crate) fn cancel_active_automation_job(&mut self) {
        if let Some(state) = self.automation.as_mut() {
            if let Some(job_id) = state.active_job.as_ref() {
                if let Some(job) = state.jobs.get_mut(job_id) {
                    job.status = "cancelling".to_owned();
                    job.cancel.store(true, Ordering::Release);
                    tracing::info!(target:"automation_bridge",job_id=%job_id,"User requested session job cancellation");
                }
            }
        }
    }
}

fn encode_png(width: u32, height: u32, rgba: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "Renderer returned an invalid RGBA image".to_owned())?;
    let longest = width.max(height);
    if longest > 4096 {
        let scale = 4096.0 / longest as f32;
        let resized = image::imageops::resize(
            &image,
            ((width as f32 * scale).round() as u32).max(1),
            ((height as f32 * scale).round() as u32).max(1),
            image::imageops::FilterType::Lanczos3,
        );
        image = resized;
    }
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(bytes.into_inner())
}
