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
        StandardizationProfile, TextureSlot,
    },
    i18n::I18n,
    preferences::{self, UserPreferences},
    ui::{
        file_tree::FileTree,
        log_viewer::LogViewer,
        main_panel,
        menu_bar::{MenuAction, Page},
    },
    viewport::{camera::OrbitCamera, canvas::ViewportCanvas},
};
use three_d::*;

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
    pub rotation_axis: [f32; 3],
    pub rotation_degrees: f32,
    pub root_scale: f32,
    pub root_translation: [f32; 3],
    pub trim_animation: usize,
    pub trim_start: f32,
    pub trim_end: f32,
    pub glb_animation_index: usize,
    pub glb_animation_time: f32,
    pub glb_animation_playing: bool,
    pub glb_animation_loop: bool,
    pub glb_animation_speed: f32,
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
    needs_reload: bool,
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
            rotation_axis: [0.0, 1.0, 0.0],
            rotation_degrees: 0.0,
            root_scale: 1.0,
            root_translation: [0.0, 0.0, 0.0],
            trim_animation: 0,
            trim_start: 0.0,
            trim_end: 1.0,
            glb_animation_index: 0,
            glb_animation_time: 0.0,
            glb_animation_playing: false,
            glb_animation_loop: true,
            glb_animation_speed: 1.0,
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
            needs_reload: false,
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
        self.needs_reload = true;
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
                    Ok(()) => self.log.append(&format!(
                        "[bvh_studio] Exported {} {}",
                        result.kind,
                        result.path.display()
                    )),
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
        if self.needs_reload {
            self.needs_reload = false;
            let Some(path) = self.glb_path.clone() else {
                self.canvas.clear_glb();
                self.canvas.clear_bvh_skeleton();
                self.glb = None;
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
                    let preview_path = if document.dirty {
                        let path = std::env::temp_dir()
                            .join("aio-asset-normalizer-preview.glb");
                        match document.export_atomic(&path) {
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
                            self.camera.reset();
                            self.reset_glb_animation_state();
                            if let Some(index) =
                                self.first_playable_glb_animation()
                            {
                                self.glb_animation_index = index;
                                if let Err(error) =
                                    self.update_glb_animation_preview()
                                {
                                    self.log.append(&format!(
                                        "[glb_editor] Animation preview failed: {error}"
                                    ));
                                }
                            }
                        }
                        Err(error) => {
                            self.glb = Some(document);
                            self.log.append(&format!(
                                "[glb_editor] Preview failed: {error}"
                            ));
                        }
                    }
                }
                Err(error) => self
                    .log
                    .append(&format!("[glb_editor] Load failed: {error}")),
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
                    elapsed.as_secs_f32() * self.glb_animation_speed.max(0.01);
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
                self.reset_glb_animation_state();
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

    pub(crate) fn apply_rotation(&mut self) {
        let Some(document) = self.glb.as_mut() else {
            self.log.append("[glb_editor] Open a GLB before editing");
            return;
        };
        let operation = EditOperation::RotateRoots {
            axis: self.rotation_axis,
            degrees: self.rotation_degrees,
        };
        match document.apply(operation) {
            Ok(()) => {
                self.log.append("[glb_editor] Applied root rotation");
                self.needs_reload = true;
            }
            Err(error) => self.log.append(&format!("[glb_editor] {error}")),
        }
    }

    pub(crate) fn apply_scale(&mut self) {
        let Some(document) = self.glb.as_mut() else {
            self.log.append("[glb_editor] Open a GLB before editing");
            return;
        };
        match document.apply(EditOperation::ScaleRoots {
            factor: self.root_scale,
        }) {
            Ok(()) => {
                self.log.append("[glb_editor] Applied root scale");
                self.needs_reload = true;
            }
            Err(error) => self.log.append(&format!("[glb_editor] {error}")),
        }
    }

    pub(crate) fn apply_translation(&mut self) {
        let Some(document) = self.glb.as_mut() else {
            self.log.append("[glb_editor] Open a GLB before editing");
            return;
        };
        match document.apply(EditOperation::TranslateRoots {
            offset: self.root_translation,
        }) {
            Ok(()) => {
                self.log.append("[glb_editor] Applied root translation");
                self.needs_reload = true;
            }
            Err(error) => self.log.append(&format!("[glb_editor] {error}")),
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

    pub(crate) fn trim_glb_animation(&mut self) {
        let Some(document) = self.glb.as_mut() else {
            self.log
                .append("[glb_editor] Open a GLB before trimming animation");
            return;
        };
        match document.apply(EditOperation::TrimAnimation {
            animation: self.trim_animation,
            start: self.trim_start,
            end: self.trim_end,
        }) {
            Ok(()) => {
                self.log.append("[glb_editor] Trimmed animation keyframes");
                self.needs_reload = true;
            }
            Err(error) => self.log.append(&format!("[glb_editor] {error}")),
        }
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
                self.needs_reload = true;
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
            match GlbDocument::load(&path) {
                Ok(document) => {
                    self.log.append(&format!(
                        "[bvh_studio] Loaded target GLB {}",
                        path.display()
                    ));
                    self.bvh_target_glb = Some(document);
                    self.bvh_target_path = Some(path);
                    self.refresh_retarget_plan();
                }
                Err(error) => self.log.append(&format!("[bvh_studio] {error}")),
            }
            return;
        }
        if let Some(parent) = path.parent() {
            self.file_tree.open_folder(parent.to_path_buf());
            self.file_tree.select_file(&path);
        }
        self.preview_glb(&path);
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
        let Some(document) = self.glb.as_ref() else {
            self.log.append("[glb_editor] Nothing to export");
            return;
        };
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
        if path.exists() {
            self.log.append(&format!(
                "[glb_editor] Refusing to overwrite existing file {}",
                path.display()
            ));
            return;
        }
        match document.export_atomic(&path) {
            Ok(()) => self
                .log
                .append(&format!("[glb_editor] Exported {}", path.display())),
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
        if path.exists() {
            self.log.append(&format!(
                "[bvh_studio] Refusing to overwrite existing file {}",
                path.display()
            ));
            return;
        }
        match bvh::save_mapping(&path, mapping) {
            Ok(()) => self.log.append(&format!(
                "[bvh_studio] Exported mapping {}",
                path.display()
            )),
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
        if path.exists() {
            self.log.append(&format!(
                "[bvh_studio] Refusing to overwrite existing file {}",
                path.display()
            ));
            return;
        }
        match document.write(&path) {
            Ok(()) => self
                .log
                .append(&format!("[bvh_studio] Exported {}", path.display())),
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
        if path.exists() {
            self.log.append(&format!(
                "[bvh_studio] Refusing to overwrite existing file {}",
                path.display()
            ));
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

    pub fn collect_preferences(&self) -> UserPreferences {
        UserPreferences {
            version: 1,
            language: self.i18n.preference(),
            view: self.canvas.to_view_prefs(),
            file_tree: self.file_tree.to_prefs(),
            log_viewer: self.log.to_prefs(),
        }
    }

    pub fn render_ui(
        &mut self,
        ui: &mut three_d::egui::Ui,
        window_width: u32,
    ) -> three_d::egui::Rect {
        let rect = main_panel::render_ui(self, ui, window_width);
        if self.needs_save && !self.quit_requested {
            self.needs_save = false;
            preferences::save(&self.collect_preferences());
        }
        rect
    }

    pub(crate) fn glb_animation_entries(
        &self,
    ) -> Vec<(String, f32, bool, String)> {
        self.canvas
            .animation_clips()
            .iter()
            .map(|clip| {
                (
                    clip.name.clone(),
                    clip.duration,
                    clip.is_playable(),
                    clip.unsupported.join(", "),
                )
            })
            .collect()
    }

    pub(crate) fn glb_animation_duration(&self) -> f32 {
        self.canvas
            .animation_clips()
            .get(self.glb_animation_index)
            .map(|clip| clip.duration)
            .unwrap_or(0.0)
    }

    pub(crate) fn first_playable_glb_animation(&self) -> Option<usize> {
        self.canvas
            .animation_clips()
            .iter()
            .position(|clip| clip.is_playable())
    }

    pub(crate) fn select_glb_animation(&mut self, index: usize) {
        if self
            .canvas
            .animation_clips()
            .get(index)
            .is_none_or(|clip| !clip.is_playable())
        {
            self.glb_animation_playing = false;
            return;
        }
        self.glb_animation_index = index;
        self.glb_animation_time = 0.0;
        self.glb_animation_accumulator = 0.0;
        self.glb_animation_playing = false;
        if let Err(error) = self.update_glb_animation_preview() {
            self.log.append(&format!(
                "[glb_editor] Animation selection failed: {error}"
            ));
        }
    }

    pub(crate) fn set_glb_animation_time(&mut self, time: f32) {
        let duration = self.glb_animation_duration();
        self.glb_animation_time = time.clamp(0.0, duration.max(0.0));
        self.glb_animation_accumulator = 0.0;
        if let Err(error) = self.update_glb_animation_preview() {
            self.log.append(&format!(
                "[glb_editor] Animation seek failed: {error}"
            ));
        }
    }

    pub(crate) fn step_glb_animation(&mut self, direction: f32) {
        let duration = self.glb_animation_duration();
        if duration <= 0.0 {
            return;
        }
        let time = self.glb_animation_time + direction * (1.0 / 30.0);
        self.glb_animation_time = if self.glb_animation_loop {
            time.rem_euclid(duration)
        } else {
            time.clamp(0.0, duration)
        };
        self.glb_animation_playing = false;
        self.glb_animation_accumulator = 0.0;
        if let Err(error) = self.update_glb_animation_preview() {
            self.log.append(&format!(
                "[glb_editor] Animation step failed: {error}"
            ));
        }
    }

    pub(crate) fn update_glb_animation_preview(
        &mut self,
    ) -> Result<(), String> {
        if self
            .canvas
            .animation_clips()
            .get(self.glb_animation_index)
            .is_none()
        {
            return Ok(());
        }
        self.canvas.update_glb_animation(
            self.glb_animation_index,
            self.glb_animation_time,
        )
    }

    fn reset_glb_animation_state(&mut self) {
        self.glb_animation_index = 0;
        self.glb_animation_time = 0.0;
        self.glb_animation_playing = false;
        self.glb_animation_accumulator = 0.0;
    }
}
