use crate::app::App;
use crate::app_fbx_converter::ConverterStatus;

/// Central-panel content for the FBX Converter page: Blender discovery,
/// batch start, and per-file status. The file tree lives in the left panel.
pub fn render(app: &mut App, ui: &mut three_d::egui::Ui) {
    use three_d::egui::*;

    ui.heading(app.i18n.tr("page.fbx_converter"));
    ui.label(app.i18n.tr("converter.hint"));
    ui.add_space(8.0);

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(app.i18n.tr("converter.blender_path"));
            let mut path = app.blender_path.clone().unwrap_or_default();
            let response = ui.add(
                TextEdit::singleline(&mut path)
                    .hint_text(app.i18n.tr("converter.blender_auto_detect"))
                    .id_salt("converter_blender_path")
                    .desired_width(280.0),
            );
            let commit_requested = response.lost_focus()
                && ui.input(|i| i.key_pressed(Key::Enter));
            if commit_requested {
                let trimmed = path.trim().to_owned();
                app.set_converter_blender_path(if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                });
            }
            if ui.button(app.i18n.tr("converter.browse")).clicked() {
                app.browse_converter_blender();
            }
            if app.blender_path.is_some()
                && ui.button(app.i18n.tr("converter.clear")).clicked()
            {
                app.set_converter_blender_path(None);
            }
        });
        match app.converter_blender_status() {
            Some(found) => {
                ui.label(app.i18n.text(
                    "converter.blender_detected",
                    &[("path", found.to_string_lossy().into_owned())],
                ));
            }
            None => {
                ui.colored_label(
                    Color32::from_rgb(225, 118, 117),
                    app.i18n.tr("converter.blender_missing"),
                );
            }
        }
    });
    ui.add_space(8.0);

    let selected_count = app.converter_file_tree.selected_files().len();
    let blender_available = app.converter_blender_status().is_some();
    let can_start =
        !app.converter_busy && selected_count > 0 && blender_available;
    if !app.converter_busy && selected_count == 0 {
        ui.label(app.i18n.tr("converter.select_hint"));
    }
    ui.add_enabled_ui(can_start, |ui| {
        let label = if app.converter_busy {
            app.i18n.tr("converter.converting").to_owned()
        } else {
            app.i18n.text(
                "converter.convert",
                &[("count", selected_count.to_string())],
            )
        };
        if ui
            .button(RichText::new(label).strong().size(15.0))
            .clicked()
        {
            app.start_converter_batch();
        }
    });

    if !app.converter_results.is_empty() {
        ui.separator();
        let done = app
            .converter_results
            .iter()
            .filter(|state| {
                matches!(
                    state.status,
                    ConverterStatus::Success | ConverterStatus::Failed
                )
            })
            .count();
        ui.label(app.i18n.text(
            "converter.progress",
            &[
                ("done", done.to_string()),
                ("total", app.converter_results.len().to_string()),
            ],
        ));
        ScrollArea::vertical()
            .max_height(260.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for state in &app.converter_results {
                    let name = state
                        .input
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let status = match state.status {
                        ConverterStatus::Pending => {
                            app.i18n.tr("converter.status_pending")
                        }
                        ConverterStatus::Running => {
                            app.i18n.tr("converter.status_running")
                        }
                        ConverterStatus::Success => {
                            app.i18n.tr("converter.status_success")
                        }
                        ConverterStatus::Failed => {
                            app.i18n.tr("converter.status_failed")
                        }
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{name}  {status}"));
                        if state.status == ConverterStatus::Success {
                            let output_name = state
                                .output
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();
                            ui.label(RichText::new(output_name).weak());
                        }
                        if let Some(error) = &state.error {
                            ui.colored_label(
                                Color32::from_rgb(225, 118, 117),
                                error,
                            );
                        }
                    });
                }
            });
    }
}
