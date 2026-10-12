use crate::{
    app::App,
    modules::mcp_setup::{self, McpLaunchSpec, McpSetupError},
};
use three_d::egui;

#[derive(Default)]
pub(crate) struct McpSetupDialogState {
    opened: bool,
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
            Ok(spec) => match mcp_setup::build_setup_prompt(spec) {
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

    let state = &mut app.mcp_setup;
    let mut opened = state.opened;
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
                .show(ui, |ui| render_contents(state, ui));
        });
    state.opened = opened;
}

fn render_contents(state: &mut McpSetupDialogState, ui: &mut egui::Ui) {
    ui.label("Copy this prompt and paste it into your local AI agent.");
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

    ui.add_space(8.0);
    let can_copy = state.prompt.is_some();
    if ui
        .add_enabled(can_copy, egui::Button::new("Copy setup prompt"))
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
        ui.label("Setup prompt preview (select text to copy manually)");
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
