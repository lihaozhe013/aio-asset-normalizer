use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use crate::app_fbx_converter::ConverterFileState;
use crate::modules::blender::task::ConverterMessage;
use crate::modules::preferences::FileTreePreferences;
use crate::modules::retarget::{MappingValidationReport, SkeletonMapping};
use crate::modules::{
    bvh::{
        BvhDocument, MappingFile, MappingSuggestion, MappingValidation,
        RetargetPlan,
    },
    glb::{
        AnimationRuntime, EditOperation, GlbDocument, PrimitiveTarget,
        SmartLoopOptions, StandardizationProfile, TextureSlot,
    },
    i18n::I18n,
    preferences::{self, UserPreferences},
    ui::{
        bvh_file_tree::BvhFileTree,
        file_tree::FileTree,
        log_viewer::LogViewer,
        menu_bar::{MenuAction, Page},
    },
    viewport::{camera::OrbitCamera, canvas::ViewportCanvas},
};
use crate::reload::{merge_glb_reload_kind, GlbReloadKind};
use three_d::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomPanelTab {
    Animation,
    DebugLog,
}

pub struct App {
    pub i18n: I18n,
    pub camera: OrbitCamera,
    pub canvas: ViewportCanvas,
    pub(crate) fonts_configured: bool,
    pub file_tree: FileTree,
    pub bvh_file_tree: BvhFileTree,
    pub log: LogViewer,
    pub page: Page,
    pub glb: Option<GlbDocument>,
    pub glb_path: Option<PathBuf>,
    pub bvh_target_glb: Option<GlbDocument>,
    pub bvh_target_path: Option<PathBuf>,
    pub bvh: Option<BvhDocument>,
    pub bvh_path: Option<PathBuf>,
    pub mapping: Option<MappingFile>,
    pub mapping_path: Option<PathBuf>,
    pub retarget_plan: Option<RetargetPlan>,
    pub mapping_report: Option<MappingValidation>,
    pub mapping_suggestions: Vec<MappingSuggestion>,
    pub retarget_mapping: Option<SkeletonMapping>,
    pub retarget_mapping_path: Option<PathBuf>,
    pub retarget_validation: Option<MappingValidationReport>,
    pub retarget_source_skin_index: usize,
    pub retarget_target_skin_index: usize,
    pub glb_retarget_target: Option<GlbDocument>,
    pub glb_retarget_target_path: Option<PathBuf>,
    pub glb_retarget_preview_active: bool,
    pub(crate) pending_glb_retarget_runtime: Option<AnimationRuntime>,
    pub retarget_root_motion: bool,
    pub retarget_normalize_initial_heading: bool,
    pub bvh_up_axis: String,
    pub bvh_forward_axis: String,
    pub bvh_unit: String,
    pub orientation_euler_degrees: [f32; 3],
    pub root_scale: f32,
    pub root_translation: [f32; 3],
    pub bake_root_transform: bool,
    pub(crate) root_preview_dirty: bool,
    pub(crate) root_preview_error: Option<String>,
    pub trim_enabled: bool,
    pub trim_animation: usize,
    pub trim_start: f32,
    pub trim_end: f32,
    pub glb_animation_index: usize,
    pub glb_animation_time: f32,
    pub glb_animation_playing: bool,
    pub glb_animation_loop: bool,
    pub glb_animation_rate: f32,
    pub smart_loop_enabled: bool,
    pub smart_loop_transition: f32,
    pub(crate) bottom_panel_tab: BottomPanelTab,
    pub texture_mesh: usize,
    pub texture_primitive: usize,
    pub texture_slot: TextureSlot,
    pub texture_duplicate_shared: bool,
    pub bvh_trim_start: f32,
    pub bvh_trim_end: f32,
    pub bvh_frame: usize,
    pub bvh_playing: bool,
    pub bvh_playback_speed: f32,
    pub bvh_reduce_keys: bool,
    pub bvh_key_tolerance: f32,
    pub(crate) show_about: bool,
    pub(crate) about_icon: Option<three_d::egui::TextureHandle>,
    pub(crate) task_busy: bool,
    last_frame_time: Instant,
    pub(crate) bvh_playback_accumulator: f32,
    pub(crate) glb_animation_accumulator: f32,
    reload_request: Option<GlbReloadKind>,
    pending_auto_play: bool,
    pending_animation_selection: Option<usize>,
    pub(crate) needs_bvh_skeleton_reload: bool,
    pub(crate) bvh_camera_focus_pending: bool,
    pub(crate) needs_bvh_target_reload: bool,
    pub(crate) needs_save: bool,
    quit_requested: bool,
    pub(crate) task_rx: Option<mpsc::Receiver<ExportTaskResult>>,
    pub converter_file_tree: FileTree,
    pub blender_path: Option<String>,
    pub(crate) converter_busy: bool,
    pub(crate) converter_rx: Option<mpsc::Receiver<ConverterMessage>>,
    pub(crate) converter_results: Vec<ConverterFileState>,
}

