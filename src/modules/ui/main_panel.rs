use crate::app::App;
use crate::modules::bvh::SuggestionConfidence;
use crate::modules::glb::TextureSlot;
use crate::modules::ui::{
    bottom_panel, fonts,
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

    Panel::left("glb_files")
        .resizable(true)
        .default_size(250.0)
        .min_size(170.0)
        .show_inside(ui, |ui| {
            let (changed, preview_path) = app.file_tree.render(ui, &app.i18n);
            if changed || app.file_tree.take_root_changed() {
                app.needs_save = true;
            }
            if let Some(path) = preview_path {
                match app.page {
                    Page::GlbEditor => app.preview_glb(&path),
                    Page::BvhStudio => app.load_bvh_target(&path),
                }
            }
        });

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

    bottom_panel::render(app, ui);

    let content_rect = CentralPanel::no_frame()
        .show_inside(ui, |ui| {
            let (_, rect) = ui.allocate_space(ui.available_size());
            rect
        })
        .inner;
    render_about_dialog(app, ui.ctx());
    content_rect
}

fn render_page_tabs(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;
    Frame::NONE
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
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
                    if app.glb_retarget_preview_active {
                        app.exit_glb_retarget_preview();
                    } else {
                        app.request_glb_reload(
                            crate::reload::GlbReloadKind::OpenModel,
                        );
                    }
                }
                if ui
                    .selectable_label(
                        app.page == Page::BvhStudio,
                        app.i18n.tr("page.bvh_studio"),
                    )
                    .clicked()
                {
                    app.page = Page::BvhStudio;
                    if app.glb_retarget_preview_active {
                        app.exit_glb_retarget_preview();
                    }
                    app.needs_bvh_target_reload = true;
                    app.refresh_v2_retarget_mapping();
                }
            });
        });
    ui.separator();
}

