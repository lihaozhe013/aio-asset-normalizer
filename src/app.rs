use crate::modules::viewport::{camera::OrbitCamera, canvas::ViewportCanvas};
use three_d::*;

pub struct App {
    pub camera: OrbitCamera,
    pub canvas: ViewportCanvas,
}

impl App {
    pub fn new(context: &Context, viewport: Viewport) -> Self {
        Self {
            camera: OrbitCamera::new(viewport),
            canvas: ViewportCanvas::new(context),
        }
    }

    pub fn render_ui(&mut self, ui: &mut three_d::egui::Ui, window_width: u32) -> f32 {
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
                        ui.label("拖拽模型文件到此处...");
                    });

                    ui.collapsing("转换配置", |ui| {
                        ui.label("目标单位比例: 1.0");
                        ui.label("目标朝向: Y-Up / Z-Forward");
                        ui.label("清理策略: 清除无用材质");
                    });

                    ui.collapsing("日志输出", |ui| {
                        ui.label("就绪，等待任务...");
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
