use crate::app::{App, BottomPanelTab};
use crate::modules::ui::menu_bar::Page;
use crate::modules::ui::{tabs, theme};
use egui_phosphor::regular::{FILM_STRIP, PAUSE, PLAY, SKIP_BACK, SKIP_FORWARD, TERMINAL};
use three_d::egui::*;

pub fn render(app: &mut App, ui: &mut three_d::egui::Ui) {
    let has_animation = page_has_animation(app);
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
                let mut available: Vec<BottomPanelTab> = Vec::new();
                if has_animation {
                    available.push(BottomPanelTab::Animation);
                }
                available.push(BottomPanelTab::DebugLog);
                let entries: Vec<tabs::TabEntry> = available
                    .iter()
                    .map(|tab| match tab {
                        BottomPanelTab::Animation => tabs::TabEntry {
                            icon: FILM_STRIP,
                            label: app.i18n.tr("bottom.animation").to_owned(),
                        },
                        BottomPanelTab::DebugLog => tabs::TabEntry {
                            icon: TERMINAL,
                            label: app.i18n.tr("bottom.debug_log").to_owned(),
                        },
                    })
                    .collect();
                let selected = available
                    .iter()
                    .position(|tab| *tab == app.bottom_panel_tab)
                    .unwrap_or(available.len().saturating_sub(1));
                if let Some(index) = tabs::segmented(ui, &entries, selected) {
                    app.bottom_panel_tab = available[index];
                }
            });
            ui.separator();

            match app.bottom_panel_tab {
                BottomPanelTab::Animation if has_animation => {
                    if app.page == Page::BvhStudio {
                        render_bvh_timeline(app, ui);
                    } else {
                        render_animation_timeline(app, ui);
                    }
                }
                _ => {
                    if app.log.render(ui, &app.i18n, app.log_runtime.log_dir()) {
                        app.needs_save = true;
                    }
                }
            }
        });
}

/// Whether the active page offers an animation timeline for the bottom dock.
fn page_has_animation(app: &App) -> bool {
    match app.page {
        Page::GlbEditor => !app.glb_animation_entries().is_empty(),
        Page::BvhStudio => app
            .bvh
            .as_ref()
            .is_some_and(|document| !document.frames.is_empty()),
        Page::FbxConverter | Page::ConnectAi => false,
    }
}

/// BVH Studio timeline with the same two-row layout as the GLB Editor dock:
/// a readout row and a transport row whose scrubber fills the remaining width.
fn render_bvh_timeline(app: &mut App, ui: &mut three_d::egui::Ui) {
    let Some(document) = app.bvh.as_ref() else {
        return;
    };
    let frame_count = document.frames.len();
    if frame_count == 0 {
        ui.label(app.i18n.tr("bvh.open_hint"));
        return;
    }
    let last_frame = frame_count.saturating_sub(1);
    let current_frame = app.bvh_frame.min(last_frame);
    let duration = document.duration();
    let current_time = current_frame as f32 * document.frame_time;

    // Row 1: title + frame/time readouts + playback speed.
    ui.horizontal(|ui| {
        ui.label(RichText::new(app.i18n.tr("bvh.timeline")).strong());
        ui.monospace(format!(
            "{}: {} / {}",
            app.i18n.tr("bvh.frame"),
            current_frame,
            last_frame
        ));
        ui.monospace(format!("{:.3}s / {:.3}s", current_time, duration));
        ui.label(app.i18n.tr("bvh.speed"));
        ui.add(
            DragValue::new(&mut app.bvh_playback_speed)
                .speed(0.05)
                .range(0.05..=8.0)
                .suffix("x"),
        );
    });

    // Row 2: transport icons + frame scrubber (fills remaining width).
    ui.horizontal(|ui| {
        let icon_size = 16.0;
        let button_size = Vec2::splat(28.0);

        // Play / Pause
        let (play_icon, play_hint) = if app.bvh_playing {
            (PAUSE, "bvh.pause")
        } else {
            (PLAY, "bvh.play")
        };
        if ui
            .add_sized(
                button_size,
                Button::new(RichText::new(play_icon).size(icon_size)),
            )
            .on_hover_text(app.i18n.tr(play_hint))
            .clicked()
        {
            app.bvh_playing = !app.bvh_playing;
            app.bvh_playback_accumulator = 0.0;
        }

        // Step back one frame
        if ui
            .add_sized(
                button_size,
                Button::new(RichText::new(SKIP_BACK).size(icon_size)),
            )
            .on_hover_text(app.i18n.tr("glb.step_back"))
            .clicked()
        {
            app.step_bvh_frame(-1);
        }

        // Step forward one frame
        if ui
            .add_sized(
                button_size,
                Button::new(RichText::new(SKIP_FORWARD).size(icon_size)),
            )
            .on_hover_text(app.i18n.tr("glb.step_forward"))
            .clicked()
        {
            app.step_bvh_frame(1);
        }

        // Frame scrubber: snaps to whole frames while dragging.
        let slider_width = ui.available_width();
        let mut frame = current_frame as f32;
        if ui
            .add_sized(
                [slider_width, 24.0],
                Slider::new(&mut frame, 0.0..=last_frame as f32)
                    .step_by(1.0)
                    .custom_formatter(|value, _| format!("{}", value.round() as i64)),
            )
            .on_hover_text(app.i18n.tr("bvh.frame"))
            .changed()
        {
            let target = frame.round().clamp(0.0, last_frame as f32) as usize;
            app.set_bvh_frame(target);
            app.bvh_playback_accumulator = 0.0;
        }
    });
}

