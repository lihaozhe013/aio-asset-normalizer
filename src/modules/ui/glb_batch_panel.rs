use crate::app::App;
use crate::app_glb_batch::{GlbBatchFileStatus, GlbBatchPreflight};
use crate::modules::glb::{
    AnimationOutputMode, BatchNameSelector, GlbExportPreset,
};

pub fn render(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;

    let i18n = app.i18n.clone();
    let selected_count = app.file_tree.selected_count();
    ui.heading(i18n.tr("glb.batch_title"));
    ui.label(i18n.text(
        "glb.batch_selected",
        &[("count", selected_count.to_string())],
    ));
    if selected_count == 0 {
        ui.colored_label(Color32::YELLOW, i18n.tr("glb.batch_select_hint"));
        return;
    }

    let previous_recipe = app.glb_batch.recipe.clone();
    ui.collapsing(i18n.tr("glb.batch_recipe"), |ui| {
        ComboBox::from_label(i18n.tr("glb.export_preset"))
            .selected_text(batch_preset_label(
                &i18n,
                app.glb_batch.recipe.preset,
            ))
            .show_ui(ui, |ui| {
                for preset in [
                    GlbExportPreset::PreserveAll,
                    GlbExportPreset::CharacterPackage,
                    GlbExportPreset::SkeletonAnimation,
                ] {
                    ui.selectable_value(
                        &mut app.glb_batch.recipe.preset,
                        preset,
                        batch_preset_label(&i18n, preset),
                    );
                }
            });

        let compact =
            app.glb_batch.recipe.preset != GlbExportPreset::PreserveAll;
        ui.label(app.i18n.tr("glb.batch_scene_default"));
        ui.add_enabled_ui(compact, |ui| {
            render_name_selector(
                ui,
                &i18n,
                i18n.tr("glb.export_skin"),
                "glb.batch_automatic",
                &mut app.glb_batch.recipe.skin,
            );
        });
        ui.label(i18n.tr("glb.batch_all_animations"));

        let mut output = app.glb_batch.recipe.animation_output;
        ComboBox::from_label(i18n.tr("glb.export_animation_output"))
            .selected_text(match output {
                AnimationOutputMode::Combined => i18n.tr("glb.export_combined"),
                AnimationOutputMode::Split => i18n.tr("glb.export_split"),
            })
            .show_ui(ui, |ui| {
                ui.add_enabled_ui(compact, |ui| {
                    ui.selectable_value(
                        &mut output,
                        AnimationOutputMode::Combined,
                        i18n.tr("glb.export_combined"),
                    );
                    ui.selectable_value(
                        &mut output,
                        AnimationOutputMode::Split,
                        i18n.tr("glb.export_split"),
                    );
                });
            });
        app.glb_batch.recipe.animation_output = if compact {
            output
        } else {
            AnimationOutputMode::Combined
        };

        let mut remove_root_motion = app.glb_batch.recipe.remove_root_motion;
        if ui
            .add_enabled(
                compact,
                Checkbox::new(
                    &mut remove_root_motion,
                    i18n.tr("glb.export_remove_root_motion"),
                ),
            )
            .changed()
        {
            app.glb_batch.recipe.remove_root_motion = remove_root_motion;
        }
        if app.glb_batch.recipe.remove_root_motion && compact {
            render_name_selector(
                ui,
                &i18n,
                i18n.tr("glb.export_root_motion_node"),
                "glb.batch_automatic_root",
                &mut app.glb_batch.recipe.root_motion_node,
            );
        }
        if !compact {
            ui.label(i18n.tr("glb.batch_preserve_hint"));
        }
    });
    if app.glb_batch.recipe.preset == GlbExportPreset::PreserveAll {
        app.glb_batch.recipe.animation_output = AnimationOutputMode::Combined;
        app.glb_batch.recipe.remove_root_motion = false;
        app.glb_batch.recipe.root_motion_node =
            BatchNameSelector::AutomaticUnique;
    }
    if previous_recipe != app.glb_batch.recipe {
        app.invalidate_glb_batch_preflight();
    }

    ui.separator();
    ui.collapsing(i18n.tr("glb.batch_output"), |ui| {
        ui.horizontal(|ui| {
            if ui.button(i18n.tr("glb.batch_choose_output")).clicked() {
                app.choose_glb_batch_output_root();
            }
            if let Some(path) = app.glb_batch.output_root.as_ref() {
                ui.label(path.display().to_string());
            } else {
                ui.label(i18n.tr("glb.batch_no_output"));
            }
        });
        if ui
            .checkbox(
                &mut app.glb_batch.overwrite_existing,
                i18n.tr("glb.batch_overwrite"),
            )
            .changed()
        {
            app.invalidate_glb_batch_preflight();
        }
        ui.label(i18n.tr("glb.batch_output_layout"));
        ui.label(i18n.tr("glb.batch_output_naming"));
    });

    ui.separator();
    let can_preflight = !app.task_busy
        && !app.glb_batch.is_busy()
        && app.glb_batch.output_root.is_some();
    if ui
        .add_enabled(can_preflight, Button::new(i18n.tr("glb.batch_preflight")))
        .clicked()
    {
        app.start_glb_batch_preflight();
    }
    let can_export = !app.task_busy
        && !app.glb_batch.is_busy()
        && app.batch_preflight_is_current();
    if ui
        .add_enabled(can_export, Button::new(i18n.tr("glb.batch_export")))
        .clicked()
    {
        app.start_glb_batch_export();
    }
    if app.glb_batch.is_busy() {
        ui.label(i18n.tr("glb.batch_busy"));
    }
    if let Some(result) = app.glb_batch.last_result.as_ref() {
        ui.label(result);
    }

    if let Some(preflight) = app.glb_batch.preflight.clone() {
        render_preflight(ui, app, &preflight);
    }
}

