//! Authenticated loopback bridge shared by the GUI and the local MCP process.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender, SyncSender},
        Arc,
    },
    thread,
    time::Duration,
};
use uuid::Uuid;

pub const BRIDGE_PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const MANIFEST_FILE: &str = "session.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionPage {
    GlbEditor,
    BvhStudio,
    FbxConverter,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CameraView {
    pub target: [f32; 3],
    pub radius: f32,
    pub theta: f32,
    pub phi: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureScope {
    Viewport,
    Window,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointAction {
    Create,
    Restore,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation", content = "arguments", rename_all = "snake_case")]
pub enum SessionCommand {
    GetState,
    OpenGlb {
        path: PathBuf,
        discard_changes: bool,
        expected_revision: u64,
    },
    OpenBvh {
        path: PathBuf,
        discard_changes: bool,
        expected_revision: u64,
    },
    SetGlbEdits {
        edits: crate::modules::operations::spec::EditSpec,
        expected_revision: u64,
    },
    PreflightGlb {
        output: PathBuf,
        overwrite: bool,
        selection: Option<crate::modules::operations::spec::SelectionSpec>,
        expected_revision: u64,
    },
    ExportGlb {
        output: PathBuf,
        overwrite: bool,
        selection: Option<crate::modules::operations::spec::SelectionSpec>,
        expected_revision: u64,
    },
    ConfigureBvh {
        up_axis: String,
        forward_axis: String,
        unit: String,
        trim: Option<[f32; 2]>,
        reduce_keys: Option<f32>,
        expected_revision: u64,
    },
    ExportBvh {
        output: PathBuf,
        overwrite: bool,
        expected_revision: u64,
    },
    OpenRetarget {
        job: crate::modules::operations::retarget::RetargetJob,
        discard_changes: bool,
        expected_revision: u64,
    },
    SetRetargetMapping {
        mapping: crate::modules::retarget::SkeletonMapping,
        expected_revision: u64,
    },
    RetargetAction {
        action: RetargetSessionAction,
        output: Option<PathBuf>,
        overwrite: bool,
        expected_revision: u64,
    },
    ConfigureBatch {
        request: AutomationBatchRequest,
        expected_revision: u64,
    },
    RunBatch {
        export: bool,
        expected_revision: u64,
    },
    ConfigureConverter {
        inputs: Vec<PathBuf>,
        output_root: PathBuf,
        blender: Option<PathBuf>,
        overwrite: bool,
        expected_revision: u64,
    },
    RunConverter {
        expected_revision: u64,
    },
    PreflightConverter {
        expected_revision: u64,
    },
    SetView {
        page: Option<SessionPage>,
        reset_camera: bool,
        camera: Option<CameraView>,
        show_grid: Option<bool>,
        show_axes: Option<bool>,
        show_origin: Option<bool>,
        show_source_skeleton: Option<bool>,
        show_target_skeleton: Option<bool>,
        show_glb_skeleton: Option<bool>,
        animation_index: Option<usize>,
        animation_time: Option<f32>,
        bvh_frame: Option<usize>,
        playing: Option<bool>,
        expected_revision: u64,
    },
    Capture {
        scope: CaptureScope,
    },
    Checkpoint {
        action: CheckpointAction,
        expected_revision: u64,
    },
    JobGet {
        job_id: String,
    },
    JobCancel {
        job_id: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetargetSessionAction {
    Prompt,
    Suggest,
    Validate,
    Preview,
    ExitPreview,
    Export,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomationBatchRequest {
    pub input_root: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub output_root: PathBuf,
    #[serde(default)]
    pub recipe: Option<crate::modules::operations::spec::RecipeSpec>,
    #[serde(default)]
    pub selection: Option<crate::modules::operations::spec::SelectionSpec>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub session_id: String,
    pub revision: u64,
    pub page: SessionPage,
    pub glb_path: Option<PathBuf>,
    pub bvh_path: Option<PathBuf>,
    pub retarget_source: Option<PathBuf>,
    pub retarget_target: Option<PathBuf>,
    pub converter_inputs: Vec<PathBuf>,
    pub glb_dirty: bool,
    pub bvh_dirty: bool,
    pub asset_busy: bool,
    pub glb_animation_index: usize,
    pub glb_animation_time: f32,
    pub glb_animation_playing: bool,
    pub bvh_frame: usize,
    pub bvh_playing: bool,
    pub retarget_preview_active: bool,
    pub has_checkpoint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<BridgeError>,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub state: Option<SessionState>,
    #[serde(default)]
    pub png_base64: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeError {
    pub kind: String,
    pub message: String,
}

impl BridgeResponse {
    pub fn success(state: SessionState) -> Self {
        Self {
            ok: true,
            error: None,
            data: None,
            state: Some(state),
            png_base64: None,
            warnings: Vec::new(),
        }
    }

    pub fn failure(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(BridgeError {
                kind: kind.into(),
                message: message.into(),
            }),
            data: None,
            state: None,
            png_base64: None,
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManifest {
    pub session_id: String,
    pub address: SocketAddr,
    pub token: String,
    pub protocol_version: u32,
    pub app_version: String,
    pub process_id: u32,
    pub started_unix_ms: u128,
}

#[derive(Serialize, Deserialize)]
struct WireRequest {
    session_id: String,
    token: String,
    protocol_version: u32,
    app_version: String,
    command: SessionCommand,
}

#[derive(Serialize, Deserialize)]
struct WireResponse {
    session_id: String,
    protocol_version: u32,
    app_version: String,
    response: BridgeResponse,
}

pub struct GuiRequest {
    pub command: SessionCommand,
    pub reply: SyncSender<BridgeResponse>,
}

pub struct BridgeServer {
    manifest_path: PathBuf,
    stopping: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    pub manifest: SessionManifest,
}

impl BridgeServer {
    pub fn start() -> Result<(Self, Receiver<GuiRequest>), String> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let session_id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().to_string() + &Uuid::new_v4().simple().to_string();
        let manifest = SessionManifest {
            session_id: session_id.clone(),
            address,
            token,
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            app_version: crate::modules::build_info::APP_VERSION.to_owned(),
            process_id: std::process::id(),
            started_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_millis(),
        };
        let manifest_path = session_directory()?.join(format!("{session_id}-{MANIFEST_FILE}"));
        write_manifest(&manifest_path, &manifest)?;
        let (sender, receiver) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let server_stop = stopping.clone();
        let server_manifest = manifest.clone();
        let thread = thread::Builder::new()
            .name("automation-bridge".to_owned())
            .spawn(move || {
                while !server_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let manifest = server_manifest.clone();
                            let sender = sender.clone();
                            let _ = thread::Builder::new()
                                .name("automation-request".to_owned())
                                .spawn(move || handle_connection(stream, &manifest, sender));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(30));
                        }
                        Err(error) => {
                            tracing::warn!(target: "automation_bridge", error = %error, "Bridge listener stopped accepting connections");
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            })
            .map_err(|error| {
                let _ = fs::remove_file(&manifest_path);
                error.to_string()
            })?;
        Ok((
            Self {
                manifest_path,
                stopping,
                thread: Some(thread),
                manifest,
            },
            receiver,
        ))
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Err(error) = fs::remove_file(&self.manifest_path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(target: "automation_bridge", error = %error, "Could not remove the session manifest");
            }
        }
    }
}

pub fn list_sessions() -> Result<Vec<SessionManifest>, String> {
    let directory = session_directory()?;
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut sessions = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if entry.path().extension().is_none_or(|value| value != "json") {
            continue;
        }
        let text = match fs::read_to_string(entry.path()) {
            Ok(text) => text,
            Err(_) => continue,
        };
        if let Ok(manifest) = serde_json::from_str::<SessionManifest>(&text) {
            sessions.push(manifest);
        }
    }
    sessions.sort_by_key(|manifest| manifest.started_unix_ms);
    Ok(sessions)
}

pub fn call(manifest: &SessionManifest, command: SessionCommand) -> Result<BridgeResponse, String> {
    if manifest.protocol_version != BRIDGE_PROTOCOL_VERSION {
        return Err(format!(
            "Bridge protocol mismatch: session uses {}, client uses {}",
            manifest.protocol_version, BRIDGE_PROTOCOL_VERSION
        ));
    }
    let mut stream = TcpStream::connect_timeout(&manifest.address, Duration::from_secs(2))
        .map_err(|error| format!("GUI session is unreachable: {error}"))?;
    stream
        .set_read_timeout(Some(REQUEST_TIMEOUT))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    let request = WireRequest {
        session_id: manifest.session_id.clone(),
        token: manifest.token.clone(),
        protocol_version: BRIDGE_PROTOCOL_VERSION,
        app_version: crate::modules::build_info::APP_VERSION.to_owned(),
        command,
    };
    write_frame(
        &mut stream,
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
    )?;
    let response = read_frame(&mut stream)?;
    let response: WireResponse =
        serde_json::from_slice(&response).map_err(|error| error.to_string())?;
    if response.session_id != manifest.session_id {
        return Err("GUI returned a response for a different session".to_owned());
    }
    if response.protocol_version != BRIDGE_PROTOCOL_VERSION
        || response.app_version != crate::modules::build_info::APP_VERSION
    {
        return Err("GUI session uses an incompatible bridge or application version".to_owned());
    }
    Ok(response.response)
}

fn handle_connection(
    mut stream: TcpStream,
    manifest: &SessionManifest,
    sender: Sender<GuiRequest>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let response = (|| -> Result<BridgeResponse, String> {
        let bytes = read_frame(&mut stream)?;
        let request: WireRequest =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if request.session_id != manifest.session_id {
            return Ok(BridgeResponse::failure(
                "session_not_found",
                "Session id does not match this GUI",
            ));
        }
        if request.token != manifest.token {
            return Ok(BridgeResponse::failure(
                "unauthorized",
                "Session authentication failed",
            ));
        }
        if request.protocol_version != manifest.protocol_version {
            return Ok(BridgeResponse::failure(
                "protocol_mismatch",
                "Bridge protocol version is incompatible",
            ));
        }
        if request.app_version != manifest.app_version {
            return Ok(BridgeResponse::failure(
                "version_mismatch",
                "GUI and MCP application versions differ",
            ));
        }
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        sender
            .send(GuiRequest {
                command: request.command,
                reply: reply_sender,
            })
            .map_err(|_| "GUI is shutting down".to_owned())?;
        reply_receiver
            .recv_timeout(REQUEST_TIMEOUT)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => {
                    "GUI request timed out; do not replay modifying requests automatically"
                        .to_owned()
                }
                mpsc::RecvTimeoutError::Disconnected => "GUI stopped before replying".to_owned(),
            })
    })()
    .unwrap_or_else(|message| BridgeResponse::failure("bridge_error", message));
    let wire = WireResponse {
        session_id: manifest.session_id.clone(),
        protocol_version: manifest.protocol_version,
        app_version: manifest.app_version.clone(),
        response,
    };
    if let Ok(bytes) = serde_json::to_vec(&wire) {
        let _ = write_frame(&mut stream, &bytes);
    }
}

fn write_frame(stream: &mut TcpStream, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_FRAME_SIZE {
        return Err("Bridge message exceeds the maximum frame size".to_owned());
    }
    let size = u32::try_from(bytes.len()).map_err(|_| "Bridge frame is too large".to_owned())?;
    stream
        .write_all(&size.to_be_bytes())
        .map_err(|error| error.to_string())?;
    stream.write_all(bytes).map_err(|error| error.to_string())
}

fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut size_bytes = [0_u8; 4];
    stream
        .read_exact(&mut size_bytes)
        .map_err(|error| error.to_string())?;
    let size = u32::from_be_bytes(size_bytes) as usize;
    if size > MAX_FRAME_SIZE {
        return Err("Bridge message exceeds the maximum frame size".to_owned());
    }
    let mut bytes = vec![0; size];
    stream
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn session_directory() -> Result<PathBuf, String> {
    let root = dirs::data_local_dir()
        .ok_or_else(|| "Could not locate the user data directory".to_owned())?;
    let directory = root.join("AIO Asset Normalizer").join("sessions");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    restrict_directory(&directory)?;
    Ok(directory)
}

fn write_manifest(path: &Path, manifest: &SessionManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec(manifest).map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    restrict_file(&mut options);
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())
}
#[cfg(not(unix))]
fn restrict_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}
#[cfg(not(unix))]
fn restrict_file(_options: &mut OpenOptions) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_commands_have_explicit_operation_names_and_arguments() {
        let command = SessionCommand::SetView {
            page: Some(SessionPage::GlbEditor),
            reset_camera: false,
            camera: None,
            show_grid: None,
            show_axes: None,
            show_origin: None,
            show_source_skeleton: None,
            show_target_skeleton: None,
            show_glb_skeleton: Some(true),
            animation_index: Some(2),
            animation_time: Some(1.25),
            bvh_frame: None,
            playing: Some(false),
            expected_revision: 8,
        };
        let value = serde_json::to_value(command).expect("command should serialize");
        assert_eq!(value["operation"], "set_view");
        assert_eq!(value["arguments"]["expected_revision"], 8);
        assert_eq!(value["arguments"]["animation_index"], 2);
    }
}
