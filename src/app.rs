use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use crate::modules::{
    animation::AnimationPlayer,
    blender::bridge,
    skeleton::Skeleton,
    ui::{
        bone_tree,
        config_panel::NormalizationConfig,
        file_list::FileList,
        fonts,
        log_viewer::LogViewer,
        menu_bar::{self, MenuAction},
    },
    viewport::{camera::OrbitCamera, canvas::ViewportCanvas},
};
use three_d::*;

pub struct App {
    pub camera: OrbitCamera,
    pub canvas: ViewportCanvas,
    fonts_configured: bool,
    pub file_list: FileList,
    pub config: NormalizationConfig,
    pub log: LogViewer,
    conversion_rx: Option<mpsc::Receiver<String>>,
    converting: bool,
    last_output: Option<PathBuf>,
    needs_reload: bool,
    quit_requested: bool,
    show_about: bool,
    pub skeleton: Option<Skeleton>,
    pub animation_player: Option<AnimationPlayer>,
    last_frame_time: Instant,
}

impl App {
    pub fn new(context: &Context, viewport: Viewport) -> Self {
        let mut canvas = ViewportCanvas::new(context);
        canvas.model = None;

        Self {
            camera: OrbitCamera::new(viewport),
            canvas,
            fonts_configured: false,
            file_list: FileList::new(),
            config: NormalizationConfig::default(),
            log: LogViewer::new(),
            conversion_rx: None,
            converting: false,
            last_output: None,
            needs_reload: false,
            quit_requested: false,
            show_about: false,
            skeleton: None,
            animation_player: None,
            last_frame_time: Instant::now(),
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
                            self.skeleton =
                                Skeleton::from_glb(path).ok().or_else(|| {
                                    self.log.append("[Info] No skeleton found (static mesh)");
                                    None
                                });
                            self.animation_player = self
                                .skeleton
                                .as_ref()
                                .and_then(|skel| {
                                    AnimationPlayer::from_glb(path, skel)
                                        .ok()
                                        .or_else(|| {
                                            self.log
                                                .append("[Info] No animations found");
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
                self.skeleton.as_ref().map(|s| {
                    (s.bone_positions(), s.joint_positions())
                })
            })
            .unwrap_or_default();

        let highlighted = self.skeleton.as_ref().and_then(|s| s.highlighted_bone);
        self.canvas.update_bones(context, &segments, &joints, highlighted);
    }

    fn start_conversion(&mut self) {
        let files: Vec<PathBuf> = self.file_list.files().to_vec();
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

        if matches!(script_version, crate::modules::blender::bridge::ScriptVersion::V2) {
            config_obj["correct_bone_axes"] =
                serde_json::Value::Bool(self.config.correct_bone_axes);
            config_obj["preserve_leaf_bones"] =
                serde_json::Value::Bool(self.config.preserve_leaf_bones);
            config_obj["bake_animations"] =
                serde_json::Value::Bool(self.config.bake_animations);
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
                match bridge::run_task(&task, &tx) {
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

    fn dispatch_action(&mut self, action: &MenuAction) {
        match action {
            MenuAction::ImportFiles => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("3D 模型", &["fbx", "blend", "obj", "glb"])
                    .pick_file()
                {
                    self.file_list.add_path(path);
                }
            }
            MenuAction::ImportFolder => {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.file_list.scan_folder(&folder);
                }
            }
            MenuAction::ClearFileList => {
                self.file_list.clear();
            }
            MenuAction::ResetConfig => {
                self.config = NormalizationConfig::default();
            }
            MenuAction::ResetCamera => {
                self.camera.reset();
            }
            MenuAction::ToggleGrid => {
                self.canvas.show_grid = !self.canvas.show_grid;
            }
            MenuAction::ToggleAxes => {
                self.canvas.show_axes = !self.canvas.show_axes;
            }
            MenuAction::ToggleOrigin => {
                self.canvas.show_origin = !self.canvas.show_origin;
            }
            MenuAction::ToggleBones => {
                self.canvas.show_bones = !self.canvas.show_bones;
            }
            MenuAction::About => {
                self.show_about = true;
            }
            MenuAction::Quit => {
                self.quit_requested = true;
            }
        }
    }

    fn collect_shortcut_actions(ctx: &three_d::egui::Context) -> Vec<MenuAction> {
        let mut actions = Vec::new();
        ctx.input(|i| {
            let ctrl = i.modifiers.ctrl || i.modifiers.command;
            if ctrl && i.key_pressed(three_d::egui::Key::Q) {
                actions.push(MenuAction::Quit);
            }
            if ctrl && !i.modifiers.shift && i.key_pressed(three_d::egui::Key::O) {
                actions.push(MenuAction::ImportFiles);
            }
            if ctrl && i.modifiers.shift && i.key_pressed(three_d::egui::Key::O) {
                actions.push(MenuAction::ImportFolder);
            }
            if ctrl && i.key_pressed(three_d::egui::Key::R) {
                actions.push(MenuAction::ResetCamera);
            }
            if ctrl && i.key_pressed(three_d::egui::Key::G) {
                actions.push(MenuAction::ToggleGrid);
            }
            if ctrl && i.key_pressed(three_d::egui::Key::A) {
                actions.push(MenuAction::ToggleAxes);
            }
            if ctrl && i.key_pressed(three_d::egui::Key::B) {
                actions.push(MenuAction::ToggleBones);
            }
        });
        actions
    }

    pub fn render_ui(&mut self, ui: &mut three_d::egui::Ui, window_width: u32) -> f32 {
        if !self.fonts_configured {
            fonts::configure(ui.ctx());
            self.fonts_configured = true;
        }

        self.file_list.handle_dropped_files(ui.ctx());
        self.poll_tasks();

        let shortcut_actions = Self::collect_shortcut_actions(ui.ctx());
        let menu_actions = menu_bar::render(
            ui,
            self.canvas.show_grid,
            self.canvas.show_axes,
            self.canvas.show_origin,
            self.canvas.show_bones,
        );

        for action in shortcut_actions.iter().chain(menu_actions.iter()) {
            self.dispatch_action(action);
        }

        self.render_about_dialog(ui.ctx());

        use three_d::egui::*;
        Panel::left("control_panel")
            .resizable(true)
            .default_size(300.0)
            .min_size(200.0)
            .show_inside(ui, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("控制面板");
                    ui.separator();

                    ui.collapsing("资产导入", |ui| {
                        self.file_list.render(ui);
                        ui.add_space(4.0);
                        let btn_text = if self.converting {
                            "正在转换..."
                        } else {
                            "开始转换"
                        };
                        let enabled = !self.file_list.files().is_empty() && !self.converting;
                        ui.add_enabled_ui(enabled, |ui| {
                            if ui.button(btn_text).clicked() {
                                self.start_conversion();
                            }
                        });
                    });

                    ui.collapsing("转换配置", |ui| {
                        self.config.render(ui);
                    });

                    if self.skeleton.is_some() {
                        ui.collapsing("骨骼层级", |ui| {
                            ui.checkbox(&mut self.canvas.show_bones, "显示骨骼");
                            if let Some(ref mut skel) = self.skeleton {
                                bone_tree::render_bone_tree(ui, skel);
                            }
                        });
                    }

                    if self.animation_player.is_some() {
                        ui.collapsing("动画播放", |ui| {
                            self.render_animation_controls(ui);
                        });
                    }

                    ui.collapsing("日志输出", |ui| {
                        self.log.render(ui);
                    });
                });
            });
        window_width as f32 - ui.available_width()
    }

    fn render_animation_controls(&mut self, ui: &mut three_d::egui::Ui) {
        let anim = match self.animation_player.as_mut() {
            Some(a) => a,
            None => {
                ui.label("No animation data");
                return;
            }
        };

        ui.horizontal(|ui| {
            let play_label = if anim.playing { "暂停" } else { "播放" };
            if ui.button(play_label).clicked() {
                anim.toggle_play();
            }
            if ui.button("停止").clicked() {
                anim.stop();
            }
            ui.checkbox(&mut anim.looping, "循环");
        });

        ui.horizontal(|ui| {
            ui.label("速度:");
            if ui.button("0.5x").clicked() {
                anim.speed = 0.5;
            }
            if ui.button("1.0x").clicked() {
                anim.speed = 1.0;
            }
            if ui.button("2.0x").clicked() {
                anim.speed = 2.0;
            }
            ui.add(
                three_d::egui::Slider::new(&mut anim.speed, 0.1..=3.0).text("x"),
            );
        });

        if anim.clips.len() > 1 {
            ui.horizontal(|ui| {
                ui.label("动画片段:");
                let names = anim.clip_names();
                for (i, name) in names.iter().enumerate() {
                    let selected = anim.current_clip == i;
                    if ui.selectable_label(selected, name).clicked() {
                        anim.set_clip(i);
                    }
                }
            });
        }

        let clip = match anim.current_clip() {
            Some(c) => c,
            None => return,
        };

        let mut slider_val = anim.current_time;
        ui.horizontal(|ui| {
            ui.label(format!(
                "{:.2}s / {:.2}s",
                anim.current_time, clip.duration
            ));
        });
        if ui
            .add(
                three_d::egui::Slider::new(&mut slider_val, 0.0..=clip.duration).text("时间"),
            )
            .changed()
        {
            anim.current_time = slider_val;
            anim.update_bone_transforms();
        }
    }

    fn render_about_dialog(&mut self, ctx: &three_d::egui::Context) {
        use three_d::egui::*;
        Window::new("About AIO Asset Normalizer")
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("AIO Asset Normalizer");
                    ui.label("v0.1.0");
                    ui.separator();
                    ui.label("Cross-platform 3D asset batch normalization tool");
                    ui.add_space(8.0);
                    ui.hyperlink_to(
                        "GitHub Repository",
                        "https://github.com/anomalyco/aio-asset-normalizer",
                    );
                });
            });
    }

    pub fn compute_viewport(
        &self,
        panel_width: f32,
        device_pixel_ratio: f32,
        full_viewport: &Viewport,
    ) -> Viewport {
        let panel_px = (panel_width * device_pixel_ratio) as i32;
        let mut width = full_viewport.width.saturating_sub(panel_px as u32);
        let mut height = full_viewport.height;
        if width < 1 {
            width = 1;
        }
        if height < 1 {
            height = 1;
        }
        Viewport {
            x: panel_px,
            y: 0,
            width,
            height,
        }
    }
}
