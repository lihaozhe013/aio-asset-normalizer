use three_d::egui;

#[derive(Clone, Copy, PartialEq)]
pub enum UpAxis {
    YUp,
    ZUp,
}

impl UpAxis {
    fn label(&self) -> &str {
        match self {
            UpAxis::YUp => "Y-Up / Z-Forward",
            UpAxis::ZUp => "Z-Up / Y-Forward",
        }
    }
}

pub struct NormalizationConfig {
    pub target_scale: f32,
    pub up_axis: UpAxis,
    pub remove_unused_materials: bool,
    pub remove_cameras: bool,
    pub remove_lights: bool,
    pub remove_loose_vertices: bool,
}

impl Default for NormalizationConfig {
    fn default() -> Self {
        Self {
            target_scale: 1.0,
            up_axis: UpAxis::YUp,
            remove_unused_materials: true,
            remove_cameras: true,
            remove_lights: true,
            remove_loose_vertices: false,
        }
    }
}

impl NormalizationConfig {
    pub fn render(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("目标单位比例:");
            ui.add(
                egui::DragValue::new(&mut self.target_scale)
                    .speed(0.1)
                    .range(0.01..=100.0),
            );
        });

        egui::ComboBox::from_label("目标朝向")
            .selected_text(self.up_axis.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.up_axis, UpAxis::YUp, UpAxis::YUp.label());
                ui.selectable_value(&mut self.up_axis, UpAxis::ZUp, UpAxis::ZUp.label());
            });

        ui.label("清理策略:");
        ui.checkbox(
            &mut self.remove_unused_materials,
            "清除无用材质",
        );
        ui.checkbox(&mut self.remove_cameras, "清除相机");
        ui.checkbox(&mut self.remove_lights, "清除灯光");
        ui.checkbox(&mut self.remove_loose_vertices, "清除游离顶点");
    }
}
