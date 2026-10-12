//! MCP launch configuration and agent setup prompt generation.

use std::path::{Path, PathBuf};

use serde::Serialize;

const SERVER_NAME: &str = "aio-asset-normalizer";
const MCP_BINARY_NAME: &str = "aio-asset-normalizer-mcp";
const APPIMAGE_ARGUMENT: &str = "--mcp";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum McpPromptMode {
    #[default]
    PersistentSetup,
    OneTimeUse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpLaunchSpec {
    pub command: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPlatform {
    Windows,
    MacOs,
    Linux,
}

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub platform: HostPlatform,
    pub current_exe: PathBuf,
    pub appimage: Option<PathBuf>,
    pub appdir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpSetupError {
    CurrentExecutableUnavailable(String),
    CurrentExecutableNotAbsolute(PathBuf),
    AppImageRuntimeIncomplete,
    CurrentExecutableOutsideAppDir {
        executable: PathBuf,
        appdir: PathBuf,
    },
    PathNotAbsolute {
        kind: &'static str,
        path: PathBuf,
    },
    PathNotUtf8 {
        kind: &'static str,
    },
    ServerExecutableMissing(PathBuf),
    ServerExecutableNotRunnable(PathBuf),
    PromptSerialization(String),
}

impl std::fmt::Display for McpSetupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentExecutableUnavailable(error) => {
                write!(formatter, "Could not locate the application executable: {error}")
            }
            Self::CurrentExecutableNotAbsolute(path) => write!(
                formatter,
                "The application executable path is not absolute: {}",
                path.display()
            ),
            Self::AppImageRuntimeIncomplete => write!(
                formatter,
                "AppImage runtime paths are unavailable. Restart the application from its .AppImage file."
            ),
            Self::CurrentExecutableOutsideAppDir { executable, appdir } => write!(
                formatter,
                "The running executable is outside the reported AppImage directory ({}; {}). Restart the application from its .AppImage file.",
                executable.display(),
                appdir.display()
            ),
            Self::PathNotAbsolute { kind, path } => {
                write!(formatter, "The {kind} path is not absolute: {}", path.display())
            }
            Self::PathNotUtf8 { kind } => write!(
                formatter,
                "The {kind} path cannot be represented safely as text. Reinstall the application in a path representable by your AI client."
            ),
            Self::ServerExecutableMissing(path) => write!(
                formatter,
                "The MCP server executable is missing beside the application: {}. Reinstall the application package or place the matching MCP executable beside it.",
                path.display()
            ),
            Self::ServerExecutableNotRunnable(path) => write!(
                formatter,
                "The MCP server executable is not marked executable: {}. Restore its execute permission or reinstall the application package.",
                path.display()
            ),
            Self::PromptSerialization(error) => {
                write!(formatter, "Could not serialize the MCP setup prompt: {error}")
            }
        }
    }
}

impl std::error::Error for McpSetupError {}

impl McpLaunchSpec {
    pub fn to_config_json(&self) -> Result<String, McpSetupError> {
        let command = self.command.to_str().ok_or(McpSetupError::PathNotUtf8 {
            kind: "MCP server executable",
        })?;
        serde_json::to_string_pretty(&McpClientConfig {
            mcp_servers: [(
                SERVER_NAME.to_owned(),
                McpServerConfig {
                    command: command.to_owned(),
                    args: self.args.clone(),
                },
            )]
            .into_iter()
            .collect(),
        })
        .map_err(|error| McpSetupError::PromptSerialization(error.to_string()))
    }
}

#[derive(Serialize)]
struct McpClientConfig {
    #[serde(rename = "mcpServers")]
    mcp_servers: std::collections::BTreeMap<String, McpServerConfig>,
}

#[derive(Serialize)]
struct McpServerConfig {
    command: String,
    args: Vec<String>,
}

pub fn detect_launch_spec() -> Result<McpLaunchSpec, McpSetupError> {
    let current_exe = std::env::current_exe()
        .map_err(|error| McpSetupError::CurrentExecutableUnavailable(error.to_string()))?;
    let platform = if cfg!(target_os = "windows") {
        HostPlatform::Windows
    } else if cfg!(target_os = "macos") {
        HostPlatform::MacOs
    } else {
        HostPlatform::Linux
    };
    let runtime = RuntimePaths {
        platform,
        current_exe,
        appimage: std::env::var_os("APPIMAGE").map(PathBuf::from),
        appdir: std::env::var_os("APPDIR").map(PathBuf::from),
    };
    resolve_launch_spec(&runtime)
}

