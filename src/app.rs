use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use crate::modules::{
    animation::AnimationPlayer,
    blender,
    preferences::{
        self, ConversionPreferences, FileTreePreferences, LogViewerPreferences, UserPreferences,
        ViewPreferences,
    },
    skeleton::Skeleton,
    ui::{
        config_panel::{NormalizationConfig, ScriptVersion, UpAxis},
        file_tree::FileTree,
        log_viewer::LogViewer,
        main_panel,
        menu_bar::MenuAction,
    },
    viewport::{camera::OrbitCamera, canvas::ViewportCanvas},
};
use three_d::*;

pub struct App {
    pub camera: OrbitCamera,
    pub canvas: ViewportCanvas,
    pub(crate) fonts_configured: bool,
    pub file_tree: FileTree,
    pub config: NormalizationConfig,
    pub log: LogViewer,
    conversion_rx: Option<mpsc::Receiver<String>>,
    pub(crate) converting: bool,
    last_output: Option<PathBuf>,
    needs_reload: bool,
    quit_requested: bool,
    pub(crate) show_about: bool,
    pub skeleton: Option<Skeleton>,
    pub animation_player: Option<AnimationPlayer>,
    last_frame_time: Instant,
    pub(crate) needs_save: bool,
}

impl App {
    pub fn new(context: &Context, viewport: Viewport, prefs: &UserPreferences) -> Self {
        let mut canvas = ViewportCanvas::new(context);
        canvas.show_grid = prefs.view.show_grid;
        canvas.show_axes = prefs.view.show_axes;
        canvas.show_origin = prefs.view.show_origin;
        canvas.show_bones = prefs.view.show_bones;
        canvas.model = None;

        let mut file_tree = FileTree::new();
        file_tree.show_all_files = prefs.file_tree.show_all_files;
        if let Some(ref dir) = prefs.file_tree.last_opened_directory {
            let path = PathBuf::from(dir);
            if path.exists() && path.is_dir() {
                file_tree.open_folder(path);
            }
        }

        let mut log = LogViewer::new();
        log.auto_scroll = prefs.log_viewer.auto_scroll;

        let config = prefs_to_config(&prefs.conversion);

        Self {
            camera: OrbitCamera::new(viewport),
            canvas,
            fonts_configured: false,
            file_tree,
            config,
            log,
            conversion_rx: None,
            converting: false,
            last_output: None,
            needs_reload: false,
            quit_requested: false,
            show_about: false,
            skeleton: None,
            animation_player: None,
            last_frame_time: Instant::now(),
            needs_save: false,
        }
    }

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    pub fn poll_tasks(&mut self) {
        if let Some(ref rx) = self.conversion_rx {
            let mut finished = false;
            loop {
                match rx.try_recv() {
                    Ok(msg) => {
                        if msg == "__CONVERSION_DONE__" {
                            finished = true;
                        } else {
                            self.log.append(&msg);
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
            if finished {
                self.conversion_rx = None;
                self.converting = false;
                self.needs_reload = true;
            }
        }
    }

    pub fn reload_model_if_needed(&mut self, context: &Context) {
        if self.needs_reload {
            self.needs_reload = false;
            if let Some(ref path) = self.last_output {
                if path.exists() {
                    match self.canvas.load_glb(context, path) {
                        Ok(()) => {
                            self.skeleton = Skeleton::from_glb(path).ok().or_else(|| {
                                self.log.append("[Info] No skeleton found (static mesh)");
                                None
                            });
                            self.animation_player = self.skeleton.as_ref().and_then(|skel| {
                                AnimationPlayer::from_glb(path, skel).ok().or_else(|| {
                                    self.log.append("[Info] No animations found");
                                    None
                                })
                            });
                            self.update_bone_visualization(context);
                        }
                        Err(e) => self.log.append(&format!("[Error] {}", e)),
                    }
                } else {
                    self.log
                        .append(&format!("[Error] Output not found: {}", path.display()));
                }
            }
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;

        if dt > 0.0 && dt < 0.5 {
            if let Some(ref mut anim) = self.animation_player {
                anim.advance(dt);
                if anim.playing {
                    self.update_bone_visualization(context);
                }
            }
        }

        self.last_frame_time = now;
    }

    pub fn update_bone_visualization(&mut self, context: &Context) {
        let (segments, joints) = self
            .animation_player
            .as_ref()
            .filter(|a| a.playing)
            .and_then(|a| self.skeleton.as_ref().map(|s| a.animated_bone_positions(s)))
            .or_else(|| {
                self.skeleton
                    .as_ref()
                    .map(|s| (s.bone_positions(), s.joint_positions()))
            })
            .unwrap_or_default();

        let highlighted = self.skeleton.as_ref().and_then(|s| s.highlighted_bone);
        self.canvas
            .update_bones(context, &segments, &joints, highlighted);
    }

    pub(crate) fn start_conversion(&mut self) {
        let files: Vec<PathBuf> = self.file_tree.selected_files();
        if files.is_empty() || self.converting {
            return;
        }

        let script_version = match self.config.script_version {
            crate::modules::ui::config_panel::ScriptVersion::V2 => {
                crate::modules::blender::bridge::ScriptVersion::V2
            }
            _ => crate::modules::blender::bridge::ScriptVersion::V1,
        };

        let mut config_obj = serde_json::json!({
            "target_scale": self.config.target_scale,
            "up_axis": match self.config.up_axis {
                crate::modules::ui::config_panel::UpAxis::YUp => "Y",
                crate::modules::ui::config_panel::UpAxis::ZUp => "Z",
            },
            "remove_unused_materials": self.config.remove_unused_materials,
            "remove_cameras": self.config.remove_cameras,
            "remove_lights": self.config.remove_lights,
            "remove_loose_vertices": self.config.remove_loose_vertices,
        });

        if matches!(
            script_version,
            crate::modules::blender::bridge::ScriptVersion::V2
        ) {
            config_obj["correct_bone_axes"] =
                serde_json::Value::Bool(self.config.correct_bone_axes);
            config_obj["preserve_leaf_bones"] =
                serde_json::Value::Bool(self.config.preserve_leaf_bones);
            config_obj["bake_animations"] = serde_json::Value::Bool(self.config.bake_animations);
        }

        let config_json = serde_json::to_string(&config_obj).unwrap_or_default();

        let (tx, rx) = mpsc::channel();
        self.conversion_rx = Some(rx);
        self.converting = true;
        self.log.clear();
        self.log.append("[Normalizer] Starting conversion...");

        let last_output = files.first().map(|p| {
            let stem = p.file_stem().unwrap_or_default();
            p.parent()
                .unwrap_or(PathBuf::from(".").as_ref())
                .join(format!("{}_normalized.glb", stem.to_string_lossy()))
        });

        self.last_output = last_output;

        std::thread::spawn(move || {
            for file in &files {
                let output = file
                    .parent()
                    .unwrap_or(PathBuf::from(".").as_ref())
                    .join(format!(
                        "{}_normalized.glb",
                        file.file_stem().unwrap_or_default().to_string_lossy()
                    ));

                let task = crate::modules::blender::task::ConversionTask {
                    input: file.clone(),
                    output,
                    config_json: config_json.clone(),
                    script_version,
                };

                let _ = tx.send(format!("[Normalizer] Processing: {}", task.input.display()));
                match blender::bridge::run_task(&task, &tx) {
                    Ok(true) => {
                        let _ = tx.send(format!("[Normalizer] Success: {}", task.output.display()));
                    }
                    Ok(false) => {
                        let _ = tx.send(format!("[Normalizer] Failed with non-zero exit code"));
                    }
                    Err(e) => {
                        let _ = tx.send(format!("[Normalizer] Error: {}", e));
                    }
                }
            }
            let _ = tx.send("__CONVERSION_DONE__".to_owned());
        });
    }

    pub fn collect_preferences(&self) -> UserPreferences {
        let prefs = UserPreferences {
            version: 1,
            view: ViewPreferences {
                show_grid: self.canvas.show_grid,
                show_axes: self.canvas.show_axes,
                show_origin: self.canvas.show_origin,
                show_bones: self.canvas.show_bones,
            },
            file_tree: FileTreePreferences {
                show_all_files: self.file_tree.show_all_files,
                last_opened_directory: self
                    .file_tree
                    .root()
                    .map(|p| p.to_string_lossy().to_string()),
            },
            log_viewer: LogViewerPreferences {
                auto_scroll: self.log.auto_scroll,
            },
            conversion: ConversionPreferences {
                target_scale: self.config.target_scale,
                up_axis: match self.config.up_axis {
                    UpAxis::YUp => "Y".to_owned(),
                    UpAxis::ZUp => "Z".to_owned(),
                },
                script_version: match self.config.script_version {
                    ScriptVersion::V1 => "V1".to_owned(),
                    ScriptVersion::V2 => "V2".to_owned(),
                },
                remove_unused_materials: self.config.remove_unused_materials,
                remove_cameras: self.config.remove_cameras,
                remove_lights: self.config.remove_lights,
                remove_loose_vertices: self.config.remove_loose_vertices,
                correct_bone_axes: self.config.correct_bone_axes,
                preserve_leaf_bones: self.config.preserve_leaf_bones,
                bake_animations: self.config.bake_animations,
            },
        };

        prefs
    }

    pub(crate) fn dispatch_action(&mut self, action: &MenuAction) {
        match action {
            MenuAction::ImportFiles => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("3D 模型", &["fbx", "blend", "obj", "glb"])
                    .pick_file()
                {
                    if let Some(parent) = path.parent().map(|p| p.to_path_buf()) {
                        self.file_tree.open_folder(parent);
                        self.file_tree.select_file(&path);
                        self.needs_save = true;
                    }
                }
            }
            MenuAction::ImportFolder => {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.file_tree.open_folder(folder);
                    self.needs_save = true;
                }
            }
            MenuAction::ClearFileList => {
                self.file_tree.clear();
                self.needs_save = true;
            }
            MenuAction::ResetConfig => {
                self.config = NormalizationConfig::default();
                self.needs_save = true;
            }
            MenuAction::ResetCamera => {
                self.camera.reset();
            }
            MenuAction::ToggleGrid => {
                self.canvas.show_grid = !self.canvas.show_grid;
                self.needs_save = true;
            }
            MenuAction::ToggleAxes => {
                self.canvas.show_axes = !self.canvas.show_axes;
                self.needs_save = true;
            }
            MenuAction::ToggleOrigin => {
                self.canvas.show_origin = !self.canvas.show_origin;
                self.needs_save = true;
            }
            MenuAction::ToggleBones => {
                self.canvas.show_bones = !self.canvas.show_bones;
                self.needs_save = true;
            }
            MenuAction::About => {
                self.show_about = true;
            }
            MenuAction::Quit => {
                preferences::save(&self.collect_preferences());
                self.quit_requested = true;
            }
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

fn prefs_to_config(prefs: &ConversionPreferences) -> NormalizationConfig {
    let up_axis = match prefs.up_axis.as_str() {
        "Z" => UpAxis::ZUp,
        _ => UpAxis::YUp,
    };
    let script_version = match prefs.script_version.as_str() {
        "V2" => ScriptVersion::V2,
        _ => ScriptVersion::V1,
    };
    NormalizationConfig {
        target_scale: prefs.target_scale,
        up_axis,
        script_version,
        remove_unused_materials: prefs.remove_unused_materials,
        remove_cameras: prefs.remove_cameras,
        remove_lights: prefs.remove_lights,
        remove_loose_vertices: prefs.remove_loose_vertices,
        correct_bone_axes: prefs.correct_bone_axes,
        preserve_leaf_bones: prefs.preserve_leaf_bones,
        bake_animations: prefs.bake_animations,
    }
}
