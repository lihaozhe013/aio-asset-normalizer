use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

use crate::modules::{
    bvh::{self, BvhDocument, MappingFile, RetargetPlan},
    glb::{
        AnimationClipData, EditOperation, GlbDocument, StandardizationProfile,
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
    pub rotation_axis: [f32; 3],
    pub rotation_degrees: f32,
    pub root_scale: f32,
    pub root_translation: [f32; 3],
    pub trim_animation: usize,
    pub trim_start: f32,
    pub trim_end: f32,
    pub bvh_trim_start: f32,
    pub bvh_trim_end: f32,
    pub(crate) show_about: bool,
    pub(crate) about_icon: Option<three_d::egui::TextureHandle>,
    pub(crate) task_busy: bool,
    last_frame_time: Instant,
    needs_reload: bool,
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
            rotation_axis: [0.0, 1.0, 0.0],
            rotation_degrees: 0.0,
            root_scale: 1.0,
            root_translation: [0.0, 0.0, 0.0],
            trim_animation: 0,
            trim_start: 0.0,
            trim_end: 1.0,
            bvh_trim_start: 0.0,
            bvh_trim_end: 1.0,
            show_about: false,
            about_icon: None,
            task_busy: false,
            last_frame_time: Instant::now(),
            needs_reload: false,
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
                self.canvas.model = None;
                self.glb = None;
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

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame_time);
        self.last_frame_time = now;
        if elapsed.as_secs_f32() > 0.5 {
            self.last_frame_time = now;
        }
    }

    pub(crate) fn dispatch_action(&mut self, action: &MenuAction) {
        match action {
            MenuAction::ImportGlb => self.import_glb(),
            MenuAction::ImportBvh => self.import_bvh(),
            MenuAction::ImportMapping => self.import_mapping(),
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
                self.canvas.model = None;
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

    pub(crate) fn trim_bvh(&mut self) {
        let Some(document) = self.bvh.as_mut() else {
            self.log.append("[bvh_studio] Open a BVH before trimming");
            return;
        };
        match document.trim(self.bvh_trim_start, self.bvh_trim_end) {
            Ok(()) => self.log.append("[bvh_studio] Trimmed BVH frames"),
            Err(error) => self.log.append(&format!("[bvh_studio] {error}")),
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
        std::thread::spawn(move || {
            let result = (|| {
                let skin =
                    target.skin_data().map_err(|error| error.to_string())?;
                let clip = source
                    .retarget_to_skin(&mapping, &skin)
                    .map_err(|error| error.to_string())?;
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
        let node_names = target.node_names();
        let skin_names = target.skin_names();
        if !skin_names.iter().any(|name| name == &mapping.target.skin)
            || !node_names.iter().any(|name| name == &mapping.target.root)
        {
            self.retarget_plan = None;
            self.log.append(
                "[bvh_studio] Mapping target Skin or root node was not found",
            );
            return;
        }
        self.retarget_plan = document.plan_retarget(mapping).ok();
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
}