fn render_animation_timeline(app: &mut App, ui: &mut three_d::egui::Ui) {
    let entries = app.glb_animation_entries();
    if entries.is_empty() {
        ui.label(app.i18n.tr("glb.no_animations"));
        return;
    }

    let selected_index = app.glb_animation_index.min(entries.len().saturating_sub(1));
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
                for (index, (name, _, playable, _)) in entries.iter().enumerate() {
                    ui.add_enabled_ui(*playable, |ui| {
                        ui.selectable_value(&mut requested_index, index, name);
                    });
                }
            });
        if requested_index != app.glb_animation_index {
            app.select_glb_animation(requested_index);
        }

        if let Some((_, duration, playable, unsupported)) = entries.get(selected_index) {
            if !playable {
                ui.colored_label(
                    theme::WARNING,
                    format!("{}: {}", app.i18n.tr("glb.unsupported"), unsupported),
                );
                return;
            }

            ui.checkbox(&mut app.glb_animation_loop, app.i18n.tr("glb.loop"));
            ui.monospace(format!("{:.3}s / {:.3}s", app.glb_animation_time, duration));
        }
    });

    // Row 2: transport icons + progress bar
    if let Some((_, duration, playable, _)) = entries.get(selected_index) {
        if *playable {
            ui.horizontal(|ui| {
                let icon_size = 16.0;
                let button_size = Vec2::splat(28.0);

                // Play / Pause
                let (play_icon, play_hint) = if app.glb_animation_playing {
                    (PAUSE, "glb.pause")
                } else {
                    (PLAY, "glb.play")
                };
                if ui
                    .add_sized(
                        button_size,
                        Button::new(RichText::new(play_icon).size(icon_size)),
                    )
                    .on_hover_text(app.i18n.tr(play_hint))
                    .clicked()
                {
                    app.glb_animation_playing = !app.glb_animation_playing;
                }

                // Step back
                if ui
                    .add_sized(
                        button_size,
                        Button::new(RichText::new(SKIP_BACK).size(icon_size)),
                    )
                    .on_hover_text(app.i18n.tr("glb.step_back"))
                    .clicked()
                {
                    app.step_glb_animation(-1.0);
                }

                // Step forward
                if ui
                    .add_sized(
                        button_size,
                        Button::new(RichText::new(SKIP_FORWARD).size(icon_size)),
                    )
                    .on_hover_text(app.i18n.tr("glb.step_forward"))
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
