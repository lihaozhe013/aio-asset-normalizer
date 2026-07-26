use crate::app::App;
use crate::modules::ui::{bone_tree, fonts, menu_bar};

pub fn render_ui(
    app: &mut App,
    ui: &mut three_d::egui::Ui,
    _window_width: u32,
) -> three_d::egui::Rect {
    use three_d::egui::*;

    if !app.fonts_configured {
        fonts::configure(ui.ctx());
        app.fonts_configured = true;
    }

    app.file_tree.handle_dropped_files(ui.ctx());
    app.poll_tasks();

    let shortcut_actions = collect_shortcut_actions(ui.ctx());
    let menu_actions = menu_bar::render(
        ui,
        app.canvas.show_grid,
        app.canvas.show_axes,
        app.canvas.show_origin,
        app.canvas.show_bones,
    );

    for action in shortcut_actions.iter().chain(menu_actions.iter()) {
        app.dispatch_action(action);
    }

    render_about_dialog(app, ui.ctx());
    render_preferences_dialog(app, ui.ctx());

    if app.pending_browse_blender {
        app.pending_browse_blender = false;
        if let Some(p) = rfd::FileDialog::new().pick_file() {
            app.blender_path = Some(p.to_string_lossy().to_string());
            app.needs_save = true;
        }
    }

    Panel::left("file_tree")
        .resizable(true)
        .default_size(250.0)
        .min_size(160.0)
        .show_inside(ui, |ui| {
            if app.file_tree.render(ui) {
                app.needs_save = true;
            }
        });

    Panel::right("inspector")
        .resizable(true)
        .default_size(280.0)
        .min_size(200.0)
        .show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.heading("格式转换");
                ui.separator();
                if app.config.render_inspector(ui) {
                    app.needs_save = true;
                }

                if app.skeleton.is_some() {
                    ui.separator();
                    CollapsingHeader::new("骨骼层级").default_open(true).show(
                        ui,
                        |ui| {
                            if ui
                                .checkbox(
                                    &mut app.canvas.show_bones,
                                    "显示骨骼",
                                )
                                .changed()
                            {
                                app.needs_save = true;
                            }
                            if let Some(ref mut skel) = app.skeleton {
                                bone_tree::render_bone_tree(ui, skel);
                            }
                        },
                    );
                }

                if app.animation_player.is_some() {
                    ui.separator();
                    CollapsingHeader::new("动画播放").default_open(true).show(
                        ui,
                        |ui| {
                            render_animation_controls(app, ui);
                        },
                    );
                }

                ui.separator();

                let btn_text = if app.converting {
                    "正在转换..."
                } else {
                    "开始转换"
                };
                let enabled = !app.file_tree.selected_files().is_empty()
                    && !app.converting;
                ui.add_enabled_ui(enabled, |ui| {
                    let btn = ui.button(RichText::new(btn_text).strong());
                    if btn.clicked() {
                        app.start_conversion();
                    }
                });
            });
        });

    let content_rect = ui.available_rect_before_wrap();

    Panel::bottom("log")
        .resizable(true)
        .default_size(150.0)
        .min_size(80.0)
        .show_inside(ui, |ui| {
            if app.log.render(ui) {
                app.needs_save = true;
            }
        });

    content_rect
}

fn render_animation_controls(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;

    let anim = match app.animation_player.as_mut() {
        Some(a) => a,
        None => {
            ui.label("No animation data");
            return;
        }
    };

    ui.horizontal(|ui| {
        let play_label = if anim.playing { "暂停" } else { "播放" };
        if ui.button(play_label).clicked() {
            anim.toggle_play();
        }
        if ui.button("停止").clicked() {
            anim.stop();
        }
        ui.checkbox(&mut anim.looping, "循环");
    });

    ui.horizontal(|ui| {
        ui.label("速度:");
        if ui.button("0.5x").clicked() {
            anim.speed = 0.5;
        }
        if ui.button("1.0x").clicked() {
            anim.speed = 1.0;
        }
        if ui.button("2.0x").clicked() {
            anim.speed = 2.0;
        }
        ui.add(Slider::new(&mut anim.speed, 0.1..=3.0).text("x"));
    });

    if anim.clips.len() > 1 {
        ui.horizontal(|ui| {
            ui.label("动画片段:");
            let names = anim.clip_names();
            for (i, name) in names.iter().enumerate() {
                let selected = anim.current_clip == i;
                if ui.selectable_label(selected, name).clicked() {
                    anim.set_clip(i);
                }
            }
        });
    }

    let clip = match anim.current_clip() {
        Some(c) => c,
        None => return,
    };

    let mut slider_val = anim.current_time;
    ui.horizontal(|ui| {
        ui.label(format!("{:.2}s / {:.2}s", anim.current_time, clip.duration));
    });
    if ui
        .add(Slider::new(&mut slider_val, 0.0..=clip.duration).text("时间"))
        .changed()
    {
        anim.current_time = slider_val;
        anim.update_bone_transforms();
    }
}

fn render_about_dialog(app: &mut App, ctx: &three_d::egui::Context) {
    use three_d::egui::*;
    Window::new("About AIO Asset Normalizer")
        .open(&mut app.show_about)
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

fn render_preferences_dialog(app: &mut App, ctx: &three_d::egui::Context) {
    use three_d::egui::*;
    Window::new("Preferences")
        .open(&mut app.show_preferences)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Blender path:");
                let mut path = app.blender_path.clone().unwrap_or_default();
                let changed = ui.text_edit_singleline(&mut path).changed();
                if changed {
                    let trimmed = path.trim();
                    app.blender_path = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_owned())
                    };
                    app.needs_save = true;
                }
                if ui.button("Browse...").clicked() {
                    app.pending_browse_blender = true;
                }
            });

            if app.blender_path.is_some() {
                if ui.button("Reset").clicked() {
                    app.blender_path = None;
                    app.needs_save = true;
                }
            }

            let detected = crate::modules::blender::bridge::find_blender(app.blender_path.as_deref())
                .map(|p| p.to_string_lossy().to_string());

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            match detected {
                Some(ref d) => {
                    ui.label(RichText::new(format!("Detected: {}", d)).weak());
                }
                None => {
                    ui.label(
                        RichText::new("No Blender found on system PATH")
                            .color(Color32::RED),
                    );
                }
            }
        });
}

fn collect_shortcut_actions(
    ctx: &three_d::egui::Context,
) -> Vec<menu_bar::MenuAction> {
    use menu_bar::MenuAction;
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
        if ctrl && i.key_pressed(three_d::egui::Key::B) {
            actions.push(MenuAction::ToggleBones);
        }
    });
    actions
}