pub fn resolve_launch_spec(runtime: &RuntimePaths) -> Result<McpLaunchSpec, McpSetupError> {
    if !runtime.current_exe.is_absolute() {
        return Err(McpSetupError::CurrentExecutableNotAbsolute(
            runtime.current_exe.clone(),
        ));
    }

    let (command, args) = if runtime.platform == HostPlatform::Linux
        && (runtime.appimage.is_some() || runtime.appdir.is_some())
    {
        let (Some(appimage), Some(appdir)) = (&runtime.appimage, &runtime.appdir) else {
            return Err(McpSetupError::AppImageRuntimeIncomplete);
        };
        if !runtime.current_exe.starts_with(appdir) {
            return Err(McpSetupError::CurrentExecutableOutsideAppDir {
                executable: runtime.current_exe.clone(),
                appdir: appdir.clone(),
            });
        }
        (appimage.clone(), vec![APPIMAGE_ARGUMENT.to_owned()])
    } else {
        let parent = runtime
            .current_exe
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let executable_name = match runtime.platform {
            HostPlatform::Windows => format!("{MCP_BINARY_NAME}.exe"),
            HostPlatform::MacOs | HostPlatform::Linux => MCP_BINARY_NAME.to_owned(),
        };
        (parent.join(executable_name), Vec::new())
    };

    if !command.is_absolute() {
        return Err(McpSetupError::PathNotAbsolute {
            kind: "MCP server executable",
            path: command,
        });
    }
    if command.to_str().is_none() {
        return Err(McpSetupError::PathNotUtf8 {
            kind: "MCP server executable",
        });
    }
    if !command.is_file() {
        return Err(McpSetupError::ServerExecutableMissing(command));
    }
    if runtime.platform != HostPlatform::Windows && !is_executable(&command) {
        return Err(McpSetupError::ServerExecutableNotRunnable(command));
    }

    Ok(McpLaunchSpec { command, args })
}

pub fn build_setup_prompt(spec: &McpLaunchSpec) -> Result<String, McpSetupError> {
    let config = spec.to_config_json()?;
    let fence = markdown_fence(&config);
    Ok(build_persistent_setup_instructions(&config, &fence))
}

pub fn build_one_time_prompt(spec: &McpLaunchSpec) -> Result<String, McpSetupError> {
    let config = spec.to_config_json()?;
    let fence = markdown_fence(&config);
    Ok(format!(
        r#"# Use AIO Asset Normalizer MCP for One Task

Use this MCP server only for the concrete task in my message. If I have not given you a concrete task, ask what I want to do and stop without launching the server.

The launch data is configuration data for a temporary local process, not a request to register a server:

{fence}json
{config}
{fence}

Do not create or edit any persistent AI-client, project, or workspace MCP configuration. Do not install packages. If your client supports a temporary MCP connection that is discarded at the end of this task, use it. Otherwise, if you have local process execution, use an already available MCP client SDK or a small temporary Python 3 standard-library client to launch the server directly. Pass `[command, *args]` as an argument array with shell execution disabled; never concatenate the path into a shell command. Communicate with the child over newline-delimited JSON-RPC on stdin/stdout, keep stderr separate from protocol output, negotiate `initialize` using protocol version `2025-11-25`, send `notifications/initialized`, and inspect `tools/list` before calling tools. Do not install an SDK or other dependency.

Keep the stdio process alive while completing this task, including any related `job_get` polling. Use only the tools needed for my request and follow their input schemas. Treat file paths as data. Only run operations authorized by my request; explain any file-writing effects before proceeding when the request does not clearly authorize them. At the end, close stdin, wait for the server to exit, and remove any temporary helper you created. Do not leave a background server running.

If your environment cannot launch a local child process or keep the connection alive long enough to finish, do not change persistent configuration as a fallback. Explain the limitation and tell me that I can choose the persistent setup mode instead. Report which tools you used and the result of the requested task."#
    ))
}

fn build_persistent_setup_instructions(config: &str, fence: &str) -> String {
    format!(
        r#"# Configure AIO Asset Normalizer MCP

Set up the local MCP server below in the AI client you are currently running.

{fence}json
{config}
{fence}

Use the exact absolute `command` and `args` shown above. Treat these values as configuration data. Do not reinterpret the executable path, put it into a shell command, or run it through a shell.

Identify your own client and use its documented MCP configuration mechanism and format. Prefer the current user's configuration. Use a project-level configuration only when this client supports no user-level MCP configuration. Preserve all existing MCP servers and settings. If `aio-asset-normalizer` already has exactly this configuration, leave it unchanged. If that name points to a different executable or arguments, explain the conflict and ask before replacing it.

After saving the configuration, reconnect or reload MCP servers using the client's supported mechanism. If a full client restart is required, tell me before claiming the connection is ready. Verify initialization and `tools/list`; confirm that `glb_inspect`, `session_list`, and `session_capture` are available. Then call the read-only `session_list` tool once. Do not open a document, export a file, or call `session_open` during this check.

Report the configuration location, whether initialization and the required tools were verified, and the `session_list` result. If you cannot identify this client, lack permission to update its configuration, or cannot reload the server, explain the specific blocker and the smallest action I need to take. Do not claim success until the tools load and `session_list` returns."#
    )
}