pub(crate) struct ExportTaskResult {
    pub(crate) kind: String,
    pub(crate) path: PathBuf,
    pub(crate) result: Result<(), String>,
}

impl App {
    pub fn new(
        context: &Context,
        viewport: Viewport,
        prefs: &UserPreferences,
    ) -> Self {
        let mut canvas = ViewportCanvas::new(context);
        canvas.apply_view_prefs(&prefs.view);
        let mut file_tree = FileTree::new();
        file_tree.apply_prefs(&prefs.file_tree);
        let mut bvh_file_tree = BvhFileTree::new();
        if let Some(root) = file_tree.root().cloned() {
            bvh_file_tree.open_folder(root);
        }
        let mut converter_file_tree = FileTree::new();
        converter_file_tree.set_accepted_extensions(vec![
            "fbx".to_owned(),
            "obj".to_owned(),
            "blend".to_owned(),
        ]);
        converter_file_tree.apply_prefs(&FileTreePreferences {
            show_all_files: prefs.converter.show_all_files,
            last_opened_directory: prefs
                .converter
                .last_opened_directory
                .clone(),
        });
        if let Some(root) = file_tree.root().cloned() {
            if converter_file_tree.root().is_none() {
                converter_file_tree.open_folder(root);
            }
        }
        let mut log = LogViewer::new();
        log.apply_prefs(&prefs.log_viewer);
        Self {
            i18n: I18n::new(prefs.language),
            camera: OrbitCamera::new(viewport),
            canvas,
            fonts_configured: false,
            file_tree,
            bvh_file_tree,
            log,
            page: Page::GlbEditor,
            glb: None,
            glb_path: None,
            bvh_target_glb: None,
            bvh_target_path: None,
            bvh: None,
            bvh_path: None,
            mapping: None,
            mapping_path: None,
            retarget_plan: None,
            mapping_report: None,
            mapping_suggestions: Vec::new(),
            retarget_mapping: None,
            retarget_mapping_path: None,
            retarget_validation: None,
            retarget_source_skin_index: 0,
            retarget_target_skin_index: 0,
            glb_retarget_target: None,
            glb_retarget_target_path: None,
            glb_retarget_preview_active: false,
            pending_glb_retarget_runtime: None,
            retarget_root_motion: true,
            retarget_normalize_initial_heading: false,
            bvh_up_axis: "Y".to_owned(),
            bvh_forward_axis: "-Z".to_owned(),
            bvh_unit: "cm".to_owned(),
            orientation_euler_degrees: [0.0, 0.0, 0.0],
            root_scale: 1.0,
            root_translation: [0.0, 0.0, 0.0],
            bake_root_transform: true,
            root_preview_dirty: false,
            root_preview_error: None,
            trim_enabled: false,
            trim_animation: 0,
            trim_start: 0.0,
            trim_end: 1.0,
            glb_animation_index: 0,
            glb_animation_time: 0.0,
            glb_animation_playing: false,
            glb_animation_loop: true,
            glb_animation_rate: 1.0,
            smart_loop_enabled: false,
            smart_loop_transition: 0.15,
            bottom_panel_tab: BottomPanelTab::DebugLog,
            texture_mesh: 0,
            texture_primitive: 0,
            texture_slot: TextureSlot::BaseColor,
            texture_duplicate_shared: true,
            bvh_trim_start: 0.0,
            bvh_trim_end: 1.0,
            bvh_frame: 0,
            bvh_playing: false,
            bvh_playback_speed: 1.0,
            bvh_reduce_keys: false,
            bvh_key_tolerance: 0.001,
            show_about: false,
            about_icon: None,
            task_busy: false,
            last_frame_time: Instant::now(),
            bvh_playback_accumulator: 0.0,
            glb_animation_accumulator: 0.0,
            reload_request: None,
            pending_auto_play: false,
            pending_animation_selection: None,
            needs_bvh_skeleton_reload: false,
            bvh_camera_focus_pending: false,
            needs_bvh_target_reload: false,
            needs_save: false,
            quit_requested: false,
            task_rx: None,
            converter_file_tree,
            blender_path: prefs.converter.blender_path.clone(),
            converter_busy: false,
            converter_rx: None,
            converter_results: Vec::new(),
        }
    }

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn preview_glb(&mut self, path: &Path) {
        self.page = Page::GlbEditor;
        self.glb_retarget_preview_active = false;
        self.pending_glb_retarget_runtime = None;
        self.canvas.clear_glb_skeleton();
        self.canvas.clear_bvh_skeleton();
        self.canvas.clear_target_skeleton();
        self.glb_path = Some(path.to_path_buf());
        self.pending_animation_selection = None;
        self.reset_root_preview();
        self.reset_glb_animation_rate();
        self.smart_loop_enabled = false;
        self.smart_loop_transition = 0.15;
        self.trim_enabled = false;
        self.trim_animation = 0;
        self.trim_start = 0.0;
        self.trim_end = 1.0;
        self.request_glb_reload(GlbReloadKind::OpenModel);
        self.pending_auto_play = true;
    }

