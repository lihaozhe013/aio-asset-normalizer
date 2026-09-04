use crate::app::{App, BottomPanelTab};
use crate::modules::ui::menu_bar::Page;

pub fn render(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;

    let has_animation =
        app.page == Page::GlbEditor && !app.glb_animation_entries().is_empty();
    if !has_animation && app.bottom_panel_tab == BottomPanelTab::Animation {
        app.bottom_panel_tab = BottomPanelTab::DebugLog;
    }

    Panel::bottom("bottom_dock")
        .resizable(true)
        .default_size(160.0)
        .min_size(100.0)
        .show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if has_animation
                    && ui
                        .selectable_label(
                            app.bottom_panel_tab == BottomPanelTab::Animation,
                            app.i18n.tr("bottom.animation"),
                        )
                        .clicked()
                {
                    app.bottom_panel_tab = BottomPanelTab::Animation;
                }
                if ui
                    .selectable_label(
                        app.bottom_panel_tab == BottomPanelTab::DebugLog,
                        app.i18n.tr("bottom.debug_log"),
                    )
                    .clicked()
                {
                    app.bottom_panel_tab = BottomPanelTab::DebugLog;
                }
            });
            ui.separator();

            match app.bottom_panel_tab {
                BottomPanelTab::Animation if has_animation => {
                    render_animation_timeline(app, ui);
                }
                _ => {
                    if app.log.render(ui, &app.i18n, app.log_runtime.log_dir())
                    {
                        app.needs_save = true;
                    }
                }
            }
        });
}

fn render_animation_timeline(app: &mut App, ui: &mut three_d::egui::Ui) {
    use egui_phosphor::regular::{PAUSE, PLAY, SKIP_BACK, SKIP_FORWARD};
    use three_d::egui::*;

    let entries = app.glb_animation_entries();
    if entries.is_empty() {
        ui.label(app.i18n.tr("glb.no_animations"));
        return;
    }

    let selected_index =
        app.glb_animation_index.min(entries.len().saturating_sub(1));
    let selected_name = entries
        .get(selected_index)
        .map(|entry| entry.0.as_str())
        .unwrap_or("Animation");

    // Row 1: label + clip selector + loop + time
    ui.horizontal(|ui| {
        ui.label(RichText::new(app.i18n.tr("glb.timeline")).strong());
        let mut requested_index = selected_index;
        ComboBox::from_id_salt("glb_animation_dock_clip")
            .selected_text(selected_name)
            .show_ui(ui, |ui| {
                for (index, (name, _, playable, _)) in
                    entries.iter().enumerate()
                {
                    ui.add_enabled_ui(*playable, |ui| {
                        ui.selectable_value(&mut requested_index, index, name);
                    });
                }
            });
        if requested_index != app.glb_animation_index {
            app.select_glb_animation(requested_index);
        }

        if let Some((_, duration, playable, unsupported)) =
            entries.get(selected_index)
        {
            if !playable {
                ui.colored_label(
                    Color32::YELLOW,
                    format!(
                        "{}: {}",
                        app.i18n.tr("glb.unsupported"),
                        unsupported
                    ),
                );
                return;
            }

            ui.checkbox(&mut app.glb_animation_loop, app.i18n.tr("glb.loop"));
            ui.label(format!(
                "{:.3}s / {:.3}s",
                app.glb_animation_time, duration
            ));
        }
    });

    // Row 2: transport icons + progress bar
    if let Some((_, duration, playable, _)) = entries.get(selected_index) {
        if *playable {
            ui.horizontal(|ui| {
                let icon_size = 18.0;

                // Play / Pause
                let play_icon = if app.glb_animation_playing {
                    PAUSE
                } else {
                    PLAY
                };
                if ui
                    .button(RichText::new(play_icon).size(icon_size))
                    .clicked()
                {
                    app.glb_animation_playing = !app.glb_animation_playing;
                }

                // Step back
                if ui
                    .button(RichText::new(SKIP_BACK).size(icon_size))
                    .clicked()
                {
                    app.step_glb_animation(-1.0);
                }

                // Step forward
                if ui
                    .button(RichText::new(SKIP_FORWARD).size(icon_size))
                    .clicked()
                {
                    app.step_glb_animation(1.0);
                }

                // Progress bar (fills remaining width)
                let slider_width = ui.available_width();
                let mut time = app.glb_animation_time;
                if ui
                    .add_sized(
                        [slider_width, 24.0],
                        Slider::new(&mut time, 0.0..=*duration),
                    )
                    .changed()
                {
                    app.set_glb_animation_time(time);
                }
            });
        }
    }
}