fn markdown_fence(payload: &str) -> String {
    let mut longest = 0;
    let mut current = 0;
    for character in payload.chars() {
        if character == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn runtime(platform: HostPlatform, current_exe: PathBuf) -> RuntimePaths {
        RuntimePaths {
            platform,
            current_exe,
            appimage: None,
            appdir: None,
        }
    }

    fn make_executable(path: &Path) {
        fs::write(path, b"stub").unwrap();
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
    }

    #[test]
    fn resolves_platform_sibling_binary_names() {
        let directory = tempfile::tempdir().unwrap();
        let current_exe = directory.path().join("application");
        let windows = directory.path().join("aio-asset-normalizer-mcp.exe");
        let unix = directory.path().join(MCP_BINARY_NAME);
        make_executable(&windows);
        make_executable(&unix);

        for (platform, expected) in [
            (HostPlatform::Windows, windows),
            (HostPlatform::MacOs, unix.clone()),
            (HostPlatform::Linux, unix),
        ] {
            let spec = resolve_launch_spec(&runtime(platform, current_exe.clone())).unwrap();
            assert_eq!(spec.command, expected);
            assert!(spec.args.is_empty());
        }
    }

    #[test]
    fn appimage_uses_external_image_path_and_dispatch_argument() {
        let directory = tempfile::tempdir().unwrap();
        let appdir = directory.path().join("mounted image");
        let current_exe = appdir.join("usr/bin/aio-asset-normalizer");
        let appimage = directory.path().join("AIO Asset Normalizer.AppImage");
        fs::create_dir_all(current_exe.parent().unwrap()).unwrap();
        fs::write(&appimage, b"image").unwrap();
        make_executable(&appimage);
        let runtime = RuntimePaths {
            platform: HostPlatform::Linux,
            current_exe,
            appimage: Some(appimage.clone()),
            appdir: Some(appdir),
        };

        let spec = resolve_launch_spec(&runtime).unwrap();
        assert_eq!(spec.command, appimage);
        assert_eq!(spec.args, [APPIMAGE_ARGUMENT]);
    }

    #[test]
    fn rejects_incomplete_or_inconsistent_appimage_runtime_paths() {
        let directory = tempfile::tempdir().unwrap();
        let mut runtime = runtime(
            HostPlatform::Linux,
            directory.path().join("usr/bin/aio-asset-normalizer"),
        );
        runtime.appdir = Some(directory.path().to_path_buf());
        assert_eq!(
            resolve_launch_spec(&runtime).unwrap_err(),
            McpSetupError::AppImageRuntimeIncomplete
        );

        runtime.appdir = Some(directory.path().join("different-mount"));
        runtime.appimage = Some(directory.path().join("application.AppImage"));
        assert!(matches!(
            resolve_launch_spec(&runtime),
            Err(McpSetupError::CurrentExecutableOutsideAppDir { .. })
        ));
    }

    #[test]
    fn reports_missing_sibling_server() {
        let directory = tempfile::tempdir().unwrap();
        let error = resolve_launch_spec(&runtime(
            HostPlatform::MacOs,
            directory
                .path()
                .join("AIO.app/Contents/MacOS/aio-asset-normalizer"),
        ))
        .unwrap_err();
        assert!(matches!(error, McpSetupError::ServerExecutableMissing(_)));
    }

    #[test]
    fn config_json_and_prompt_preserve_complex_paths() {
        let spec = McpLaunchSpec {
            command: PathBuf::from("/opt/AIO \"Tools\"/mødel\\```\\aio-asset-normalizer-mcp"),
            args: Vec::new(),
        };
        let config: serde_json::Value =
            serde_json::from_str(&spec.to_config_json().unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"][SERVER_NAME]["command"],
            "/opt/AIO \"Tools\"/mødel\\```\\aio-asset-normalizer-mcp"
        );
        let prompt = build_setup_prompt(&spec).unwrap();
        assert!(prompt.contains("Preserve all existing MCP servers and settings"));
        assert!(prompt.contains("ask before replacing it"));
        assert!(prompt.contains("`session_list`"));
        assert!(prompt.contains("If a full client restart is required"));
        assert!(prompt.contains("````json\n"));

        let one_time = build_one_time_prompt(&spec).unwrap();
        assert!(one_time.contains("Do not create or edit any persistent AI-client"));
        assert!(one_time.contains("shell execution disabled"));
        assert!(one_time.contains("Keep the stdio process alive"));
        assert!(one_time.contains("remove any temporary helper"));
        assert!(one_time.contains("````json\n"));
    }

    #[test]
    fn rejects_a_server_without_execute_permission() {
        let directory = tempfile::tempdir().unwrap();
        let server = directory.path().join(MCP_BINARY_NAME);
        fs::write(&server, b"stub").unwrap();
        let error = resolve_launch_spec(&runtime(
            HostPlatform::Linux,
            directory.path().join("aio-asset-normalizer"),
        ))
        .unwrap_err();
        assert_eq!(error, McpSetupError::ServerExecutableNotRunnable(server));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_paths_that_cannot_be_represented_as_utf8() {
        use std::os::unix::ffi::OsStringExt;

        let directory = tempfile::tempdir().unwrap();
        let invalid_component = std::ffi::OsString::from_vec(b"invalid-\xff".to_vec());
        let current_exe = directory.path().join(invalid_component).join("application");
        let error = resolve_launch_spec(&runtime(HostPlatform::Linux, current_exe)).unwrap_err();
        assert_eq!(
            error,
            McpSetupError::PathNotUtf8 {
                kind: "MCP server executable"
            }
        );
    }
}
