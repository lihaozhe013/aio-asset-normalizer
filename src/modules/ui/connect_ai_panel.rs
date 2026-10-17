//! Central-panel content for the Connect AI page: MCP launch detection,
//! agent setup prompts, and GUI bridge diagnostics.

use crate::app::App;
use crate::app_mcp_setup::{CopyFailure, CopyStatus};
use crate::modules::mcp_setup::McpPromptMode;
use crate::modules::ui::theme;
use three_d::egui::{self, RichText, ScrollArea, TextWrapMode};

/// Maximum content width; the page reads best as a single narrow column.
const CONTENT_WIDTH: f32 = 640.0;

pub fn render(app: &mut App, ui: &mut three_d::egui::Ui) {
    let runtime_status = app.automation_status();
    let mut stop_task_requested = false;
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Pad the column away from the panel edges; the central panel is
            // frameless, so without this the content hugs the left edge.
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(24, 16))
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(CONTENT_WIDTH));
                    ui.heading(app.i18n.tr("page.connect_ai"));
                    ui.label(app.i18n.tr("connect_ai.hint"));
                    ui.add_space(8.0);
                    render_contents(app, runtime_status.as_ref(), &mut stop_task_requested, ui);
                });
        });
    if stop_task_requested {
        app.cancel_active_automation_job();
    }
}

