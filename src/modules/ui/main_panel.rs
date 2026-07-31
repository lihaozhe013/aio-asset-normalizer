use crate::app::App;
use crate::modules::i18n::LanguagePreference;
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
        &app.i18n,
        app.i18n.preference(),
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
        .min_size(60.0)
        .show_inside(ui, |ui| {
            let (prefs_changed, preview_path) =
                app.file_tree.render(ui, &app.i18n);
            if prefs_changed || app.file_tree.take_root_changed() {
                app.needs_save = true;
            }
            if let Some(path) = preview_path {
                app.preview_glb(&path);
            }
        });

    Panel::right("inspector")
        .resizable(true)
        .default_size(280.0)
        .min_size(60.0)
        .show_inside(ui, |ui| {
            ScrollArea::both().show(ui, |ui| {
                CollapsingHeader::new(app.i18n.tr("panel.conversion"))
                    .id_salt("conversion_section")
                    .default_open(true)
                    .show(ui, |ui| {
                        if app.config.render_inspector(ui, &app.i18n) {
                            app.needs_save = true;
                        }
                    });

                if app.skeleton.is_some() {
                    ui.separator();
                    CollapsingHeader::new(app.i18n.tr("panel.skeleton"))
                        .default_open(true)
                        .show(ui, |ui| {
                            if ui
                                .checkbox(
                                    &mut app.canvas.show_bones,
                                    app.i18n.tr("menu.show_bones"),
                                )
                                .changed()
                            {
                                app.needs_save = true;
                            }
                            if let Some(ref mut skel) = app.skeleton {
                                bone_tree::render_bone_tree(
                                    ui, skel, &app.i18n,
                                );
                            }
                        });
                }

                if app.animation_player.is_some() {
                    ui.separator();
                    CollapsingHeader::new(app.i18n.tr("panel.animation"))
                        .default_open(true)
                        .show(ui, |ui| {
                            render_animation_controls(app, ui);
                        });
                }

                ui.separator();

                let btn_text = if app.converting {
                    app.i18n.tr("button.converting").to_owned()
                } else {
                    app.i18n.tr("button.start_conversion").to_owned()
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
            if app.log.render(ui, &app.i18n) {
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
            ui.label(app.i18n.tr("label.no_animation_data"));
            return;
        }
    };

    ui.horizontal(|ui| {
        let play_label = if anim.playing {
            app.i18n.tr("button.pause")
        } else {
            app.i18n.tr("button.play")
        };
        if ui.button(play_label).clicked() {
            anim.toggle_play();
        }
        if ui.button(app.i18n.tr("button.stop")).clicked() {
            anim.stop();
        }
        ui.checkbox(&mut anim.looping, app.i18n.tr("label.looping"));
    });

    ui.horizontal(|ui| {
        ui.label(app.i18n.tr("label.speed"));
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
            ui.label(app.i18n.tr("label.animation_clip"));
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
        .add(
            Slider::new(&mut slider_val, 0.0..=clip.duration)
                .text(app.i18n.tr("label.time")),
        )
        .changed()
    {
        anim.current_time = slider_val;
        anim.update_bone_transforms();
    }
}

fn render_about_dialog(app: &mut App, ctx: &three_d::egui::Context) {
    use three_d::egui::*;
    if !app.show_about {
        return;
    }

    const ICON_PNG: &[u8] = include_bytes!(
        "../../../assets/icon/aio-asset-normalizer-transparent.png"
    );
    if app.about_icon.is_none() {
        let decoded_icon = image::load_from_memory(ICON_PNG)
            .expect("failed to decode the About dialog icon")
            .to_rgba8();
        let icon_image = ColorImage::from_rgba_unmultiplied(
            [
                decoded_icon.width() as usize,
                decoded_icon.height() as usize,
            ],
            decoded_icon.as_raw(),
        );
        app.about_icon = Some(ctx.load_texture(
            "aio-asset-normalizer-about-icon",
            icon_image,
            TextureOptions::LINEAR,
        ));
    }
    let icon_texture = app.about_icon.as_ref().unwrap().clone();

    Window::new(app.i18n.tr("about.title"))
        .open(&mut app.show_about)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .fixed_size([360.0, 240.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add(
                    Image::from_texture(&icon_texture)
                        .fit_to_exact_size(vec2(72.0, 72.0)),
                );
                ui.heading("AIO Asset Normalizer");
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .strong(),
                );
                ui.separator();
                ui.label(app.i18n.tr("label.description"));
                ui.add_space(4.0);
                ui.label(RichText::new(app.i18n.tr("label.author")).strong());
                ui.label(
                    RichText::new(app.i18n.tr("label.year"))
                        .color(Color32::GRAY),
                );
                ui.add_space(8.0);
                let link_text = RichText::new(app.i18n.tr("about.github"))
                    .color(Color32::from_rgb(100, 149, 237))
                    .underline();
                let link = ui.add(Label::new(link_text).sense(Sense::click()));
                if link.clicked() {
                    let _ = open::that(
                        "https://github.com/lihaozhe013/aio-asset-normalizer",
                    );
                }
            });
        });
}

fn render_preferences_dialog(app: &mut App, ctx: &three_d::egui::Context) {
    use three_d::egui::*;
    Window::new(app.i18n.tr("preferences.title"))
        .open(&mut app.show_preferences)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("label.blender_path"));
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
                if ui.button(app.i18n.tr("button.browse")).clicked() {
                    app.pending_browse_blender = true;
                }
            });

            if app.blender_path.is_some() {
                if ui.button(app.i18n.tr("button.reset")).clicked() {
                    app.blender_path = None;
                    app.needs_save = true;
                }
            }

            let detected = crate::modules::blender::bridge::find_blender(
                app.blender_path.as_deref(),
            )
            .map(|p| p.to_string_lossy().to_string());

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            match detected {
                Some(ref d) => {
                    ui.label(
                        RichText::new(
                            app.i18n
                                .text("label.detected", &[("path", d.clone())]),
                        )
                        .weak(),
                    );
                }
                None => {
                    ui.label(
                        RichText::new(app.i18n.tr("label.no_blender"))
                            .color(Color32::RED),
                    );
                }
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("preferences.language"));
                let mut preference = app.i18n.preference();
                ComboBox::from_id_salt("language")
                    .selected_text(preference.label(&app.i18n))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut preference,
                            LanguagePreference::Auto,
                            LanguagePreference::Auto.label(&app.i18n),
                        );
                        ui.selectable_value(
                            &mut preference,
                            LanguagePreference::English,
                            LanguagePreference::English.label(&app.i18n),
                        );
                        ui.selectable_value(
                            &mut preference,
                            LanguagePreference::Chinese,
                            LanguagePreference::Chinese.label(&app.i18n),
                        );
                    });
                if preference != app.i18n.preference() {
                    app.i18n.set_preference(preference);
                    app.needs_save = true;
                }
            });
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
