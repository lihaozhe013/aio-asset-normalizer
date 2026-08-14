use crate::app::App;
use crate::modules::bvh::SuggestionConfidence;
use crate::modules::glb::TextureSlot;
use crate::modules::ui::{
    fonts,
    menu_bar::{self, MenuAction, Page},
};

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
    let actions = menu_bar::render(
        ui,
        &app.i18n,
        app.page,
        app.canvas.show_grid,
        app.canvas.show_axes,
        app.canvas.show_origin,
    );
    for action in actions {
        app.dispatch_action(&action);
    }
    render_page_tabs(app, ui);

    if app.page == Page::GlbEditor {
        Panel::left("glb_files")
            .resizable(true)
            .default_size(250.0)
            .min_size(170.0)
            .show_inside(ui, |ui| {
                let (changed, preview_path) =
                    app.file_tree.render(ui, &app.i18n);
                if changed || app.file_tree.take_root_changed() {
                    app.needs_save = true;
                }
                if let Some(path) = preview_path {
                    app.preview_glb(&path);
                }
            });
    }

    Panel::right("inspector")
        .resizable(true)
        .default_size(320.0)
        .min_size(250.0)
        .show_inside(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| match app.page {
                Page::GlbEditor => render_glb_inspector(app, ui),
                Page::BvhStudio => render_bvh_inspector(app, ui),
            });
        });

    Panel::bottom("log")
        .resizable(true)
        .default_size(140.0)
        .min_size(80.0)
        .show_inside(ui, |ui| {
            if app.log.render(ui, &app.i18n) {
                app.needs_save = true;
            }
        });

    if app.page == Page::GlbEditor && !app.glb_animation_entries().is_empty() {
        Panel::bottom("glb_animation_timeline")
            .resizable(false)
            .default_size(94.0)
            .min_size(82.0)
            .show_inside(ui, |ui| {
                render_glb_animation_timeline(app, ui);
            });
    }

    let content_rect = ui.available_rect_before_wrap();
    render_about_dialog(app, ui.ctx());
    content_rect
}

fn render_page_tabs(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;
    ui.horizontal(|ui| {
        ui.label(RichText::new("AIO Asset Normalizer").strong());
        ui.separator();
        if ui
            .selectable_label(
                app.page == Page::GlbEditor,
                app.i18n.tr("page.glb_editor"),
            )
            .clicked()
        {
            app.page = Page::GlbEditor;
        }
        if ui
            .selectable_label(
                app.page == Page::BvhStudio,
                app.i18n.tr("page.bvh_studio"),
            )
            .clicked()
        {
            app.page = Page::BvhStudio;
        }
    });
    ui.separator();
}

