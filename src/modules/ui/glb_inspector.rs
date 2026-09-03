use crate::app::App;
use crate::modules::glb::{TextureSlot, UpAxisPreset};
use crate::modules::ui::glb_export_panel;
use crate::modules::ui::skeleton_panel::{self, SkeletonPanelContext};

pub fn render(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;
    ui.heading(app.i18n.tr("page.glb_editor"));
    if app.glb_retarget_preview_active {
        ui.colored_label(Color32::LIGHT_BLUE, "Retarget preview is active");
        if ui.button("Exit retarget preview").clicked() {
            app.exit_glb_retarget_preview();
        }
        skeleton_panel::render(
            app,
            ui,
            SkeletonPanelContext::Bvh {
                include_source_fit: false,
            },
        );
    }
    if !app.glb_retarget_preview_active && app.canvas.has_glb_skeleton() {
        ui.colored_label(
            Color32::LIGHT_BLUE,
            app.i18n.tr(if app.canvas.is_glb_skeleton_only_preview() {
                "glb.skeleton_preview"
            } else {
                "glb.skeleton_preview_active"
            }),
        );
        ui.checkbox(
            &mut app.canvas.show_glb_skeleton,
            app.i18n.tr("glb.show_skeleton"),
        );
        skeleton_panel::render(app, ui, SkeletonPanelContext::Glb);
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
        let up_axis_label = app.i18n.tr("glb.up_axis").to_owned();
        let selected_text = match UpAxisPreset::from_correction_euler_degrees(
            app.orientation_euler_degrees,
        ) {
            Some(preset) => app.i18n.tr(preset.label_key()),
            None => app.i18n.tr("glb.up_axis.custom"),
        }
        .to_owned();
        ComboBox::from_label(up_axis_label)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for preset in UpAxisPreset::ALL {
                    let label = app.i18n.tr(preset.label_key());
                    let is_selected =
                        UpAxisPreset::from_correction_euler_degrees(
                            app.orientation_euler_degrees,
                        ) == Some(preset);
                    if ui.selectable_label(is_selected, label).clicked() {
                        app.orientation_euler_degrees =
                            preset.correction_euler_degrees();
                        app.mark_root_preview_dirty();
                        app.log.append(&format!(
                            "[glb_editor] Applied up-axis preset {preset:?}"
                        ));
                    }
                }
            });
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
        if ui
            .checkbox(
                &mut app.bake_root_transform,
                app.i18n.tr("glb.bake_root_transform"),
            )
            .changed()
        {
            app.glb_export_estimate = None;
        }
        ui.label(app.i18n.tr("glb.bake_root_transform_hint"));
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
            ui.colored_label(
                Color32::LIGHT_BLUE,
                app.i18n.tr("glb.no_animations"),
            );
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
                    if ui
                        .add(
                            DragValue::new(&mut app.glb_animation_rate)
                                .range(0.05..=8.0)
                                .speed(0.05)
                                .suffix("x"),
                        )
                        .changed()
                    {
                        app.glb_export_estimate = None;
                    }
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

    if let Some(target) = app.glb_retarget_target.as_ref() {
        if let Ok(catalog) = target.export_catalog() {
            let i18n = app.i18n.clone();
            glb_export_panel::render_selection_controls(
                ui,
                &i18n,
                &catalog,
                &mut app.glb_retarget_export_selection,
                false,
                false,
            );
        }
    }

    ui.separator();
    glb_export_panel::render(app, ui);
    ui.separator();
    if ui.button(app.i18n.tr("glb.standardize")).clicked() {
        app.standardize();
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