fn render_contents(
    app: &mut App,
    runtime_status: Option<&(String, Option<String>, bool)>,
    stop_task_requested: &mut bool,
    ui: &mut three_d::egui::Ui,
) {
    let state = &mut app.mcp_setup;

    let mut selected_mode = state.mode();
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut selected_mode,
            McpPromptMode::PersistentSetup,
            app.i18n.tr("connect_ai.mode_setup"),
        );
        ui.selectable_value(
            &mut selected_mode,
            McpPromptMode::OneTimeUse,
            app.i18n.tr("connect_ai.mode_one_time"),
        );
    });
    state.set_mode(selected_mode);
    ui.label(match state.mode() {
        McpPromptMode::PersistentSetup => app.i18n.tr("connect_ai.mode_setup_hint"),
        McpPromptMode::OneTimeUse => app.i18n.tr("connect_ai.mode_one_time_hint"),
    });
    ui.add_space(8.0);

    // The launch result is cloned so the Refresh buttons can re-run the
    // detection while the arm still displays the previous result.
    let launch = state.launch().cloned();
    match launch {
        Some(Ok(spec)) if state.prompt_error().is_none() => {
            ui.horizontal(|ui| {
                ui.colored_label(theme::SUCCESS, app.i18n.tr("connect_ai.ready"));
                if ui.button(app.i18n.tr("connect_ai.refresh")).clicked() {
                    state.refresh();
                }
            });
            ui.label(app.i18n.tr("connect_ai.server_executable"));
            selectable_path(ui, &spec.command.display().to_string());
        }
        Some(Ok(spec)) => {
            ui.colored_label(theme::ERROR, app.i18n.tr("connect_ai.unavailable"));
            selectable_path(ui, &spec.command.display().to_string());
            if let Some(error) = state.prompt_error() {
                ui.label(error);
            }
            if ui.button(app.i18n.tr("connect_ai.refresh")).clicked() {
                state.refresh();
            }
        }
        Some(Err(error)) => {
            ui.horizontal(|ui| {
                ui.colored_label(theme::ERROR, app.i18n.tr("connect_ai.unavailable"));
                if ui.button(app.i18n.tr("connect_ai.refresh")).clicked() {
                    state.refresh();
                }
            });
            ui.label(error.to_string());
        }
        None => {
            ui.label(app.i18n.tr("connect_ai.path_unchecked"));
            if ui.button(app.i18n.tr("connect_ai.refresh")).clicked() {
                state.refresh();
            }
        }
    }

    ui.add_space(8.0);
    egui::CollapsingHeader::new(app.i18n.tr("connect_ai.connection_details"))
        .default_open(false)
        .show(ui, |ui| {
            ui.label(app.i18n.text(
                "connect_ai.server_name",
                &[("name", "aio-asset-normalizer".to_owned())],
            ));
            ui.label(
                app.i18n
                    .text("connect_ai.transport", &[("transport", "stdio".to_owned())]),
            );
            if let Some(Ok(spec)) = state.launch() {
                ui.label(app.i18n.tr("connect_ai.command"));
                selectable_path(ui, &spec.command.display().to_string());
                ui.label(app.i18n.text(
                    "connect_ai.arguments",
                    &[(
                        "arguments",
                        serde_json::to_string(&spec.args).unwrap_or_else(|_| "[]".to_owned()),
                    )],
                ));
            }
        });

    egui::CollapsingHeader::new(app.i18n.tr("connect_ai.diagnostics"))
        .default_open(false)
        .show(ui, |ui| {
            match runtime_status {
                Some((session, task, recently_active)) => {
                    ui.label(
                        app.i18n
                            .text("connect_ai.gui_session", &[("session", session.clone())]),
                    );
                    ui.label(if *recently_active {
                        app.i18n.tr("connect_ai.recent_request")
                    } else {
                        app.i18n.tr("connect_ai.waiting_request")
                    });
                    if let Some(task) = task {
                        ui.label(
                            app.i18n
                                .text("connect_ai.active_task", &[("task", task.clone())]),
                        );
                        if ui.button(app.i18n.tr("connect_ai.stop_task")).clicked() {
                            *stop_task_requested = true;
                        }
                    }
                }
                None => {
                    ui.label(app.i18n.tr("connect_ai.no_session"));
                }
            }
            ui.small(app.i18n.tr("connect_ai.diagnostics_note"));
        });

    ui.add_space(8.0);
    let can_copy = state.prompt().is_some();
    if ui
        .add_enabled(
            can_copy,
            egui::Button::new(match state.mode() {
                McpPromptMode::PersistentSetup => app.i18n.tr("connect_ai.copy_setup"),
                McpPromptMode::OneTimeUse => app.i18n.tr("connect_ai.copy_one_time"),
            }),
        )
        .clicked()
    {
        state.copy_prompt();
    }
    match state.copy_status() {
        CopyStatus::Idle => {}
        CopyStatus::Copied => {
            ui.colored_label(theme::SUCCESS, app.i18n.tr("connect_ai.copied"));
        }
        CopyStatus::Failed(failure) => {
            ui.colored_label(theme::ERROR, copy_failure_text(&app.i18n, failure));
        }
    }

    if let Some(prompt) = state.prompt() {
        ui.add_space(8.0);
        ui.label(match state.mode() {
            McpPromptMode::PersistentSetup => app.i18n.tr("connect_ai.preview_setup"),
            McpPromptMode::OneTimeUse => app.i18n.tr("connect_ai.preview_one_time"),
        });
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ScrollArea::vertical()
                .max_height(240.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(prompt).monospace())
                            .selectable(true)
                            .wrap_mode(TextWrapMode::Wrap),
                    );
                });
        });
    }
}

fn copy_failure_text(i18n: &crate::modules::i18n::I18n, failure: &CopyFailure) -> String {
    match failure {
        CopyFailure::NoPrompt => i18n.tr("connect_ai.copy_failed_no_prompt").to_owned(),
        CopyFailure::ClipboardOpen(None) => i18n.tr("connect_ai.copy_failed_clipboard").to_owned(),
        CopyFailure::ClipboardOpen(Some(error)) => i18n.text(
            "connect_ai.copy_failed_clipboard_with_error",
            &[("error", error.clone())],
        ),
        CopyFailure::ClipboardWrite(error) => i18n.text(
            "connect_ai.copy_failed_clipboard_write",
            &[("error", error.clone())],
        ),
    }
}

fn selectable_path(ui: &mut three_d::egui::Ui, path: &str) {
    ui.add(
        egui::Label::new(RichText::new(path).monospace())
            .selectable(true)
            .wrap_mode(TextWrapMode::Wrap),
    );
}