fn render_glb_inspector(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;
    ui.heading(app.i18n.tr("page.glb_editor"));
    let Some(document) = app.glb.as_ref() else {
        ui.label(app.i18n.tr("glb.open_hint"));
        return;
    };
    let summary = document.summary();
    ui.label(
        document
            .source_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| app.i18n.tr("glb.unsaved").to_owned()),
    );
    Grid::new("glb_summary").num_columns(2).show(ui, |ui| {
        for (label, value) in [
            (app.i18n.tr("glb.scenes"), summary.scenes),
            (app.i18n.tr("glb.nodes"), summary.nodes),
            (app.i18n.tr("glb.meshes"), summary.meshes),
            (app.i18n.tr("glb.materials"), summary.materials),
            (app.i18n.tr("glb.skins"), summary.skins),
            (app.i18n.tr("glb.animations"), summary.animations),
            (app.i18n.tr("glb.images"), summary.images),
        ] {
            ui.label(label);
            ui.label(value.to_string());
            ui.end_row();
        }
    });
    if !summary.extensions.is_empty() {
        ui.label(format!("Extensions: {}", summary.extensions.join(", ")));
    }
    render_named_items(
        ui,
        app.i18n.tr("glb.scene_names"),
        document.scene_names(),
    );
    render_named_items(
        ui,
        app.i18n.tr("glb.node_names"),
        document.node_names(),
    );
    render_named_items(
        ui,
        app.i18n.tr("glb.mesh_names"),
        document.mesh_names(),
    );
    render_named_items(
        ui,
        app.i18n.tr("glb.material_names"),
        document.material_names(),
    );
    render_named_items(
        ui,
        app.i18n.tr("glb.animation_names"),
        document.animation_names(),
    );

    ui.separator();
    let orientation_label = app.i18n.tr("glb.orientation").to_owned();
    ui.collapsing(orientation_label, |ui| {
        ui.label(app.i18n.tr("glb.orientation_hint"));
        for (label, index) in [("X", 0), ("Y", 1), ("Z", 2)] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(
                    DragValue::new(&mut app.rotation_axis[index]).speed(0.1),
                );
            });
        }
        ui.add(
            DragValue::new(&mut app.rotation_degrees)
                .speed(1.0)
                .suffix("°"),
        );
        if ui.button(app.i18n.tr("glb.apply_rotation")).clicked() {
            app.apply_rotation();
        }
    });

    let transform_label = app.i18n.tr("glb.transform").to_owned();
    ui.collapsing(transform_label, |ui| {
        ui.horizontal(|ui| {
            ui.label(app.i18n.tr("glb.scale"));
            ui.add(
                DragValue::new(&mut app.root_scale)
                    .speed(0.01)
                    .range(0.001..=1000.0),
            );
            if ui.button(app.i18n.tr("glb.apply")).clicked() {
                app.apply_scale();
            }
        });
        for (label, index) in [("X", 0), ("Y", 1), ("Z", 2)] {
            ui.horizontal(|ui| {
                ui.label(format!("Δ{label}"));
                ui.add(
                    DragValue::new(&mut app.root_translation[index])
                        .speed(0.01),
                );
            });
        }
        if ui.button(app.i18n.tr("glb.apply_translation")).clicked() {
            app.apply_translation();
        }
    });

    let animation_label = app.i18n.tr("glb.animation").to_owned();
    ui.collapsing(animation_label, |ui| {
        if summary.animations == 0 {
            ui.label(app.i18n.tr("glb.no_animations"));
        } else {
            ui.label(app.i18n.tr("glb.timeline_hint"));
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("glb.animation_index"));
                ui.add(
                    DragValue::new(&mut app.trim_animation)
                        .range(0..=summary.animations.saturating_sub(1)),
                );
            });
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("glb.start"));
                ui.add(DragValue::new(&mut app.trim_start).speed(0.01));
                ui.label(app.i18n.tr("glb.end"));
                ui.add(DragValue::new(&mut app.trim_end).speed(0.01));
            });
            if ui.button(app.i18n.tr("glb.trim_animation")).clicked() {
                app.trim_glb_animation();
            }
        }
    });

    let replacement_label = app.i18n.tr("glb.replacements").to_owned();
    ui.collapsing(replacement_label, |ui| {
        ui.label(app.i18n.tr("glb.texture_target"));
        ui.horizontal(|ui| {
            ui.label(app.i18n.tr("glb.texture_mesh"));
            ui.add(
                DragValue::new(&mut app.texture_mesh)
                    .range(0..=summary.meshes.saturating_sub(1)),
            );
            ui.label(app.i18n.tr("glb.texture_primitive"));
            ui.add(DragValue::new(&mut app.texture_primitive).range(0..=64));
        });
        ComboBox::from_label(app.i18n.tr("glb.texture_slot"))
            .selected_text(app.texture_slot.label())
            .show_ui(ui, |ui| {
                for slot in TextureSlot::ALL {
                    ui.selectable_value(
                        &mut app.texture_slot,
                        slot,
                        slot.label(),
                    );
                }
            });
        ui.checkbox(
            &mut app.texture_duplicate_shared,
            app.i18n.tr("glb.texture_duplicate_shared"),
        );
        if ui.button(app.i18n.tr("glb.replace_texture")).clicked() {
            app.replace_glb_texture();
        }
        ui.add_enabled(false, Button::new(app.i18n.tr("glb.replace_skeleton")));
        ui.label(app.i18n.tr("glb.replacements_hint"));
    });

    ui.separator();
    if ui.button(app.i18n.tr("glb.standardize")).clicked() {
        app.standardize();
    }
    if ui.button(app.i18n.tr("menu.export")).clicked() {
        app.dispatch_action(&MenuAction::Export);
    }
}