fn render_glb_inspector(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;
    ui.heading(app.i18n.tr("page.glb_editor"));
    if app.glb_retarget_preview_active {
        ui.colored_label(Color32::LIGHT_BLUE, "Retarget preview is active");
        if ui.button("Exit retarget preview").clicked() {
            app.exit_glb_retarget_preview();
        }
    }
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
        ui.label(app.i18n.tr("glb.preview_hint"));
        for (label, index) in [("X", 0), ("Y", 1), ("Z", 2)] {
            ui.horizontal(|ui| {
                ui.label(label);
                if ui
                    .add(
                        DragValue::new(
                            &mut app.orientation_euler_degrees[index],
                        )
                        .speed(1.0)
                        .suffix("°"),
                    )
                    .changed()
                {
                    app.mark_root_preview_dirty();
                }
            });
        }
        if ui.button(app.i18n.tr("glb.reset_rotation")).clicked() {
            app.reset_root_orientation();
        }
    });

    let transform_label = app.i18n.tr("glb.transform").to_owned();
    ui.collapsing(transform_label, |ui| {
        ui.horizontal(|ui| {
            ui.label(app.i18n.tr("glb.scale"));
            if ui
                .add(
                    DragValue::new(&mut app.root_scale)
                        .speed(0.01)
                        .range(0.001..=1000.0),
                )
                .changed()
            {
                app.mark_root_preview_dirty();
            }
            if ui.button(app.i18n.tr("glb.reset_scale")).clicked() {
                app.reset_root_scale();
            }
        });
        for (label, index) in [("X", 0), ("Y", 1), ("Z", 2)] {
            ui.horizontal(|ui| {
                ui.label(format!("Δ{label}"));
                if ui
                    .add(
                        DragValue::new(&mut app.root_translation[index])
                            .speed(0.01),
                    )
                    .changed()
                {
                    app.mark_root_preview_dirty();
                }
            });
        }
        if ui.button(app.i18n.tr("glb.reset_translation")).clicked() {
            app.reset_root_translation();
        }
    });
    if let Some(error) = app.root_preview_error() {
        ui.colored_label(Color32::YELLOW, error);
    }

    let animation_label = app.i18n.tr("glb.animation").to_owned();
    ui.collapsing(animation_label, |ui| {
        if summary.animations == 0 {
            ui.label(app.i18n.tr("glb.no_animations"));
        } else {
            ui.label(app.i18n.tr("glb.timeline_hint"));
            let entries = app.glb_animation_entries();
            let selected_index =
                app.glb_animation_index.min(entries.len().saturating_sub(1));
            if let Some((name, duration, playable, unsupported)) =
                entries.get(selected_index)
            {
                ui.horizontal(|ui| {
                    ui.label(app.i18n.tr("glb.active_animation"));
                    ui.label(name);
                });
                if !playable {
                    ui.colored_label(
                        Color32::YELLOW,
                        format!(
                            "{}: {}",
                            app.i18n.tr("glb.unsupported"),
                            unsupported
                        ),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label(app.i18n.tr("glb.animation_rate"));
                    ui.add(
                        DragValue::new(&mut app.glb_animation_rate)
                            .range(0.05..=8.0)
                            .speed(0.05)
                            .suffix("x"),
                    );
                    let can_reset =
                        (app.glb_animation_rate - 1.0).abs() > f32::EPSILON;
                    if ui
                        .add_enabled(
                            can_reset,
                            Button::new(app.i18n.tr("glb.reset_rate")),
                        )
                        .clicked()
                    {
                        app.reset_glb_animation_rate();
                    }
                });
                ui.label(app.i18n.tr("glb.animation_rate_hint"));
                ui.label(format!(
                    "{}: {:.3}s",
                    app.i18n.tr("glb.duration"),
                    duration
                ));
            }
            let mut trim_changed = ui
                .checkbox(
                    &mut app.trim_enabled,
                    app.i18n.tr("glb.trim_enabled"),
                )
                .changed();
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("glb.animation_index"));
                trim_changed |= ui
                    .add_enabled(
                        app.trim_enabled,
                        DragValue::new(&mut app.trim_animation)
                            .range(0..=summary.animations.saturating_sub(1)),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("glb.start"));
                trim_changed |= ui
                    .add_enabled(
                        app.trim_enabled,
                        DragValue::new(&mut app.trim_start).speed(0.001),
                    )
                    .changed();
                ui.label(app.i18n.tr("glb.end"));
                trim_changed |= ui
                    .add_enabled(
                        app.trim_enabled,
                        DragValue::new(&mut app.trim_end).speed(0.001),
                    )
                    .changed();
            });
            if trim_changed {
                app.trim_setting_changed();
            }
            ui.separator();
            let mut smart_loop_changed = ui
                .checkbox(
                    &mut app.smart_loop_enabled,
                    app.i18n.tr("glb.smart_loop"),
                )
                .changed();
            ui.horizontal(|ui| {
                ui.label(app.i18n.tr("glb.smart_loop_transition"));
                smart_loop_changed |= ui
                    .add_enabled(
                        app.smart_loop_enabled,
                        DragValue::new(&mut app.smart_loop_transition)
                            .range(0.01..=2.0)
                            .speed(0.001)
                            .fixed_decimals(3)
                            .suffix("s"),
                    )
                    .changed();
            });
            ui.label(app.i18n.tr("glb.smart_loop_hint"));
            if smart_loop_changed {
                app.smart_loop_setting_changed();
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
    ui.collapsing("Animation retargeting", |ui| {
        ui.label("Retarget the selected source animation onto a user-selected GLB Skin.");
        if ui.button("Choose target GLB").clicked() {
            app.import_glb_retarget_target();
        }
        if let Some(path) = app.glb_retarget_target_path.as_ref() {
            ui.label(path.display().to_string());
        } else {
            ui.label("No retarget target selected");
        }
        if let Some(target) = app.glb_retarget_target.as_ref() {
            let target_summary = target.summary();
            let source_skin_count = app.glb.as_ref().map(|source| source.summary().skins).unwrap_or(0);
            if target_summary.skins > 0 && source_skin_count > 0 {
                let mut changed = false;
                ComboBox::from_label("Source Skin")
                    .selected_text(app.retarget_source_skin_index.to_string())
                    .show_ui(ui, |ui| {
                        for index in 0..source_skin_count {
                            changed |= ui.selectable_value(
                                &mut app.retarget_source_skin_index,
                                index,
                                format!("Skin {index}"),
                            ).changed();
                        }
                    });
                ComboBox::from_label("Target Skin")
                    .selected_text(app.retarget_target_skin_index.to_string())
                    .show_ui(ui, |ui| {
                        for index in 0..target_summary.skins {
                            changed |= ui.selectable_value(
                                &mut app.retarget_target_skin_index,
                                index,
                                format!("Skin {index}"),
                            ).changed();
                        }
                    });
                if changed {
                    app.refresh_glb_retarget_mapping();
                }
            }
        }
        ui.checkbox(&mut app.retarget_root_motion, "Root motion");
        ui.checkbox(
            &mut app.retarget_normalize_initial_heading,
            "Normalize initial heading",
        );
        let mut root_scale_changed = false;
        if let Some(mapping) = app.retarget_mapping.as_mut() {
            if let Some(root_motion) = mapping.root_motion.as_mut() {
                ui.horizontal(|ui| {
                    ui.label("Root translation scale");
                    root_scale_changed = ui
                        .add(
                            DragValue::new(&mut root_motion.translation_scale)
                                .range(0.0001..=1000.0)
                                .speed(0.01),
                        )
                        .changed();
                });
            }
        }
        if root_scale_changed {
            app.refresh_glb_retarget_mapping();
        }
        ui.horizontal(|ui| {
            if ui.button("Import Mapping").clicked() {
                app.import_mapping();
            }
            if ui.button("Export Agent Mapping Prompt").clicked() {
                app.export_glb_retarget_agent_prompt();
            }
            if ui.button("Preview retargeted animation").clicked() {
                app.preview_glb_retarget();
            }
            if ui.button("Export retargeted GLB").clicked() {
                app.export_glb_retarget();
            }
        });
        if let Some(report) = app.retarget_validation.as_ref() {
            if report.is_valid() {
                ui.colored_label(Color32::LIGHT_GREEN, "Mapping v2 valid");
            } else {
                ui.colored_label(Color32::YELLOW, "Mapping v2 invalid");
                for error in report.errors.iter().take(4) {
                    ui.label(error);
                }
            }
        }
    });

    ui.separator();
    if ui.button(app.i18n.tr("glb.standardize")).clicked() {
        app.standardize();
    }
    if ui.button(app.i18n.tr("menu.export")).clicked() {
        app.dispatch_action(&MenuAction::Export);
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
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut app.canvas.show_source_skeleton,
            "Source BVH skeleton",
        );
        ui.checkbox(
            &mut app.canvas.show_target_skeleton,
            "Target Skin skeleton",
        );
    });
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
    ui.horizontal(|ui| {
        ui.label(app.i18n.tr("bvh.target_glb"));
        if ui.button("Choose target GLB").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("GLB", &["glb"])
                .pick_file()
            {
                app.load_bvh_target(&path);
            }
        }
    });
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
    let mut axes_changed = false;
    ComboBox::from_label("BVH up axis")
        .selected_text(app.bvh_up_axis.clone())
        .show_ui(ui, |ui| {
            for axis in ["+X", "-X", "+Y", "-Y", "+Z", "-Z"] {
                axes_changed |= ui
                    .selectable_value(
                        &mut app.bvh_up_axis,
                        axis.to_owned(),
                        axis,
                    )
                    .changed();
            }
        });
    ComboBox::from_label("BVH forward axis")
        .selected_text(app.bvh_forward_axis.clone())
        .show_ui(ui, |ui| {
            for axis in ["+X", "-X", "+Y", "-Y", "+Z", "-Z"] {
                axes_changed |= ui
                    .selectable_value(
                        &mut app.bvh_forward_axis,
                        axis.to_owned(),
                        axis,
                    )
                    .changed();
            }
        });
    ComboBox::from_label("BVH unit")
        .selected_text(app.bvh_unit.clone())
        .show_ui(ui, |ui| {
            for unit in ["m", "cm", "mm"] {
                axes_changed |= ui
                    .selectable_value(&mut app.bvh_unit, unit.to_owned(), unit)
                    .changed();
            }
        });
    if axes_changed {
        let up_axis = app.bvh_up_axis.clone();
        let forward_axis = app.bvh_forward_axis.clone();
        let unit = app.bvh_unit.clone();
        if let Some(mapping) = app.retarget_mapping.as_mut() {
            if mapping.source.kind == crate::modules::retarget::SourceKind::Bvh
            {
                mapping.source.up_axis = up_axis;
                mapping.source.forward_axis = forward_axis;
                mapping.source.unit = unit;
            }
        }
        if let Some(mapping) = app.mapping.as_mut() {
            mapping.source.up_axis = app.bvh_up_axis.clone();
            mapping.source.forward_axis = app.bvh_forward_axis.clone();
            mapping.source.unit = app.bvh_unit.clone();
        }
        app.refresh_v2_retarget_mapping();
        app.needs_bvh_target_reload = true;
    }
    if let Some(target) = app.bvh_target_glb.as_ref() {
        let summary = target.summary();
        if summary.skins > 0 {
            let mut skin_changed = false;
            ComboBox::from_label("Target Skin")
                .selected_text(app.retarget_target_skin_index.to_string())
                .show_ui(ui, |ui| {
                    for index in 0..summary.skins {
                        skin_changed |= ui
                            .selectable_value(
                                &mut app.retarget_target_skin_index,
                                index,
                                format!("Skin {index}"),
                            )
                            .changed();
                    }
                });
            if skin_changed {
                app.refresh_retarget_plan();
                app.refresh_v2_retarget_mapping();
            }
        }
    }
    if ui
        .checkbox(&mut app.retarget_root_motion, "Root motion")
        .changed()
        || ui
            .checkbox(
                &mut app.retarget_normalize_initial_heading,
                "Normalize initial heading",
            )
            .changed()
    {
        app.needs_bvh_target_reload = true;
    }
    let mut root_scale_changed = false;
    if let Some(mapping) = app.retarget_mapping.as_mut() {
        if let Some(root_motion) = mapping.root_motion.as_mut() {
            ui.horizontal(|ui| {
                ui.label("Root translation scale");
                root_scale_changed = ui
                    .add(
                        DragValue::new(&mut root_motion.translation_scale)
                            .range(0.0001..=1000.0)
                            .speed(0.01),
                    )
                    .changed();
            });
        }
    }
    if root_scale_changed {
        app.refresh_v2_retarget_mapping();
        app.needs_bvh_target_reload = true;
    }
    ui.horizontal(|ui| {
        if ui.button("Import Mapping").clicked() {
            app.import_mapping();
        }
        if ui.button("Export Agent Mapping Prompt").clicked() {
            app.export_retarget_agent_prompt();
        }
    });
    if let Some(report) = app.retarget_validation.as_ref() {
        ui.separator();
        ui.label("Mapping v2 validation");
        if report.is_valid() {
            ui.colored_label(Color32::LIGHT_GREEN, "Valid");
        } else {
            ui.colored_label(Color32::YELLOW, "Invalid");
        }
        for error in report.errors.iter().take(6) {
            ui.label(error);
        }
        for warning in report.warnings.iter().take(4) {
            ui.colored_label(Color32::YELLOW, format!("Warning: {warning}"));
        }
    }
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
                let frame_size = vec2(88.0, 88.0);
                let available = ui.available_width();
                let left_pad = ((available - frame_size.x) / 2.0).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(left_pad);
                    Frame::NONE
                        .fill(Color32::from_rgb(248, 240, 230))
                        .stroke(Stroke::new(
                            1.0_f32,
                            Color32::from_rgb(224, 198, 176),
                        ))
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(8))
                        .show(ui, |ui| {
                            ui.add(
                                Image::from_texture(&icon)
                                    .fit_to_exact_size(vec2(72.0, 72.0)),
                            );
                        });
                });
                ui.heading("AIO Asset Normalizer");
                ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                ui.separator();
                ui.label(app.i18n.tr("label.description"));
            });
        });
}
