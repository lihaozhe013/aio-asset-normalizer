use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use crate::modules::{
    bvh::{
        self, BvhDocument, MappingFile, MappingSuggestion, MappingValidation,
        RetargetPlan,
    },
    glb::{
        AnimationClipData, EditOperation, GlbDocument, PrimitiveTarget,
        SmartLoopOptions, StandardizationProfile, TextureSlot,
    },
    i18n::I18n,
    preferences::{self, UserPreferences},
    ui::{
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
    pub orientation_euler_degrees: [f32; 3],
    pub root_scale: f32,
    pub root_translation: [f32; 3],
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
    needs_bvh_skeleton_reload: bool,
    pub(crate) needs_save: bool,
    quit_requested: bool,
    task_rx: Option<mpsc::Receiver<ExportTaskResult>>,
}

struct ExportTaskResult {
    kind: String,
    path: PathBuf,
    result: Result<(), String>,
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
        let mut log = LogViewer::new();
        log.apply_prefs(&prefs.log_viewer);
        Self {
            i18n: I18n::new(prefs.language),
            camera: OrbitCamera::new(viewport),
            canvas,
            fonts_configured: false,
            file_tree,
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
            orientation_euler_degrees: [0.0, 0.0, 0.0],
            root_scale: 1.0,
            root_translation: [0.0, 0.0, 0.0],
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
            needs_save: false,
            quit_requested: false,
            task_rx: None,
        }
    }

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn preview_glb(&mut self, path: &Path) {
        self.page = Page::GlbEditor;
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

    fn request_glb_reload(&mut self, requested: GlbReloadKind) {
        self.reload_request =
            Some(merge_glb_reload_kind(self.reload_request, requested));
    }

    pub fn poll_tasks(&mut self) {
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
                        self.log.append(&format!(
                            "[bvh_studio] Exported {} {}",
                            result.kind,
                            result.path.display()
                        ));
                    }
                    Err(error) => self.log.append(&format!(
                        "[bvh_studio] Export failed: {error}"
                    )),
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
                            if self.pending_auto_play
                                && !self.canvas.animation_clips().is_empty()
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

        if self.needs_bvh_skeleton_reload {
            self.needs_bvh_skeleton_reload = false;
            if let Some(document) = self.bvh.as_ref() {
                let frame =
                    self.bvh_frame.min(document.frames.len().saturating_sub(1));
                match document.joint_positions(frame) {
                    Ok(positions) => {
                        let parents = document
                            .joints
                            .iter()
                            .map(|joint| joint.parent)
                            .collect::<Vec<_>>();
                        self.canvas
                            .set_bvh_skeleton(context, &positions, &parents);
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
            },
            MenuAction::ExportBvhGlb => self.export_bvh_glb(false),
            MenuAction::ExportBvhAnimationClip => self.export_bvh_glb(true),
            MenuAction::ClearFileList => {
                self.file_tree.clear();
                self.glb = None;
                self.glb_path = None;
                self.canvas.clear_glb();
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
            MenuAction::OpenGlbEditor => self.page = Page::GlbEditor,
            MenuAction::OpenBvhStudio => self.page = Page::BvhStudio,
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

    pub(crate) fn load_bvh_target(&mut self, path: &Path) {
        match GlbDocument::load(path) {
            Ok(document) => {
                self.log.append(&format!(
                    "[bvh_studio] Loaded target GLB {}",
                    path.display()
                ));
                self.bvh_target_glb = Some(document);
                self.bvh_target_path = Some(path.to_path_buf());
                self.refresh_retarget_plan();
            }
            Err(error) => self.log.append(&format!("[bvh_studio] {error}")),
        }
    }

    fn import_bvh(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("BVH", &["bvh"])
            .pick_file()
        else {
            return;
        };
        match BvhDocument::load(&path) {
            Ok(document) => {
                self.bvh_trim_end =
                    document.duration().max(document.frame_time);
                self.log.append(&format!(
                    "[bvh_studio] Loaded {} ({} joints, {} frames)",
                    path.display(),
                    document.joints.len(),
                    document.frames.len()
                ));
                self.bvh = Some(document);
                self.bvh_path = Some(path);
                self.bvh_frame = 0;
                self.bvh_playing = false;
                self.needs_bvh_skeleton_reload = true;
                self.page = Page::BvhStudio;
                self.refresh_retarget_plan();
            }
            Err(error) => self.log.append(&format!("[bvh_studio] {error}")),
        }
    }

    fn import_mapping(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Mapping JSON", &["json"])
            .pick_file()
        else {
            return;
        };
        match bvh::load_mapping(&path) {
            Ok(mapping) => {
                self.log.append(&format!(
                    "[bvh_studio] Loaded mapping {}",
                    path.display()
                ));
                self.retarget_plan = self
                    .bvh
                    .as_ref()
                    .and_then(|document| document.plan_retarget(&mapping).ok());
                self.mapping = Some(mapping);
                self.mapping_path = Some(path);
                self.page = Page::BvhStudio;
                self.refresh_retarget_plan();
            }
            Err(error) => self.log.append(&format!("[bvh_studio] {error}")),
        }
    }

    fn export_glb(&mut self) {
        if self.glb.is_none() {
            self.log.append("[glb_editor] Nothing to export");
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GLB", &["glb"])
            .set_file_name(
                self.glb_path
                    .as_ref()
                    .and_then(|path| path.file_stem())
                    .map(|stem| {
                        format!("{}_standardized.glb", stem.to_string_lossy())
                    })
                    .as_deref()
                    .unwrap_or("asset_standardized.glb"),
            )
            .save_file()
        else {
            return;
        };
        if is_source_path(&path, self.glb_path.as_deref()) {
            self.log
                .append("[glb_editor] Refusing to overwrite the source GLB");
            return;
        }
        let document = match self.build_glb_export_snapshot() {
            Ok(document) => document,
            Err(error) => {
                self.log
                    .append(&format!("[glb_editor] Export failed: {error}"));
                return;
            }
        };
        match document.export_atomic(&path) {
            Ok(()) => {
                self.file_tree.refresh();
                self.log.append(&format!(
                    "[glb_editor] Exported {}",
                    path.display()
                ));
            }
            Err(error) => self
                .log
                .append(&format!("[glb_editor] Export failed: {error}")),
        }
    }

    fn export_mapping(&mut self) {
        let Some(mapping) = self.mapping.as_ref() else {
            self.log.append("[bvh_studio] Nothing to export");
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Mapping JSON", &["json"])
            .set_file_name("mapping.json")
            .save_file()
        else {
            return;
        };
        if is_source_path(&path, self.mapping_path.as_deref()) {
            self.log.append(
                "[bvh_studio] Refusing to overwrite the source mapping file",
            );
            return;
        }
        match bvh::save_mapping(&path, mapping) {
            Ok(()) => {
                self.file_tree.refresh();
                self.log.append(&format!(
                    "[bvh_studio] Exported mapping {}",
                    path.display()
                ));
            }
            Err(error) => self.log.append(&format!(
                "[bvh_studio] Mapping export failed: {error}"
            )),
        }
    }

    fn export_bvh(&mut self) {
        let Some(document) = self.bvh.as_ref() else {
            self.log.append("[bvh_studio] Nothing to export");
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("BVH", &["bvh"])
            .set_file_name("animation_trimmed.bvh")
            .save_file()
        else {
            return;
        };
        if is_source_path(&path, self.bvh_path.as_deref()) {
            self.log
                .append("[bvh_studio] Refusing to overwrite the source BVH");
            return;
        }
        match document.write(&path) {
            Ok(()) => {
                self.file_tree.refresh();
                self.log.append(&format!(
                    "[bvh_studio] Exported {}",
                    path.display()
                ));
            }
            Err(error) => self
                .log
                .append(&format!("[bvh_studio] Export failed: {error}")),
        }
    }

    fn export_bvh_glb(&mut self, clip_only: bool) {
        if self.task_busy {
            return;
        }
        let Some(source) = self.bvh.clone() else {
            self.log
                .append("[bvh_studio] Open a BVH before exporting GLB");
            return;
        };
        let Some(mapping) = self.mapping.clone() else {
            self.log.append(
                "[bvh_studio] Load a Mapping JSON before exporting GLB",
            );
            return;
        };
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
        if is_source_path(&path, self.bvh_target_path.as_deref()) {
            self.log.append(
                "[bvh_studio] Refusing to overwrite the target source GLB",
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
        std::thread::spawn(move || {
            let result = (|| {
                let skin =
                    target.skin_data().map_err(|error| error.to_string())?;
                let clip = source
                    .retarget_to_skin(&mapping, &skin)
                    .map_err(|error| error.to_string())?;
                let mut clip = clip;
                if reduce_keys {
                    clip.reduce_keys(key_tolerance)
                        .map_err(|error| error.to_string())?;
                }
                let mut output = target;
                output
                    .append_animation(&AnimationClipData {
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

    fn refresh_retarget_plan(&mut self) {
        self.mapping_report = None;
        self.mapping_suggestions.clear();
        let Some(document) = self.bvh.as_ref() else {
            self.retarget_plan = None;
            return;
        };
        let Some(mapping) = self.mapping.as_ref() else {
            self.retarget_plan = None;
            return;
        };
        let Some(target) = self.bvh_target_glb.as_ref() else {
            self.retarget_plan = None;
            return;
        };
        let Ok(skin) = target.skin_data() else {
            self.retarget_plan = None;
            self.log.append(
                "[bvh_studio] Target GLB does not expose a usable Skin",
            );
            return;
        };
        let report = document.mapping_report(mapping, &skin);
        self.mapping_suggestions = document.suggest_mapping(&skin);
        self.retarget_plan = if report.is_valid() {
            document.plan_retarget(mapping).ok()
        } else {
            None
        };
        self.mapping_report = Some(report);
    }
}

fn is_source_path(path: &Path, source: Option<&Path>) -> bool {
    source.is_some_and(|source| same_path(path, source))
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());

    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}