fn render_glb_animation_timeline(app: &mut App, ui: &mut three_d::egui::Ui) {
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
    ui.horizontal(|ui| {
        ui.label(RichText::new(app.i18n.tr("glb.timeline")).strong());
        let mut requested_index = selected_index;
        ComboBox::from_id_salt("glb_animation_timeline_clip")
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

            if ui
                .button(if app.glb_animation_playing {
                    app.i18n.tr("glb.pause")
                } else {
                    app.i18n.tr("glb.play")
                })
                .clicked()
            {
                app.glb_animation_playing = !app.glb_animation_playing;
            }
            if ui.button(app.i18n.tr("glb.step_back")).clicked() {
                app.step_glb_animation(-1.0);
            }
            if ui.button(app.i18n.tr("glb.step_forward")).clicked() {
                app.step_glb_animation(1.0);
            }
            ui.checkbox(&mut app.glb_animation_loop, app.i18n.tr("glb.loop"));
            ui.label(app.i18n.tr("glb.speed"));
            ui.add(
                DragValue::new(&mut app.glb_animation_speed)
                    .range(0.05..=8.0)
                    .speed(0.05),
            );
            ui.label(format!(
                "{:.3}s / {:.3}s",
                app.glb_animation_time, duration
            ));
        }
    });

    if let Some((_, duration, playable, _)) = entries.get(selected_index) {
        if *playable {
            let mut time = app.glb_animation_time;
            let slider_width = ui.available_width();
            if ui
                .add_sized(
                    [slider_width, 24.0],
                    Slider::new(&mut time, 0.0..=*duration)
                        .text(app.i18n.tr("glb.time")),
                )
                .changed()
            {
                app.set_glb_animation_time(time);
            }
        }
    }
}

fn render_named_items(
    ui: &mut three_d::egui::Ui,
    label: &str,
    names: Vec<String>,
) {
    if names.is_empty() {
        return;
    }
    let heading = format!("{label} ({})", names.len());
    ui.collapsing(heading, |ui| {
        for name in names {
            ui.label(name);
        }
    });
}