    pub(crate) fn request_glb_reload(&mut self, requested: GlbReloadKind) {
        self.reload_request =
            Some(merge_glb_reload_kind(self.reload_request, requested));
    }

    pub fn poll_tasks(&mut self) {
        self.poll_converter();
        let Some(receiver) = self.task_rx.as_ref() else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.task_busy = false;
                self.task_rx = None;
                match result.result {
                    Ok(()) => {
                        self.file_tree.refresh();
                        self.bvh_file_tree.refresh();
                        let prefix = task_log_prefix(&result.kind);
                        self.log.append(&format!(
                            "{prefix} Exported {} {}",
                            result.kind,
                            result.path.display()
                        ));
                    }
                    Err(error) => {
                        let prefix = task_log_prefix(&result.kind);
                        self.log.append(&format!(
                            "{prefix} Export failed: {error}"
                        ));
                    }
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.task_busy = false;
                self.task_rx = None;
                self.log.append("[bvh_studio] Export worker disconnected");
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    pub fn reload_model_if_needed(&mut self, context: &Context) {
        if let Some(reload_kind) = self.reload_request.take() {
            self.bottom_panel_tab = BottomPanelTab::DebugLog;
            let Some(path) = self.glb_path.clone() else {
                self.canvas.clear_glb();
                self.canvas.clear_bvh_skeleton();
                self.glb = None;
                self.bottom_panel_tab = BottomPanelTab::DebugLog;
                self.reset_glb_animation_state();
                return;
            };
            let document = match self.glb.take() {
                Some(document)
                    if document.source_path.as_ref() == Some(&path) =>
                {
                    Ok(document)
                }
                _ => GlbDocument::load(&path),
            };
            match document {
                Ok(document) => {
                    self.log.append(&format!(
                        "[glb_editor] Loaded {}",
                        path.display()
                    ));
                    let preview_document = if self.trim_enabled
                        || self.smart_loop_enabled
                    {
                        let mut preview = document.clone();
                        let result = (|| {
                            if self.trim_enabled {
                                preview
                                    .apply(EditOperation::TrimAnimation {
                                        animation: self.trim_animation,
                                        start: self.trim_start,
                                        end: self.trim_end,
                                    })
                                    .map_err(|error| error.to_string())?;
                            }
                            if self.smart_loop_enabled {
                                let report = preview
                                    .smart_loop_animation(
                                        self.glb_animation_index,
                                        SmartLoopOptions {
                                            transition_seconds: self
                                                .smart_loop_transition,
                                        },
                                    )
                                    .map_err(|error| error.to_string())?;
                                if !report.already_looped {
                                    self.log.append(&format!(
                                        "[glb_editor] Smart LOOP preview added {:.3}s and {} keyframes",
                                        report.new_duration
                                            - report.original_duration,
                                        report.added_keyframes
                                    ));
                                }
                            }
                            Ok::<_, String>(preview)
                        })();
                        match result {
                            Ok(preview) => preview,
                            Err(error) => {
                                self.log.append(&format!(
                                    "[glb_editor] Animation preview setting unavailable: {error}"
                                ));
                                document.clone()
                            }
                        }
                    } else {
                        document.clone()
                    };
                    let preview_path = if preview_document.dirty {
                        let path = std::env::temp_dir()
                            .join("aio-asset-normalizer-preview.glb");
                        match preview_document.export_atomic(&path) {
                            Ok(()) => path,
                            Err(error) => {
                                self.log.append(&format!(
                                    "[glb_editor] Preview export failed: {error}"
                                ));
                                path
                            }
                        }
                    } else {
                        path.clone()
                    };
                    match self.canvas.load_glb(context, &preview_path) {
                        Ok(()) => {
                            self.glb = Some(document);
                            if self.canvas.has_glb_skeleton() {
                                let source = self
                                    .canvas
                                    .animation_runtime()
                                    .map(|runtime| {
                                        if runtime.preview_uses_skin() {
                                            "Skin joints"
                                        } else {
                                            "first-scene nodes"
                                        }
                                    })
                                    .unwrap_or("scene nodes");
                                let status = if self
                                    .canvas
                                    .is_glb_skeleton_only_preview()
                                {
                                    "Skeleton-only preview ready"
                                } else {
                                    "GLB skeleton preview ready"
                                };
                                self.log.append(&format!(
                                    "[glb_editor] {status} ({source})"
                                ));
                            }
                            let skin_count = self
                                .glb
                                .as_ref()
                                .map(|document| document.summary().skins)
                                .unwrap_or_default();
                            if skin_count == 0 {
                                self.retarget_source_skin_index = 0;
                            } else {
                                self.retarget_source_skin_index = self
                                    .retarget_source_skin_index
                                    .min(skin_count - 1);
                            }
                            self.refresh_glb_retarget_mapping();
                            if reload_kind == GlbReloadKind::OpenModel {
                                self.camera.reset();
                            }
                            self.reset_glb_animation_state();
                            if !self.canvas.animation_clips().is_empty() {
                                self.bottom_panel_tab =
                                    BottomPanelTab::Animation;
                            }
                            let selected = self
                                .pending_animation_selection
                                .take()
                                .filter(|index| {
                                    self.canvas
                                        .animation_clips()
                                        .get(*index)
                                        .is_some_and(|clip| clip.is_playable())
                                })
                                .or_else(|| {
                                    self.first_playable_glb_animation()
                                });
                            if let Some(index) = selected {
                                self.glb_animation_index = index;
                                if let Err(error) =
                                    self.update_glb_animation_preview()
                                {
                                    self.log.append(&format!(
                                        "[glb_editor] Animation preview failed: {error}"
                                    ));
                                }
                            }
                            self.frame_glb_preview(context, reload_kind);
                            if self.pending_auto_play
                                && self.first_playable_glb_animation().is_some()
                            {
                                self.glb_animation_playing = true;
                            }
                            self.pending_auto_play = false;
                        }
                        Err(error) => {
                            self.glb = Some(document);
                            self.pending_auto_play = false;
                            self.bottom_panel_tab = BottomPanelTab::DebugLog;
                            self.log.append(&format!(
                                "[glb_editor] Preview failed: {error}"
                            ));
                        }
                    }
                }
                Err(error) => {
                    self.pending_auto_play = false;
                    self.log
                        .append(&format!("[glb_editor] Load failed: {error}"));
                }
            }
        }

        if self.page == Page::BvhStudio {
            self.reload_bvh_target_preview(context);
        }
        if let Some(runtime) = self.pending_glb_retarget_runtime.take() {
            if let Some(path) = self.glb_retarget_target_path.as_ref() {
                match self.canvas.load_glb_with_runtime_for_target_preview(
                    context,
                    path,
                    runtime.clone(),
                ) {
                    Ok(()) => {
                        self.glb_retarget_preview_active = true;
                        self.glb_animation_index = 0;
                        self.glb_animation_time = 0.0;
                        self.glb_animation_playing = false;
                        self.log
                            .append("[glb_retarget] Retarget preview ready");
                        self.initialize_glb_retarget_preview_skeleton(context);
                    }
                    Err(error) => {
                        self.log.append(&format!(
                            "[glb_retarget] Target Mesh preview unavailable; using skeleton-only playback: {error}"
                        ));
                        self.canvas.load_skeleton_runtime(runtime);
                        self.glb_retarget_preview_active = true;
                        self.glb_animation_index = 0;
                        self.glb_animation_time = 0.0;
                        self.glb_animation_playing = false;
                        self.initialize_glb_retarget_preview_skeleton(context);
                    }
                }
            }
        }

        if self.needs_bvh_skeleton_reload {
            self.needs_bvh_skeleton_reload = false;
            if let Some(document) = self.bvh.as_ref() {
                let frame =
                    self.bvh_frame.min(document.frames.len().saturating_sub(1));
                match self.bvh_preview_pose(document, frame) {
                    Ok(pose) => {
                        let positions = pose.positions.clone();
                        self.canvas.set_bvh_skeleton_pose(context, &pose);
                        self.canvas.set_guide_scale(
                            context,
                            self.canvas
                                .skeleton
                                .as_ref()
                                .map(|skeleton| skeleton.metrics().height)
                                .unwrap_or(1.0),
                        );
                        if self.bvh_camera_focus_pending {
                            if let Some((minimum, maximum)) =
                                self.canvas.preview_bounds()
                            {
                                self.camera.focus_on_bounds(minimum, maximum);
                            } else {
                                self.camera.focus_on_points(&positions);
                            }
                            self.bvh_camera_focus_pending = false;
                            self.log.append(&format!(
                                "[bvh_studio] Focused preview on {} converted joints (unit={})",
                                positions.len(),
                                self.bvh_unit
                            ));
                        }
                        if self
                            .retarget_validation
                            .as_ref()
                            .is_some_and(|report| report.is_valid())
                        {
                            if let Err(error) =
                                self.canvas.update_glb_animation(
                                    0,
                                    frame as f32 * document.frame_time,
                                )
                            {
                                self.log.append(&format!(
                                    "[retarget] Target animation preview failed: {error}"
                                ));
                            }
                            if let Some(target) = self.bvh_target_glb.as_ref() {
                                if let Ok(skin) = target.skin_data_at(
                                    self.retarget_target_skin_index,
                                ) {
                                    if let Err(error) = self
                                        .canvas
                                        .update_target_skeleton_animation(
                                            context,
                                            0,
                                            frame as f32 * document.frame_time,
                                            &skin.joints,
                                        )
                                    {
                                        self.log.append(&format!(
                                            "[retarget] Target skeleton overlay failed: {error}"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    Err(error) => self.log.append(&format!(
                        "[bvh_studio] Skeleton preview failed: {error}"
                    )),
                }
            } else {
                self.canvas.clear_bvh_skeleton();
            }
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame_time);
        self.last_frame_time = now;
        if self.bvh_playing {
            let frame_time = self
                .bvh
                .as_ref()
                .map(|document| document.frame_time)
                .unwrap_or_default();
            if frame_time > 0.0 {
                self.bvh_playback_accumulator +=
                    elapsed.as_secs_f32() * self.bvh_playback_speed.max(0.01);
                if self.bvh_playback_accumulator >= frame_time {
                    let steps = (self.bvh_playback_accumulator / frame_time)
                        .floor() as usize;
                    self.bvh_playback_accumulator -= steps as f32 * frame_time;
                    let frame_count = self
                        .bvh
                        .as_ref()
                        .map(|document| document.frames.len())
                        .unwrap_or_default();
                    if frame_count > 0 {
                        self.set_bvh_frame(
                            (self.bvh_frame + steps) % frame_count,
                        );
                    }
                }
            }
        }
        if self.page == Page::GlbEditor && self.glb_animation_playing {
            let duration = self.glb_animation_duration();
            if duration > 0.0 {
                self.glb_animation_accumulator +=
                    elapsed.as_secs_f32() * self.glb_animation_rate.max(0.01);
                self.glb_animation_time += self.glb_animation_accumulator;
                self.glb_animation_accumulator = 0.0;
                if self.glb_animation_loop {
                    self.glb_animation_time =
                        self.glb_animation_time.rem_euclid(duration);
                } else if self.glb_animation_time >= duration {
                    self.glb_animation_time = duration;
                    self.glb_animation_playing = false;
                }
                if let Err(error) = self.update_glb_animation_preview() {
                    self.glb_animation_playing = false;
                    self.log.append(&format!(
                        "[glb_editor] Animation playback failed: {error}"
                    ));
                }
            } else {
                self.glb_animation_playing = false;
            }
        }
        self.update_root_preview_if_needed();
        if elapsed.as_secs_f32() > 0.5 {
            self.last_frame_time = now;
        }
    }

    pub(crate) fn dispatch_action(&mut self, action: &MenuAction) {
        match action {
            MenuAction::ImportGlb => self.import_glb(),
            MenuAction::ImportBvh => self.import_bvh(),
            MenuAction::ImportMapping => self.import_mapping(),
            MenuAction::ExportMapping => self.export_mapping(),
            MenuAction::Save => self.export_glb(),
            MenuAction::Export => match self.page {
                Page::GlbEditor => self.export_glb(),
                Page::BvhStudio => self.export_bvh(),
                Page::FbxConverter => self.log.append(&format!(
                    "{} Use Convert on the FBX Converter page; this page \
                     has no document export",
                    crate::app_fbx_converter::CONVERTER_LOG_PREFIX
                )),
            },
            MenuAction::ExportBvhGlb => self.export_bvh_glb(false),
            MenuAction::ExportBvhAnimationClip => self.export_bvh_glb(true),
            MenuAction::ClearFileList => {
                self.file_tree.clear();
                self.bvh_file_tree.clear();
                self.converter_file_tree.clear();
                self.converter_results.clear();
                self.glb = None;
                self.glb_path = None;
                self.canvas.clear_glb();
                self.canvas.clear_target_skeleton();
                self.canvas.clear_bvh_skeleton();
                self.bvh_target_glb = None;
                self.bvh_target_path = None;
                self.bvh = None;
                self.bvh_path = None;
                self.mapping = None;
                self.mapping_path = None;
                self.retarget_plan = None;
                self.mapping_report = None;
                self.mapping_suggestions.clear();
                self.glb_retarget_target = None;
                self.glb_retarget_target_path = None;
                self.retarget_mapping = None;
                self.retarget_mapping_path = None;
                self.retarget_validation = None;
                self.glb_retarget_preview_active = false;
                self.pending_glb_retarget_runtime = None;
                self.needs_bvh_target_reload = false;
                self.needs_bvh_skeleton_reload = false;
                self.bvh_camera_focus_pending = false;
                self.bvh_playing = false;
                self.bvh_frame = 0;
                self.reset_root_preview();
                self.reset_glb_animation_state();
                self.reset_glb_animation_rate();
                self.smart_loop_enabled = false;
                self.smart_loop_transition = 0.15;
                self.trim_enabled = false;
                self.trim_animation = 0;
                self.trim_start = 0.0;
                self.trim_end = 1.0;
                self.bottom_panel_tab = BottomPanelTab::DebugLog;
                self.needs_save = true;
            }
            MenuAction::ResetCamera => self.camera.reset(),
            MenuAction::ToggleGrid => {
                self.canvas.show_grid = !self.canvas.show_grid
            }
            MenuAction::ToggleAxes => {
                self.canvas.show_axes = !self.canvas.show_axes
            }
            MenuAction::ToggleOrigin => {
                self.canvas.show_origin = !self.canvas.show_origin
            }
            MenuAction::OpenGlbEditor => {
                self.page = Page::GlbEditor;
                self.canvas.clear_glb_skeleton();
                if self.glb_retarget_preview_active {
                    self.exit_glb_retarget_preview();
                } else {
                    self.request_glb_reload(GlbReloadKind::OpenModel);
                }
            }
            MenuAction::OpenFbxConverter => {
                self.page = Page::FbxConverter;
                self.canvas.clear_glb_skeleton();
                if self.glb_retarget_preview_active {
                    self.exit_glb_retarget_preview();
                }
            }
            MenuAction::OpenBvhStudio => {
                self.page = Page::BvhStudio;
                self.canvas.show_origin = false;
                self.canvas.clear_glb_skeleton();
                if self.glb_retarget_preview_active {
                    self.exit_glb_retarget_preview();
                }
                if self.bvh.is_some() {
                    self.needs_bvh_skeleton_reload = true;
                    self.bvh_camera_focus_pending = true;
                }
                self.needs_bvh_target_reload = true;
                self.refresh_v2_retarget_mapping();
            }
            MenuAction::About => self.show_about = true,
            MenuAction::SetLanguage(preference) => {
                self.i18n.set_preference(*preference);
                self.needs_save = true;
            }
            MenuAction::Quit => {
                preferences::save(&self.collect_preferences());
                self.quit_requested = true;
            }
        }
    }

    pub(crate) fn standardize(&mut self) {
        let Some(document) = self.glb.as_mut() else {
            self.log
                .append("[glb_editor] Open a GLB before standardizing");
            return;
        };
        match document.standardize(&StandardizationProfile::default()) {
            Ok(()) => self
                .log
                .append("[glb_editor] GLB matches the default contract"),
            Err(error) => self.log.append(&format!("[glb_editor] {error}")),
        }
    }

    pub(crate) fn trim_setting_changed(&mut self) {
        self.pending_animation_selection = Some(self.glb_animation_index);
        self.request_glb_reload(GlbReloadKind::EditedModel);
    }

    pub(crate) fn smart_loop_setting_changed(&mut self) {
        self.pending_animation_selection = Some(self.glb_animation_index);
        self.request_glb_reload(GlbReloadKind::EditedModel);
    }

    pub(crate) fn replace_glb_texture(&mut self) {
        let Some(document) = self.glb.as_mut() else {
            self.log
                .append("[glb_editor] Open a GLB before replacing a texture");
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG or JPEG", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        match document.replace_texture(
            PrimitiveTarget {
                mesh: self.texture_mesh,
                primitive: self.texture_primitive,
            },
            self.texture_slot,
            &path,
            self.texture_duplicate_shared,
        ) {
            Ok(()) => {
                self.log.append(&format!(
                    "[glb_editor] Replaced {} texture with {}",
                    self.texture_slot.label(),
                    path.display()
                ));
                self.request_glb_reload(GlbReloadKind::EditedModel);
            }
            Err(error) => self.log.append(&format!("[glb_editor] {error}")),
        }
    }

    pub(crate) fn trim_bvh(&mut self) {
        let Some(document) = self.bvh.as_mut() else {
            self.log.append("[bvh_studio] Open a BVH before trimming");
            return;
        };
        let trimmed =
            match document.trim(self.bvh_trim_start, self.bvh_trim_end) {
                Ok(()) => {
                    self.log.append("[bvh_studio] Trimmed BVH frames");
                    true
                }
                Err(error) => {
                    self.log.append(&format!("[bvh_studio] {error}"));
                    false
                }
            };
        if trimmed {
            self.bvh_frame = 0;
            self.bvh_playback_accumulator = 0.0;
            self.needs_bvh_skeleton_reload = true;
        }
    }

    pub(crate) fn set_bvh_frame(&mut self, frame: usize) {
        if self.bvh_frame != frame {
            self.bvh_frame = frame;
            self.needs_bvh_skeleton_reload = true;
        }
    }

    fn import_glb(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB", &["glb"])
            .pick_file()
        else {
            return;
        };
        if self.page == Page::BvhStudio {
            self.load_bvh_target(&path);
            return;
        }
        if let Some(parent) = path.parent() {
            self.file_tree.open_folder(parent.to_path_buf());
            self.file_tree.select_file(&path);
        }
        self.preview_glb(&path);
    }
}

fn task_log_prefix(kind: &str) -> &'static str {
    if kind.starts_with("GLB") {
        "[glb_retarget]"
    } else if kind.starts_with("Agent") {
        "[retarget_agent]"
    } else {
        "[bvh_studio]"
    }
}
