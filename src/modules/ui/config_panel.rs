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

#[derive(Clone, Copy, PartialEq)]
pub enum ScriptVersion {
    V1,
    V2,
}

impl ScriptVersion {
    fn label(&self) -> &str {
        match self {
            ScriptVersion::V1 => "V1 (Static Mesh)",
            ScriptVersion::V2 => "V2 (Skinned Mesh + Animation)",
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
    pub script_version: ScriptVersion,
    pub correct_bone_axes: bool,
    pub preserve_leaf_bones: bool,
    pub bake_animations: bool,
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
            script_version: ScriptVersion::V1,
            correct_bone_axes: true,
            preserve_leaf_bones: true,
            bake_animations: true,
        }
    }
}

impl NormalizationConfig {
    pub fn render_inspector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("目标单位比例:");
            ui.add(
                egui::DragValue::new(&mut self.target_scale)
                    .speed(0.1)
                    .range(0.01..=100.0),
            );
        });

        ui.add_space(4.0);

        egui::ComboBox::from_label("目标朝向")
            .selected_text(self.up_axis.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.up_axis, UpAxis::YUp, UpAxis::YUp.label());
                ui.selectable_value(&mut self.up_axis, UpAxis::ZUp, UpAxis::ZUp.label());
            });

        ui.add_space(4.0);

        egui::ComboBox::from_label("脚本版本")
            .selected_text(self.script_version.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.script_version,
                    ScriptVersion::V1,
                    ScriptVersion::V1.label(),
                );
                ui.selectable_value(
                    &mut self.script_version,
                    ScriptVersion::V2,
                    ScriptVersion::V2.label(),
                );
            });

        ui.add_space(4.0);

        ui.label(egui::RichText::new("清理策略").strong());
        ui.checkbox(&mut self.remove_unused_materials, "移除未使用材质");
        ui.checkbox(&mut self.remove_cameras, "移除摄像机");
        ui.checkbox(&mut self.remove_lights, "移除灯光");
        ui.checkbox(&mut self.remove_loose_vertices, "移除孤立顶点");

        if self.script_version == ScriptVersion::V2 {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("骨骼与动画 (V2)").strong());
            ui.checkbox(&mut self.correct_bone_axes, "骨骼轴向校正");
            ui.checkbox(&mut self.preserve_leaf_bones, "保留末端骨骼");
            ui.checkbox(&mut self.bake_animations, "烘焙动画关键帧");
        }
    }
}
