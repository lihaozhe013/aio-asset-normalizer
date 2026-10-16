use crate::app::App;
use crate::modules::viewport::skeleton_visual::SkeletonDisplayMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkeletonPanelContext {
    Bvh { include_source_fit: bool },
    Glb,
}

/// Render the shared skeleton display controls used by BVH Studio and GLB
/// previews. Fit controls depend on the active preview context.
pub fn render(app: &mut App, ui: &mut three_d::egui::Ui, context: SkeletonPanelContext) {
    use three_d::egui::*;

    let mut config = app.canvas.skeleton_config;
    let i18n = app.i18n.clone();
    ui.collapsing(i18n.tr("skeleton.title"), |ui| {
        ComboBox::from_label(i18n.tr("skeleton.display_mode"))
            .selected_text(match config.mode {
                SkeletonDisplayMode::Octahedral => {
                    i18n.tr("skeleton.mode_octahedral")
                }
                SkeletonDisplayMode::Stick => i18n.tr("skeleton.mode_stick"),
                SkeletonDisplayMode::Lines => i18n.tr("skeleton.mode_lines"),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut config.mode,
                    SkeletonDisplayMode::Octahedral,
                    i18n.tr("skeleton.mode_octahedral"),
                );
                ui.selectable_value(
                    &mut config.mode,
                    SkeletonDisplayMode::Stick,
                    i18n.tr("skeleton.mode_stick"),
                );
                ui.selectable_value(
                    &mut config.mode,
                    SkeletonDisplayMode::Lines,
                    i18n.tr("skeleton.mode_lines"),
                );
            });
        ui.checkbox(&mut config.show_joints, i18n.tr("skeleton.show_joints"));
        ui.checkbox(
            &mut config.show_end_sites,
            i18n.tr("skeleton.show_end_sites"),
        );
        ui.checkbox(&mut config.in_front, i18n.tr("skeleton.in_front"));
        ui.add(
            Slider::new(&mut config.width_scale, 0.25..=3.0)
                .text(i18n.tr("skeleton.bone_width")),
        );
        ui.horizontal(|ui| {
            let include_source_fit = matches!(
                context,
                SkeletonPanelContext::Bvh {
                    include_source_fit: true
                }
            );
            if include_source_fit
                && ui.button(i18n.tr("skeleton.fit_source")).clicked()
            {
                if let Some((minimum, maximum)) = app.canvas.skeleton_bounds() {
                    app.camera.focus_on_bounds(minimum, maximum);
                }
            }
            match context {
                SkeletonPanelContext::Bvh { .. } => {
                    if ui.button(i18n.tr("skeleton.fit_target")).clicked() {
                        if let Some((minimum, maximum)) =
                            app.canvas.target_skeleton_bounds()
                        {
                            app.camera.focus_on_bounds(minimum, maximum);
                        }
                    }
                }
                SkeletonPanelContext::Glb => {
                    if ui.button(i18n.tr("skeleton.fit_skeleton")).clicked() {
                        if let Some((minimum, maximum)) =
                            app.canvas.glb_skeleton_bounds()
                        {
                            app.camera.focus_on_bounds(minimum, maximum);
                        }
                    }
                }
            }
            if ui.button(i18n.tr("skeleton.fit_all")).clicked() {
                let mut bounds = if include_source_fit {
                    app.canvas.skeleton_bounds()
                } else {
                    None
                };
                match context {
                    SkeletonPanelContext::Bvh { .. } => {
                        if let Some((minimum, maximum)) = app.canvas.target_skeleton_bounds() {
                            bounds = Some(merge_bounds(bounds, (minimum, maximum)));
                        }
                    }
                    SkeletonPanelContext::Glb => {
                        if let Some((minimum, maximum)) = app.canvas.glb_skeleton_bounds() {
                            bounds = Some(merge_bounds(bounds, (minimum, maximum)));
                        }
                    }
                }
                if let Some((minimum, maximum)) = app.canvas.model_bounds() {
                    bounds = Some(merge_bounds(bounds, (minimum, maximum)));
                }
                if let Some((minimum, maximum)) = bounds {
                    app.camera.focus_on_bounds(minimum, maximum);
                }
            }
        });
    });
    if config != app.canvas.skeleton_config {
        app.canvas.set_skeleton_config(config);
    }
}

fn merge_bounds(
    first: Option<([f32; 3], [f32; 3])>,
    second: ([f32; 3], [f32; 3]),
) -> ([f32; 3], [f32; 3]) {
    let Some((first_minimum, first_maximum)) = first else {
        return second;
    };
    (
        [
            first_minimum[0].min(second.0[0]),
            first_minimum[1].min(second.0[1]),
            first_minimum[2].min(second.0[2]),
        ],
        [
            first_maximum[0].max(second.1[0]),
            first_maximum[1].max(second.1[1]),
            first_maximum[2].max(second.1[2]),
        ],
    )
}
