use crate::modules::i18n::I18n;
use crate::modules::preferences::ConversionPreferences;
use three_d::egui;

#[derive(Clone, Copy, PartialEq)]
pub enum UpAxis {
    YUp,
    ZUp,
}

impl UpAxis {
    fn label<'a>(&self, i18n: &'a I18n) -> &'a str {
        match self {
            UpAxis::YUp => i18n.tr("option.y_up"),
            UpAxis::ZUp => i18n.tr("option.z_up"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ScriptVersion {
    V1,
    V2,
}

impl ScriptVersion {
    fn label<'a>(&self, i18n: &'a I18n) -> &'a str {
        match self {
            ScriptVersion::V1 => i18n.tr("option.v1"),
            ScriptVersion::V2 => i18n.tr("option.v2"),
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
    pub fn render_inspector(&mut self, ui: &mut egui::Ui, i18n: &I18n) -> bool {
        let mut changed = false;

        ui.horizontal(|ui| {
            ui.label(i18n.tr("label.target_scale"));
            if ui
                .add(
                    egui::DragValue::new(&mut self.target_scale)
                        .speed(0.1)
                        .range(0.01..=100.0),
                )
                .changed()
            {
                changed = true;
            }
        });

        ui.add_space(4.0);

        if egui::ComboBox::from_label(i18n.tr("label.target_orientation"))
            .selected_text(self.up_axis.label(i18n))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.up_axis,
                    UpAxis::YUp,
                    UpAxis::YUp.label(i18n),
                );
                ui.selectable_value(
                    &mut self.up_axis,
                    UpAxis::ZUp,
                    UpAxis::ZUp.label(i18n),
                );
            })
            .response
            .changed()
        {
            changed = true;
        }

        ui.add_space(4.0);

        if egui::ComboBox::from_label(i18n.tr("label.script_version"))
            .selected_text(self.script_version.label(i18n))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.script_version,
                    ScriptVersion::V1,
                    ScriptVersion::V1.label(i18n),
                );
                ui.selectable_value(
                    &mut self.script_version,
                    ScriptVersion::V2,
                    ScriptVersion::V2.label(i18n),
                );
            })
            .response
            .changed()
        {
            changed = true;
        }

        ui.add_space(4.0);

        ui.label(
            egui::RichText::new(i18n.tr("panel.cleanup_strategy")).strong(),
        );
        if ui
            .checkbox(
                &mut self.remove_unused_materials,
                i18n.tr("label.remove_unused_materials"),
            )
            .changed()
        {
            changed = true;
        }
        if ui
            .checkbox(&mut self.remove_cameras, i18n.tr("label.remove_cameras"))
            .changed()
        {
            changed = true;
        }
        if ui
            .checkbox(&mut self.remove_lights, i18n.tr("label.remove_lights"))
            .changed()
        {
            changed = true;
        }
        if ui
            .checkbox(
                &mut self.remove_loose_vertices,
                i18n.tr("label.remove_loose_vertices"),
            )
            .changed()
        {
            changed = true;
        }

        if self.script_version == ScriptVersion::V2 {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(i18n.tr("panel.bones_animation_v2"))
                    .strong(),
            );
            if ui
                .checkbox(
                    &mut self.correct_bone_axes,
                    i18n.tr("label.correct_bone_axes"),
                )
                .changed()
            {
                changed = true;
            }
            if ui
                .checkbox(
                    &mut self.preserve_leaf_bones,
                    i18n.tr("label.preserve_leaf_bones"),
                )
                .changed()
            {
                changed = true;
            }
            if ui
                .checkbox(
                    &mut self.bake_animations,
                    i18n.tr("label.bake_animations"),
                )
                .changed()
            {
                changed = true;
            }
        }

        changed
    }
}

impl From<&ConversionPreferences> for NormalizationConfig {
    fn from(prefs: &ConversionPreferences) -> Self {
        let up_axis = match prefs.up_axis.as_str() {
            "Z" => UpAxis::ZUp,
            _ => UpAxis::YUp,
        };
        let script_version = match prefs.script_version.as_str() {
            "V2" => ScriptVersion::V2,
            _ => ScriptVersion::V1,
        };
        Self {
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
}

impl From<&NormalizationConfig> for ConversionPreferences {
    fn from(config: &NormalizationConfig) -> Self {
        Self {
            target_scale: config.target_scale,
            up_axis: match config.up_axis {
                UpAxis::YUp => "Y".to_owned(),
                UpAxis::ZUp => "Z".to_owned(),
            },
            script_version: match config.script_version {
                ScriptVersion::V1 => "V1".to_owned(),
                ScriptVersion::V2 => "V2".to_owned(),
            },
            remove_unused_materials: config.remove_unused_materials,
            remove_cameras: config.remove_cameras,
            remove_lights: config.remove_lights,
            remove_loose_vertices: config.remove_loose_vertices,
            correct_bone_axes: config.correct_bone_axes,
            preserve_leaf_bones: config.preserve_leaf_bones,
            bake_animations: config.bake_animations,
        }
    }
}
