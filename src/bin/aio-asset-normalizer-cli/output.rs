//! Machine-readable CLI output contract.
//!
//! Every command except `docs` writes exactly one JSON envelope to stdout.
//! Diagnostics go to stderr through the shared logging pipeline.

use serde::Serialize;
use serde_json::Value;

use aio_asset_normalizer::modules::blender::bridge::BlenderError;
use aio_asset_normalizer::modules::build_info;
use aio_asset_normalizer::modules::bvh::BvhError;
use aio_asset_normalizer::modules::glb::GlbError;
use aio_asset_normalizer::modules::retarget::RetargetError;
use aio_asset_normalizer::modules::retarget_export::RetargetExportError;

/// Stable contract version for agent integrations.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Validation,
    Io,
    External,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Io => "io",
            Self::External => "external",
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            Self::Validation => 3,
            Self::Io => 4,
            Self::External => 5,
        }
    }
}

#[derive(Debug)]
pub struct CliError {
    pub code: ErrorCode,
    pub message: String,
}

impl CliError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Validation,
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Io,
            message: message.into(),
        }
    }

    pub fn external(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::External,
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
struct VersionInfo {
    app: &'static str,
    version: &'static str,
    commit: &'static str,
}

#[derive(Serialize)]
struct Failure {
    code: &'static str,
    message: String,
}

#[derive(Serialize)]
struct Envelope {
    schema_version: u32,
    ok: bool,
    command: &'static str,
    version: VersionInfo,
    results: Value,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Failure>,
}

pub fn emit_success(
    command: &'static str,
    results: Value,
    warnings: Vec<String>,
) -> i32 {
    write(&Envelope {
        schema_version: SCHEMA_VERSION,
        ok: true,
        command,
        version: version_info(),
        results,
        warnings,
        error: None,
    });
    0
}

/// Emit a failure envelope and return the matching process exit code.
pub fn emit_failure(
    command: &'static str,
    results: Value,
    error: &CliError,
) -> i32 {
    write(&Envelope {
        schema_version: SCHEMA_VERSION,
        ok: false,
        command,
        version: version_info(),
        results,
        warnings: Vec::new(),
        error: Some(Failure {
            code: error.code.as_str(),
            message: error.message.clone(),
        }),
    });
    error.code.exit_code()
}

fn version_info() -> VersionInfo {
    VersionInfo {
        app: build_info::APP_VERSION,
        version: build_info::APP_VERSION,
        commit: build_info::GIT_COMMIT,
    }
}

fn write(envelope: &Envelope) {
    match serde_json::to_string_pretty(envelope) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("failed to serialize CLI output: {error}"),
    }
}

pub fn glb_error(error: GlbError) -> CliError {
    match error {
        GlbError::Io(error) => CliError::io(error.to_string()),
        GlbError::Invalid(message) | GlbError::Unsupported(message) => {
            CliError::validation(message)
        }
    }
}

pub fn bvh_error(error: BvhError) -> CliError {
    match error {
        BvhError::Io(error) => CliError::io(error.to_string()),
        BvhError::Parse(message) | BvhError::Mapping(message) => {
            CliError::validation(message)
        }
    }
}

pub fn retarget_error(error: RetargetError) -> CliError {
    match error {
        RetargetError::Io(error) => CliError::io(error.to_string()),
        RetargetError::Mapping(message)
        | RetargetError::Source(message)
        | RetargetError::Target(message)
        | RetargetError::Unsupported(message) => {
            CliError::validation(format!("retarget: {message}"))
        }
    }
}

pub fn retarget_export_error(error: RetargetExportError) -> CliError {
    match error {
        RetargetExportError::Retarget(error) => retarget_error(error),
        RetargetExportError::Glb(error) => glb_error(error),
        RetargetExportError::Bvh(error) => bvh_error(error),
    }
}

pub fn blender_error(error: BlenderError) -> CliError {
    match error {
        BlenderError::InputMissing(_) | BlenderError::ScriptWrite(_) => {
            CliError::validation(error.to_string())
        }
        BlenderError::BlenderNotFound
        | BlenderError::Spawn(_)
        | BlenderError::ExitCode(_)
        | BlenderError::OutputMissing => CliError::external(error.to_string()),
    }
}