fn render_name_selector(
    ui: &mut three_d::egui::Ui,
    i18n: &crate::modules::i18n::I18n,
    label: &str,
    automatic_label_key: &str,
    selector: &mut BatchNameSelector,
) {
    use three_d::egui::*;

    let exact = matches!(selector, BatchNameSelector::Exact(_));
    let mut mode = exact;
    ComboBox::from_label(label)
        .selected_text(if exact {
            i18n.tr("glb.batch_exact_name")
        } else {
            i18n.tr(automatic_label_key)
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut mode, false, i18n.tr(automatic_label_key));
            ui.selectable_value(
                &mut mode,
                true,
                i18n.tr("glb.batch_exact_name"),
            );
        });
    if mode {
        let mut name = match selector {
            BatchNameSelector::Exact(name) => name.clone(),
            BatchNameSelector::AutomaticUnique => String::new(),
        };
        if ui
            .add(
                TextEdit::singleline(&mut name)
                    .hint_text(i18n.tr("glb.batch_name_hint")),
            )
            .changed()
            || !matches!(selector, BatchNameSelector::Exact(_))
        {
            *selector = BatchNameSelector::Exact(name);
        }
    } else {
        *selector = BatchNameSelector::AutomaticUnique;
    }
}

fn render_preflight(
    ui: &mut three_d::egui::Ui,
    app: &App,
    preflight: &GlbBatchPreflight,
) {
    use three_d::egui::*;

    ui.separator();
    ui.label(app.i18n.tr("glb.batch_results"));
    let ready = preflight
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.status,
                GlbBatchFileStatus::Ready
                    | GlbBatchFileStatus::ReadyWithWarnings
            )
        })
        .count();
    let errors = preflight
        .entries
        .iter()
        .filter(|entry| entry.status == GlbBatchFileStatus::Error)
        .count();
    ui.label(app.i18n.text(
        "glb.batch_result_summary",
        &[
            ("ready", ready.to_string()),
            ("errors", errors.to_string()),
            ("total", preflight.entries.len().to_string()),
        ],
    ));
    let done = preflight
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.status,
                GlbBatchFileStatus::Succeeded
                    | GlbBatchFileStatus::Failed
                    | GlbBatchFileStatus::Skipped
            )
        })
        .count();
    ui.label(app.i18n.text(
        "glb.batch_progress",
        &[
            ("done", done.to_string()),
            ("total", preflight.entries.len().to_string()),
        ],
    ));
    ScrollArea::vertical()
        .max_height(320.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in &preflight.entries {
                let name = entry
                    .input
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let status = match entry.status {
                    GlbBatchFileStatus::Ready => {
                        app.i18n.tr("glb.batch_status_ready")
                    }
                    GlbBatchFileStatus::ReadyWithWarnings => {
                        app.i18n.tr("glb.batch_status_warning")
                    }
                    GlbBatchFileStatus::Error => {
                        app.i18n.tr("glb.batch_status_error")
                    }
                    GlbBatchFileStatus::Exporting => {
                        app.i18n.tr("glb.batch_status_exporting")
                    }
                    GlbBatchFileStatus::Succeeded => {
                        app.i18n.tr("glb.batch_status_success")
                    }
                    GlbBatchFileStatus::Failed => {
                        app.i18n.tr("glb.batch_status_failed")
                    }
                    GlbBatchFileStatus::Skipped => {
                        app.i18n.tr("glb.batch_status_skipped")
                    }
                };
                ui.collapsing(format!("{name} — {status}"), |ui| {
                    for warning in &entry.warnings {
                        ui.colored_label(Color32::YELLOW, warning);
                    }
                    if let Some(error) = entry.error.as_ref() {
                        ui.colored_label(Color32::RED, error);
                    }
                    for output in &entry.outputs {
                        ui.label(output.path.display().to_string());
                        if let Some(report) = output.report.as_ref() {
                            ui.label(format!(
                                "{}: {} root-motion channels",
                                app.i18n.tr("glb.batch_estimate"),
                                report.root_motion_channels_modified
                            ));
                        }
                    }
                    for output in &entry.completed_outputs {
                        ui.colored_label(
                            Color32::LIGHT_GREEN,
                            format!(
                                "{}: {}",
                                app.i18n.tr("glb.batch_written"),
                                output.display()
                            ),
                        );
                    }
                });
            }
        });
}

fn batch_preset_label(
    i18n: &crate::modules::i18n::I18n,
    preset: GlbExportPreset,
) -> String {
    i18n.tr(match preset {
        GlbExportPreset::PreserveAll => "glb.export_preset_all",
        GlbExportPreset::CharacterPackage => "glb.export_preset_character",
        GlbExportPreset::SkeletonAnimation => "glb.export_preset_skeleton",
    })
    .to_owned()
}
