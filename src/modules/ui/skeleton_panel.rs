use crate::app::App;
use crate::modules::viewport::skeleton_visual::SkeletonDisplayMode;

/// Render the shared skeleton display controls used by BVH Studio and GLB
/// retarget previews. The source fit button is hidden for GLB-only previews.
pub fn render(
    app: &mut App,
    ui: &mut three_d::egui::Ui,
    include_source_fit: bool,
) {
    use three_d::egui::*;

    let mut config = app.canvas.skeleton_config;
    ui.collapsing("Skeleton Display", |ui| {
        ComboBox::from_label("Display mode")
            .selected_text(match config.mode {
                SkeletonDisplayMode::Octahedral => "Octahedral",
                SkeletonDisplayMode::Stick => "Stick",
                SkeletonDisplayMode::Lines => "Lines",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut config.mode,
                    SkeletonDisplayMode::Octahedral,
                    "Octahedral",
                );
                ui.selectable_value(
                    &mut config.mode,
                    SkeletonDisplayMode::Stick,
                    "Stick",
                );
                ui.selectable_value(
                    &mut config.mode,
                    SkeletonDisplayMode::Lines,
                    "Lines",
                );
            });
        ui.checkbox(&mut config.show_joints, "Show joints");
        ui.checkbox(&mut config.show_end_sites, "Show End Sites");
        ui.checkbox(&mut config.in_front, "In Front");
        ui.add(
            Slider::new(&mut config.width_scale, 0.25..=3.0).text("Bone width"),
        );
        ui.horizontal(|ui| {
            if include_source_fit && ui.button("Fit source").clicked() {
                if let Some((minimum, maximum)) = app.canvas.skeleton_bounds() {
                    app.camera.focus_on_bounds(minimum, maximum);
                }
            }
            if ui.button("Fit target").clicked() {
                if let Some((minimum, maximum)) =
                    app.canvas.target_skeleton_bounds()
                {
                    app.camera.focus_on_bounds(minimum, maximum);
                }
            }
            if ui.button("Fit all").clicked() {
                let mut bounds = if include_source_fit {
                    app.canvas.skeleton_bounds()
                } else {
                    None
                };
                if let Some((minimum, maximum)) =
                    app.canvas.target_skeleton_bounds()
                {
                    bounds = Some(merge_bounds(bounds, (minimum, maximum)));
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
