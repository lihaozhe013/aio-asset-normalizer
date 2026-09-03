use crate::app::App;
use crate::modules::bvh::SuggestionConfidence;
use crate::modules::ui::glb_export_panel;
use crate::modules::ui::menu_bar::MenuAction;
use crate::modules::ui::skeleton_panel::{self, SkeletonPanelContext};

pub fn render(app: &mut App, ui: &mut three_d::egui::Ui) {
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
    skeleton_panel::render(
        app,
        ui,
        SkeletonPanelContext::Bvh {
            include_source_fit: true,
        },
    );
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
    let unit_shortcut = app.bvh_unit.clone();
    let bvh_diagnostic = app
        .bvh
        .as_ref()
        .and_then(|document| app.bvh_span_diagnostic(document).ok());
    if let Some((raw_span, converted_span)) = bvh_diagnostic {
        ui.label(format!(
            "Raw span: {:.4} / Converted span: {:.4} m",
            raw_span, converted_span
        ));
        if converted_span < 0.05 || converted_span > 100.0 {
            let hint = if converted_span < 0.05 {
                "Try Use m if this file stores metre offsets."
            } else {
                "Try Use cm or Use mm if this file stores smaller offsets."
            };
            ui.colored_label(
                Color32::YELLOW,
                format!(
                    "Current unit produces a {:.3} m skeleton. {hint}",
                    converted_span,
                ),
            );
        }
        ui.horizontal(|ui| {
            for unit in ["m", "cm", "mm"] {
                if ui.button(format!("Use {unit}")).clicked()
                    && unit_shortcut != unit
                {
                    app.set_bvh_unit_from_ui(unit);
                }
            }
        });
    }
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
        app.needs_bvh_skeleton_reload = true;
        app.bvh_camera_focus_pending = true;
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
                app.needs_bvh_target_reload = true;
            }
        }
    }
    if let Some(target) = app.bvh_target_glb.as_ref() {
        if let Ok(catalog) = target.export_catalog() {
            let i18n = app.i18n.clone();
            app.bvh_export_selection.skin_index =
                Some(app.retarget_target_skin_index);
            glb_export_panel::render_selection_controls(
                ui,
                &i18n,
                &catalog,
                &mut app.bvh_export_selection,
                false,
                false,
            );
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
