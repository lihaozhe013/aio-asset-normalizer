use crate::modules::{
    ui::{config_panel::NormalizationConfig, file_list::FileList, fonts, log_viewer::LogViewer},
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
}

impl App {
    pub fn new(context: &Context, viewport: Viewport) -> Self {
        Self {
            camera: OrbitCamera::new(viewport),
            canvas: ViewportCanvas::new(context),
            fonts_configured: false,
            file_list: FileList::new(),
            config: NormalizationConfig::default(),
            log: LogViewer::new(),
        }
    }

    pub fn render_ui(&mut self, ui: &mut three_d::egui::Ui, window_width: u32) -> f32 {
        if !self.fonts_configured {
            fonts::configure(ui.ctx());
            self.fonts_configured = true;
        }

        self.file_list.handle_dropped_files(ui.ctx());

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
