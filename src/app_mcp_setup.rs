use crate::{
    app::App,
    modules::mcp_setup::{self, McpLaunchSpec, McpPromptMode, McpSetupError},
};
use three_d::egui;

#[derive(Default)]
pub(crate) struct McpSetupDialogState {
    opened: bool,
    mode: McpPromptMode,
    launch: Option<Result<McpLaunchSpec, McpSetupError>>,
    prompt: Option<String>,
    prompt_error: Option<String>,
    copy_status: CopyStatus,
    clipboard: Option<arboard::Clipboard>,
}

#[derive(Default)]
enum CopyStatus {
    #[default]
    Idle,
    Copied,
    Failed(String),
}

impl McpSetupDialogState {
    fn refresh(&mut self) {
        let launch = mcp_setup::detect_launch_spec();
        let (prompt, prompt_error) = match &launch {
            Ok(spec) => match build_prompt(self.mode, spec) {
                Ok(prompt) => (Some(prompt), None),
                Err(error) => (None, Some(error.to_string())),
            },
            Err(error) => (None, Some(error.to_string())),
        };
        self.prompt = prompt;
        self.prompt_error = prompt_error;
        self.launch = Some(launch);
        self.copy_status = CopyStatus::Idle;
    }

    fn refresh_prompt(&mut self) {
        let Some(Ok(spec)) = self.launch.as_ref() else {
            return;
        };
        match build_prompt(self.mode, spec) {
            Ok(prompt) => {
                self.prompt = Some(prompt);
                self.prompt_error = None;
            }
            Err(error) => {
                self.prompt = None;
                self.prompt_error = Some(error.to_string());
            }
        }
        self.copy_status = CopyStatus::Idle;
    }

    fn copy_prompt(&mut self) {
        let Some(prompt) = self.prompt.as_ref() else {
            self.copy_status = CopyStatus::Failed(
                "A valid MCP server path is required before this prompt can be copied.".to_owned(),
            );
            return;
        };
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(clipboard) => self.clipboard = Some(clipboard),
                Err(error) => {
                    self.copy_status = CopyStatus::Failed(format!(
                        "Could not access the system clipboard: {error}. Select the prompt below and copy it manually."
                    ));
                    return;
                }
            }
        }
        let Some(clipboard) = self.clipboard.as_mut() else {
            self.copy_status = CopyStatus::Failed(
                "Could not access the system clipboard. Select the prompt below and copy it manually."
                    .to_owned(),
            );
            return;
        };
        let result = clipboard.set_text(prompt.clone());
        self.copy_status = match result {
            Ok(()) => CopyStatus::Copied,
            Err(error) => CopyStatus::Failed(format!(
                "Could not copy to the system clipboard: {error}. Select the prompt below and copy it manually."
            )),
        };
    }
}

fn build_prompt(mode: McpPromptMode, spec: &McpLaunchSpec) -> Result<String, McpSetupError> {
    match mode {
        McpPromptMode::PersistentSetup => mcp_setup::build_setup_prompt(spec),
        McpPromptMode::OneTimeUse => mcp_setup::build_one_time_prompt(spec),
    }
}

impl App {
    pub(crate) fn open_mcp_setup(&mut self) {
        self.mcp_setup.refresh();
        self.mcp_setup.opened = true;
    }
}

pub(crate) fn render(app: &mut App, ctx: &egui::Context) {
    if !app.mcp_setup.opened {
        return;
    }

    let runtime_status = app.automation_status();
    let stop_task_requested = {
        let state = &mut app.mcp_setup;
        let mut opened = state.opened;
        let mut stop_task_requested = false;
        let available_width = (ctx.content_rect().width() - 24.0).clamp(240.0, 640.0);
        let available_height = (ctx.content_rect().height() - 96.0).max(160.0);
        egui::Window::new("Connect your AI agent")
            .open(&mut opened)
            .default_width(available_width.min(640.0))
            .min_width(available_width.min(360.0))
            .max_width(available_width)
            .resizable(true)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(available_height)
                    .show(ui, |ui| {
                        render_contents(
                            state,
                            runtime_status.as_ref(),
                            &mut stop_task_requested,
                            ui,
                        )
                    });
            });
        state.opened = opened;
        stop_task_requested
    };
    if stop_task_requested {
        app.cancel_active_automation_job();
    }
}