fn render_bvh_inspector(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;
    ui.heading(app.i18n.tr("page.bvh_studio"));
    if let Some(document) = app.bvh.as_ref() {
        Grid::new("bvh_summary").num_columns(2).show(ui, |ui| {
            for (label, value) in [
                (app.i18n.tr("bvh.joints"), document.joints.len().to_string()),
                (app.i18n.tr("bvh.frames"), document.frames.len().to_string()),
                (
                    app.i18n.tr("bvh.frame_time"),
                    format!("{:.4}s", document.frame_time),
                ),
                (
                    app.i18n.tr("bvh.duration"),
                    format!("{:.3}s", document.duration()),
                ),
            ] {
                ui.label(label);
                ui.label(value);
                ui.end_row();
            }
        });
        ui.separator();
        let mut requested_frame = None;
        ui.horizontal(|ui| {
            ui.label(app.i18n.tr("bvh.frame"));
            let mut frame = app.bvh_frame;
            if ui
                .add(
                    DragValue::new(&mut frame)
                        .range(0..=document.frames.len().saturating_sub(1)),
                )
                .changed()
            {
                requested_frame = Some(frame);
            }
            ui.label(format!("/ {}", document.frames.len().saturating_sub(1)));
        });
        if let Some(frame) = requested_frame {
            app.set_bvh_frame(frame);
        }
        ui.horizontal(|ui| {
            if ui
                .button(if app.bvh_playing {
                    app.i18n.tr("bvh.pause")
                } else {
                    app.i18n.tr("bvh.play")
                })
                .clicked()
            {
                app.bvh_playing = !app.bvh_playing;
                app.bvh_playback_accumulator = 0.0;
            }
            ui.label(app.i18n.tr("bvh.speed"));
            ui.add(
                DragValue::new(&mut app.bvh_playback_speed)
                    .speed(0.05)
                    .range(0.05..=8.0)
                    .suffix("x"),
            );
        });
        ui.checkbox(&mut app.bvh_reduce_keys, app.i18n.tr("bvh.reduce_keys"));
        if app.bvh_reduce_keys {
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("bvh.key_tolerance"));
                ui.add(
                    DragValue::new(&mut app.bvh_key_tolerance)
                        .speed(0.0001)
                        .range(0.000001..=1.0),
                );
            });
        }
        ui.label(app.i18n.tr("bvh.trim"));
        ui.horizontal(|ui| {
            ui.label(app.i18n.tr("glb.start"));
            ui.add(DragValue::new(&mut app.bvh_trim_start).speed(0.01));
            ui.label(app.i18n.tr("glb.end"));
            ui.add(DragValue::new(&mut app.bvh_trim_end).speed(0.01));
        });
        if ui.button(app.i18n.tr("bvh.trim_apply")).clicked() {
            app.trim_bvh();
        }
        if ui.button(app.i18n.tr("menu.export")).clicked() {
            app.dispatch_action(&MenuAction::Export);
        }
        if app.task_busy {
            ui.label(app.i18n.tr("bvh.export_busy"));
        } else {
            if ui.button(app.i18n.tr("bvh.export_glb")).clicked() {
                app.dispatch_action(&MenuAction::ExportBvhGlb);
            }
            if ui.button(app.i18n.tr("bvh.export_clip")).clicked() {
                app.dispatch_action(&MenuAction::ExportBvhAnimationClip);
            }
        }
    } else {
        ui.label(app.i18n.tr("bvh.open_hint"));
    }
    ui.separator();
    ui.label(app.i18n.tr("bvh.target_glb"));
    if let Some(path) = &app.bvh_target_path {
        ui.label(path.display().to_string());
        if let Some(target) = app.bvh_target_glb.as_ref() {
            let summary = target.summary();
            ui.label(format!(
                "{} {} / {} {}",
                app.i18n.tr("glb.nodes"),
                summary.nodes,
                app.i18n.tr("glb.skins"),
                summary.skins
            ));
        }
    } else {
        ui.label(app.i18n.tr("bvh.no_target_glb"));
    }
    ui.label(app.i18n.tr("bvh.target_hint"));
    ui.separator();
    ui.label(app.i18n.tr("bvh.mapping"));
    if let Some(path) = &app.mapping_path {
        ui.label(path.display().to_string());
    } else {
        ui.label(app.i18n.tr("bvh.no_mapping"));
    }
    if let Some(plan) = app.retarget_plan.as_ref() {
        ui.label(format!(
            "{}: {}",
            app.i18n.tr("bvh.mapped"),
            plan.source_to_target.len()
        ));
        if !plan.unmapped_source_joints.is_empty() {
            ui.label(format!(
                "{}: {}",
                app.i18n.tr("bvh.unmapped"),
                plan.unmapped_source_joints.join(", ")
            ));
        }
    }
    if let (Some(document), Some(report)) =
        (app.bvh.as_ref(), app.mapping_report.as_ref())
    {
        ui.separator();
        ui.label(app.i18n.tr("bvh.mapping_report"));
        ui.label(format!(
            "{}: {:.1}%",
            app.i18n.tr("bvh.coverage"),
            report.coverage_percent(document.joints.len())
        ));
        if report.is_valid() {
            ui.colored_label(
                Color32::LIGHT_GREEN,
                app.i18n.tr("bvh.mapping_valid"),
            );
        } else {
            ui.colored_label(
                Color32::YELLOW,
                app.i18n.tr("bvh.mapping_invalid"),
            );
        }
        for (label, values) in [
            (
                app.i18n.tr("bvh.unknown_source"),
                &report.unknown_source_joints,
            ),
            (
                app.i18n.tr("bvh.unknown_target"),
                &report.unknown_target_nodes,
            ),
            (
                app.i18n.tr("bvh.duplicate_target"),
                &report.duplicate_target_nodes,
            ),
        ] {
            if !values.is_empty() {
                ui.label(format!("{label}: {}", values.join(", ")));
            }
        }
        if let Some(error) = report.contract_error.as_deref() {
            ui.label(error);
        }
    }
    if !app.mapping_suggestions.is_empty() {
        ui.collapsing(app.i18n.tr("bvh.suggestions"), |ui| {
            for suggestion in app.mapping_suggestions.iter().take(12) {
                let confidence = match suggestion.confidence {
                    SuggestionConfidence::Exact => {
                        app.i18n.tr("bvh.suggestion_exact")
                    }
                    SuggestionConfidence::Normalized => {
                        app.i18n.tr("bvh.suggestion_normalized")
                    }
                };
                ui.label(format!(
                    "{} → {} ({confidence})",
                    suggestion.source_joint, suggestion.target_node
                ));
            }
        });
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
        let decoded = image::load_from_memory(ICON_PNG)
            .expect("failed to decode the About dialog icon")
            .to_rgba8();
        let image = ColorImage::from_rgba_unmultiplied(
            [decoded.width() as usize, decoded.height() as usize],
            decoded.as_raw(),
        );
        app.about_icon = Some(ctx.load_texture(
            "aio-asset-normalizer-about-icon",
            image,
            TextureOptions::LINEAR,
        ));
    }
    let icon = app.about_icon.as_ref().unwrap().clone();
    Window::new(app.i18n.tr("about.title"))
        .open(&mut app.show_about)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add(
                    Image::from_texture(&icon)
                        .fit_to_exact_size(vec2(72.0, 72.0)),
                );
                ui.heading("AIO Asset Normalizer");
                ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.label(app.i18n.tr("label.description"));
            });
        });
}
