use crate::app::App;
use crate::modules::mcp_setup::{self, McpLaunchSpec, McpPromptMode, McpSetupError};

/// State for the Connect AI page.
///
/// The state lives on [`App`] for the whole session so the selected prompt
/// mode and copy feedback survive page switches; only the launch detection
/// refreshes when the page is entered.
#[derive(Default)]
pub(crate) struct McpSetupState {
    mode: McpPromptMode,
    launch: Option<Result<McpLaunchSpec, McpSetupError>>,
    prompt: Option<String>,
    prompt_error: Option<String>,
    copy_status: CopyStatus,
    clipboard: Option<arboard::Clipboard>,
}

/// Outcome of the last copy attempt. Failures stay structured so the panel
/// can translate the message with the active locale.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) enum CopyStatus {
    #[default]
    Idle,
    Copied,
    Failed(CopyFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CopyFailure {
    /// No prompt is available because the MCP launch spec is unusable.
    NoPrompt,
    /// The system clipboard could not be opened; `None` when the OS gave no
    /// error detail.
    ClipboardOpen(Option<String>),
    /// The prompt could not be written to the clipboard.
    ClipboardWrite(String),
}

impl McpSetupState {
    pub(crate) fn mode(&self) -> McpPromptMode {
        self.mode
    }

    /// Switch the prompt mode and rebuild the prompt. Switching to the
    /// current mode keeps the prompt and copy status untouched.
    pub(crate) fn set_mode(&mut self, mode: McpPromptMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.refresh_prompt();
    }

    pub(crate) fn launch(&self) -> Option<&Result<McpLaunchSpec, McpSetupError>> {
        self.launch.as_ref()
    }

    pub(crate) fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    pub(crate) fn prompt_error(&self) -> Option<&str> {
        self.prompt_error.as_deref()
    }

    pub(crate) fn copy_status(&self) -> &CopyStatus {
        &self.copy_status
    }

    /// Re-detect the MCP launch spec and rebuild the prompt from scratch.
    pub(crate) fn refresh(&mut self) {
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

    pub(crate) fn copy_prompt(&mut self) {
        let Some(prompt) = self.prompt.clone() else {
            self.copy_status = CopyStatus::Failed(CopyFailure::NoPrompt);
            return;
        };
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(clipboard) => self.clipboard = Some(clipboard),
                Err(error) => {
                    self.copy_status =
                        CopyStatus::Failed(CopyFailure::ClipboardOpen(Some(error.to_string())));
                    return;
                }
            }
        }
        let Some(clipboard) = self.clipboard.as_mut() else {
            self.copy_status = CopyStatus::Failed(CopyFailure::ClipboardOpen(None));
            return;
        };
        self.copy_status = match clipboard.set_text(prompt) {
            Ok(()) => CopyStatus::Copied,
            Err(error) => CopyStatus::Failed(CopyFailure::ClipboardWrite(error.to_string())),
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
    /// Refresh the MCP detection when the Connect AI page becomes active,
    /// mirroring the previous refresh-per-dialog-open behavior.
    pub(crate) fn enter_connect_ai_page(&mut self) {
        self.mcp_setup.refresh();
    }
}