fn render_contents(
    state: &mut McpSetupDialogState,
    runtime_status: Option<&(String, Option<String>, bool)>,
    stop_task_requested: &mut bool,
    ui: &mut egui::Ui,
) {
    let mut selected_mode = state.mode;
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut selected_mode,
            McpPromptMode::PersistentSetup,
            "Configure agent",
        );
        ui.selectable_value(
            &mut selected_mode,
            McpPromptMode::OneTimeUse,
            "One-time use",
        );
    });
    if selected_mode != state.mode {
        state.mode = selected_mode;
        state.refresh_prompt();
    }
    ui.label(match state.mode {
        McpPromptMode::PersistentSetup => "Copy this prompt and paste it into your local AI agent.",
        McpPromptMode::OneTimeUse => {
            "Copy this prompt to use MCP for one task without saving client configuration."
        }
    });
    ui.add_space(8.0);

    let launch = state.launch.clone();
    match launch.as_ref() {
        Some(Ok(spec)) if state.prompt_error.is_none() => {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "Ready to configure");
                if ui.button("Refresh").clicked() {
                    state.refresh();
                }
            });
            ui.label("MCP server executable");
            selectable_path(ui, &spec.command.display().to_string());
        }
        Some(Ok(spec)) => {
            ui.colored_label(egui::Color32::LIGHT_RED, "Setup unavailable");
            selectable_path(ui, &spec.command.display().to_string());
            if let Some(error) = &state.prompt_error {
                ui.label(error);
            }
            if ui.button("Refresh").clicked() {
                state.refresh();
            }
        }
        Some(Err(error)) => {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::LIGHT_RED, "Setup unavailable");
                if ui.button("Refresh").clicked() {
                    state.refresh();
                }
            });
            ui.label(error.to_string());
        }
        None => {
            ui.label("MCP server path has not been checked.");
            if ui.button("Refresh").clicked() {
                state.refresh();
            }
        }
    }

    ui.add_space(8.0);
    egui::CollapsingHeader::new("Connection details")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Server name: aio-asset-normalizer");
            ui.label("Transport: stdio");
            if let Some(Ok(spec)) = state.launch.as_ref() {
                ui.label("Command");
                selectable_path(ui, &spec.command.display().to_string());
                ui.label(format!(
                    "Arguments: {}",
                    serde_json::to_string(&spec.args).unwrap_or_else(|_| "[]".to_owned())
                ));
            }
        });

    egui::CollapsingHeader::new("MCP diagnostics")
        .default_open(false)
        .show(ui, |ui| {
            match runtime_status {
                Some((session, task, recently_active)) => {
                    ui.label(format!("GUI session: {session}"));
                    ui.label(if *recently_active {
                        "Recent GUI request received"
                    } else {
                        "Waiting for a GUI session request"
                    });
                    if let Some(task) = task {
                        ui.label(format!("Active task: {task}"));
                        if ui.button("Stop MCP task").clicked() {
                            *stop_task_requested = true;
                        }
                    }
                }
                None => {
                    ui.label("No GUI session is currently registered.");
                }
            }
            ui.small(
                "This reports app-side GUI activity only. It does not confirm that your AI agent is configured.",
            );
        });

    ui.add_space(8.0);
    let can_copy = state.prompt.is_some();
    if ui
        .add_enabled(
            can_copy,
            egui::Button::new(match state.mode {
                McpPromptMode::PersistentSetup => "Copy setup prompt",
                McpPromptMode::OneTimeUse => "Copy one-time prompt",
            }),
        )
        .clicked()
    {
        state.copy_prompt();
    }
    match &state.copy_status {
        CopyStatus::Idle => {}
        CopyStatus::Copied => {
            ui.colored_label(egui::Color32::LIGHT_GREEN, "Copied");
        }
        CopyStatus::Failed(error) => {
            ui.colored_label(egui::Color32::LIGHT_RED, error);
        }
    }

    if let Some(prompt) = state.prompt.as_ref() {
        ui.add_space(8.0);
        ui.label(match state.mode {
            McpPromptMode::PersistentSetup => "Setup prompt preview (select text to copy manually)",
            McpPromptMode::OneTimeUse => "One-time prompt preview (select text to copy manually)",
        });
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(prompt).monospace())
                            .selectable(true)
                            .wrap_mode(egui::TextWrapMode::Wrap),
                    );
                });
        });
    }
}

fn selectable_path(ui: &mut egui::Ui, path: &str) {
    ui.add(
        egui::Label::new(egui::RichText::new(path).monospace())
            .selectable(true)
            .wrap_mode(egui::TextWrapMode::Wrap),
    );
}
