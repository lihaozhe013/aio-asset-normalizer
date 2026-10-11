//! Local stdio MCP entry point for shared asset operations.

mod files;

use aio_asset_normalizer::modules::automation::{
    self, CaptureScope, CheckpointAction, SessionCommand,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone)]
struct McpServer {
    tool_router: ToolRouter<Self>,
}

impl McpServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                "aio-asset-normalizer",
                env!("CARGO_PKG_VERSION"),
            ))
    }
}

#[tool_router(router = tool_router)]
impl McpServer {
    #[tool(
        name = "glb_inspect",
        description = "Read a GLB summary, export catalog, and Skin hierarchy from an absolute file path. This tool does not start the GUI or modify files."
    )]
    async fn glb_inspect(&self, request: Parameters<files::GlbInspectRequest>) -> CallToolResult {
        files::glb_inspect(request.0)
    }

    #[tool(
        name = "glb_edit",
        description = "Apply a typed GLB edit job and export its result to an absolute output path. This tool writes files and does not start the GUI."
    )]
    async fn glb_edit(&self, request: Parameters<files::GlbEditRequest>) -> CallToolResult {
        files::glb_edit(request.0)
    }

    #[tool(
        name = "bvh_inspect",
        description = "Read a BVH skeleton and motion summary from an absolute file path. This tool does not start the GUI or modify files."
    )]
    async fn bvh_inspect(&self, request: Parameters<files::BvhInspectRequest>) -> CallToolResult {
        files::bvh_inspect(request.0)
    }

    #[tool(
        name = "bvh_process",
        description = "Trim and validate a BVH file, then write it to an absolute output path. This tool writes files and does not start the GUI."
    )]
    async fn bvh_process(
        &self,
        request: Parameters<aio_asset_normalizer::modules::operations::bvh::BvhProcessRequest>,
    ) -> CallToolResult {
        files::bvh_process(request.0)
    }

    #[tool(
        name = "retarget",
        description = "Build a prompt, validate a typed Mapping v2 contract, suggest BVH name matches, or retarget a BVH/GLB source to a GLB target. Mapping suggestions never replace explicit mappings. File outputs use absolute paths; this tool does not start the GUI."
    )]
    async fn retarget(&self, request: Parameters<files::RetargetRequest>) -> CallToolResult {
        files::retarget(request.0)
    }

    #[tool(
        name = "glb_batch",
        description = "Preflight or export a typed GLB batch recipe. Paths must be absolute. Export writes files and does not start the GUI."
    )]
    async fn glb_batch(&self, request: Parameters<files::GlbBatchRequest>) -> CallToolResult {
        files::glb_batch(request.0)
    }

    #[tool(
        name = "fbx_convert",
        description = "Convert one FBX, OBJ, or Blend source to GLB using the shared Blender conversion profile. Blender is required only for this tool. Paths must be absolute; this tool writes files and does not start the GUI."
    )]
    async fn fbx_convert(
        &self,
        request: Parameters<
            aio_asset_normalizer::modules::operations::converter::ConversionRequest,
        >,
    ) -> CallToolResult {
        files::fbx_convert(request.0)
    }

    #[tool(
        name = "session_list",
        description = "Discover reachable GUI sessions. This read-only tool never starts the GUI."
    )]
    async fn session_list(&self, _request: Parameters<NoArguments>) -> CallToolResult {
        session_list()
    }

    #[tool(
        name = "session_open",
        description = "Connect to a compatible GUI session, or start the installed GUI when none is running. If multiple sessions exist, pass a session_id from session_list."
    )]
    async fn session_open(&self, request: Parameters<SessionOpenRequest>) -> CallToolResult {
        session_open(request.0)
    }

    #[tool(
        name = "session_get_state",
        description = "Read the current page, assets, dirty state, revision, playback, and task state for an explicit GUI session. This tool never starts the GUI."
    )]
    async fn session_get_state(&self, request: Parameters<SessionIdRequest>) -> CallToolResult {
        call_session(&request.0.session_id, SessionCommand::GetState)
    }

    #[tool(
        name = "session_glb",
        description = "Open or edit the in-memory GLB Editor session, preflight its current edited snapshot, or export that snapshot. Mutations require expected_revision and never open file dialogs."
    )]
    async fn session_glb(&self, request: Parameters<SessionCommandRequest>) -> CallToolResult {
        dispatch_session_command(
            request.0,
            &["open_glb", "set_glb_edits", "preflight_glb", "export_glb"],
        )
    }

    #[tool(
        name = "session_bvh",
        description = "Open or configure BVH Studio state and export the current BVH. Mutations require expected_revision and never open file dialogs."
    )]
    async fn session_bvh(&self, request: Parameters<SessionCommandRequest>) -> CallToolResult {
        dispatch_session_command(request.0, &["open_bvh", "configure_bvh", "export_bvh"])
    }

    #[tool(
        name = "session_retarget",
        description = "Configure source, target, and Mapping v2 in the GUI session; validate, preview, exit preview, or export retarget results. Mutations require expected_revision."
    )]
    async fn session_retarget(&self, request: Parameters<SessionCommandRequest>) -> CallToolResult {
        dispatch_session_command(
            request.0,
            &["open_retarget", "set_retarget_mapping", "retarget_action"],
        )
    }

    #[tool(
        name = "session_batch",
        description = "Configure the GUI GLB batch page, preflight or run it, and read per-file results. Mutations require expected_revision."
    )]
    async fn session_batch(&self, request: Parameters<SessionCommandRequest>) -> CallToolResult {
        dispatch_session_command(request.0, &["configure_batch", "run_batch"])
    }

    #[tool(
        name = "session_converter",
        description = "Configure, preflight, and run the GUI FBX Converter page. Blender is required for a successful conversion. Mutations require expected_revision."
    )]
    async fn session_converter(
        &self,
        request: Parameters<SessionCommandRequest>,
    ) -> CallToolResult {
        dispatch_session_command(
            request.0,
            &[
                "configure_converter",
                "preflight_converter",
                "run_converter",
            ],
        )
    }

    #[tool(
        name = "session_view",
        description = "Change the GUI page, camera, overlays, GLB animation selection or time, BVH frame, or playback. Explicit view changes require expected_revision."
    )]
    async fn session_view(&self, request: Parameters<SessionCommandRequest>) -> CallToolResult {
        dispatch_session_command(request.0, &["set_view"])
    }

    #[tool(
        name = "session_capture",
        description = "Capture a fresh PNG of the current viewport or full GUI window after the current render frame. Returns the rendered revision and frame metadata."
    )]
    async fn session_capture(&self, request: Parameters<SessionCaptureRequest>) -> CallToolResult {
        call_session(
            &request.0.session_id,
            SessionCommand::Capture {
                scope: request.0.scope,
            },
        )
    }

    #[tool(
        name = "session_checkpoint",
        description = "Create, restore, or delete the session's one explicit in-memory checkpoint. Restore pauses playback and rebuilds the preview; written files are unchanged."
    )]
    async fn session_checkpoint(
        &self,
        request: Parameters<SessionCheckpointRequest>,
    ) -> CallToolResult {
        call_session(
            &request.0.session_id,
            SessionCommand::Checkpoint {
                action: request.0.action,
                expected_revision: request.0.expected_revision,
            },
        )
    }

    #[tool(
        name = "job_get",
        description = "Read the progress and result for a file-oriented MCP job or a GUI session job. Pass session_id for a GUI job; omit it for a file job."
    )]
    async fn job_get(&self, request: Parameters<SessionJobRequest>) -> CallToolResult {
        match request.0.session_id {
            Some(session_id) => call_session(
                &session_id,
                SessionCommand::JobGet {
                    job_id: request.0.job_id,
                },
            ),
            None => files::file_job_get(&request.0.job_id),
        }
    }

    #[tool(
        name = "job_cancel",
        description = "Request cooperative cancellation for a file-oriented MCP job or GUI session job. Pass session_id for a GUI job; omit it for a file job. Completed atomic outputs are retained and reported."
    )]
    async fn job_cancel(&self, request: Parameters<SessionJobRequest>) -> CallToolResult {
        match request.0.session_id {
            Some(session_id) => call_session(
                &session_id,
                SessionCommand::JobCancel {
                    job_id: request.0.job_id,
                },
            ),
            None => files::file_job_cancel(&request.0.job_id),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoArguments {}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SessionOpenRequest {
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SessionIdRequest {
    session_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SessionCommandRequest {
    session_id: String,
    command: SessionCommand,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SessionCaptureRequest {
    session_id: String,
    scope: CaptureScope,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SessionCheckpointRequest {
    session_id: String,
    action: CheckpointAction,
    expected_revision: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SessionJobRequest {
    #[serde(default)]
    session_id: Option<String>,
    job_id: String,
}

fn session_list() -> CallToolResult {
    let sessions = match automation::list_sessions() {
        Ok(sessions) => sessions,
        Err(error) => return session_error("io", error),
    };
    let mut reachable = Vec::new();
    for manifest in sessions {
        if let Ok(response) = automation::call(&manifest, SessionCommand::GetState) {
            if response.ok {
                reachable.push(json!({ "session_id":manifest.session_id,"app_version":manifest.app_version,"state":response.state }));
            }
        }
    }
    CallToolResult::structured(json!({"ok":true,"sessions":reachable}))
}

fn session_open(request: SessionOpenRequest) -> CallToolResult {
    let mut sessions = match reachable_sessions() {
        Ok(sessions) => sessions,
        Err(error) => return session_error("io", error),
    };
    if let Some(session_id) = request.session_id {
        let Some(manifest) = sessions
            .into_iter()
            .find(|manifest| manifest.session_id == session_id)
        else {
            return session_error(
                "session_not_found",
                format!("No reachable GUI session has id {session_id}"),
            );
        };
        return call_manifest(&manifest, SessionCommand::GetState);
    }
    match sessions.len() {
        1 => call_manifest(&sessions.remove(0), SessionCommand::GetState),
        count if count > 1 => CallToolResult::structured_error(json!({
            "ok":false,
            "error":{"kind":"multiple_sessions","message":"Specify one of these GUI sessions"},
            "sessions":sessions.iter().map(|manifest| json!({"session_id":manifest.session_id,"app_version":manifest.app_version})).collect::<Vec<_>>(),
        })),
        _ => match launch_gui() {
            Ok(()) => {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                while std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    sessions = match reachable_sessions() {
                        Ok(sessions) => sessions,
                        Err(error) => return session_error("io", error),
                    };
                    if sessions.len() == 1 {
                        return call_manifest(&sessions[0], SessionCommand::GetState);
                    }
                    if sessions.len() > 1 {
                        return session_open(SessionOpenRequest { session_id: None });
                    }
                }
                session_error(
                    "session_start_timeout",
                    "The GUI did not register a session within 15 seconds",
                )
            }
            Err(error) => session_error("session_start_failed", error),
        },
    }
}

fn reachable_sessions() -> Result<Vec<automation::SessionManifest>, String> {
    let mut reachable = Vec::new();
    for manifest in automation::list_sessions()? {
        if automation::call(&manifest, SessionCommand::GetState).is_ok_and(|response| response.ok) {
            reachable.push(manifest);
        }
    }
    Ok(reachable)
}

fn launch_gui() -> Result<(), String> {
    use std::{
        path::PathBuf,
        process::{Command, Stdio},
    };
    let mcp_path = std::env::current_exe().map_err(|error| error.to_string())?;
    let executable_name = format!("aio-asset-normalizer{}", std::env::consts::EXE_SUFFIX);
    let gui_path = mcp_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(executable_name);
    if !gui_path.is_file() {
        return Err(format!(
            "GUI executable is missing beside the MCP server: {}",
            gui_path.display()
        ));
    }
    let mut command = Command::new(PathBuf::from(gui_path));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn call_session(session_id: &str, command: SessionCommand) -> CallToolResult {
    let manifest = match automation::list_sessions() {
        Ok(sessions) => sessions
            .into_iter()
            .find(|candidate| candidate.session_id == session_id),
        Err(error) => return session_error("io", error),
    };
    let Some(manifest) = manifest else {
        return session_error(
            "session_not_found",
            format!("No GUI session has id {session_id}"),
        );
    };
    call_manifest(&manifest, command)
}

fn call_manifest(
    manifest: &automation::SessionManifest,
    command: SessionCommand,
) -> CallToolResult {
    match automation::call(manifest, command) {
        Ok(response) => {
            let value = json!({
                "ok":response.ok,
                "error":response.error,
                "data":response.data,
                "state":response.state,
                "warnings":response.warnings,
            });
            let mut result = if response.ok {
                CallToolResult::structured(value)
            } else {
                CallToolResult::structured_error(value)
            };
            if let Some(png) = response.png_base64 {
                result.content.push(ContentBlock::image(png, "image/png"));
            }
            result
        }
        Err(error) => session_error("bridge_error", error),
    }
}

fn dispatch_session_command(request: SessionCommandRequest, allowed: &[&str]) -> CallToolResult {
    let operation = serde_json::to_value(&request.command)
        .ok()
        .and_then(|value| {
            value
                .get("operation")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    let Some(operation) = operation else {
        return session_error(
            "validation",
            "Could not identify the typed session operation",
        );
    };
    if !allowed.contains(&operation.as_str()) {
        return session_error(
            "validation",
            format!("Operation {operation} is not available through this session tool"),
        );
    }
    call_session(&request.session_id, request.command)
}

fn session_error(kind: &str, message: impl Into<String>) -> CallToolResult {
    CallToolResult::structured_error(
        json!({"ok":false,"error":{"kind":kind,"message":message.into()}}),
    )
}

#[tokio::main]
async fn main() {
    let _logging = aio_asset_normalizer::modules::logging::LogRuntime::init_cli(Some("info"));
    let server = McpServer::new();
    match server.serve(rmcp::transport::stdio()).await {
        Ok(service) => {
            if let Err(error) = service.waiting().await {
                tracing::error!(target: "mcp", error = %error, "MCP service stopped with an error");
            }
        }
        Err(error) => {
            tracing::error!(target: "mcp", error = %error, "MCP service failed to start");
        }
    }
    files::shutdown_file_jobs();
}
