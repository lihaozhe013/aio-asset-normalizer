use std::path::PathBuf;
use std::sync::mpsc;

use crate::modules::{
    blender::bridge,
    ui::{
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
                        Ok(()) => {}
                        Err(e) => self.log.append(&format!("[Error] {}", e)),
                    }
                } else {
                    self.log
                        .append(&format!("[Error] Output not found: {}", path.display()));
                }
            }
        }
    }

    fn start_conversion(&mut self) {
        let files: Vec<PathBuf> = self.file_list.files().to_vec();
        if files.is_empty() || self.converting {
            return;
        }

        let config_json = serde_json::to_string(&serde_json::json!({
            "target_scale": self.config.target_scale,
            "up_axis": match self.config.up_axis {
                crate::modules::ui::config_panel::UpAxis::YUp => "Y",
                crate::modules::ui::config_panel::UpAxis::ZUp => "Z",
            },
            "remove_unused_materials": self.config.remove_unused_materials,
            "remove_cameras": self.config.remove_cameras,
            "remove_lights": self.config.remove_lights,
            "remove_loose_vertices": self.config.remove_loose_vertices,
        }))
        .unwrap_or_default();

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

                    ui.collapsing("日志输出", |ui| {
                        self.log.render(ui);
                    });
                });
            });
        window_width as f32 - ui.available_width()
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
        Viewport {
            x: panel_px,
            y: 0,
            width: full_viewport.width.saturating_sub(panel_px as u32),
            height: full_viewport.height,
        }
    }
}
