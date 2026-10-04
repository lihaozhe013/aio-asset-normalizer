//! Checked conversion through the shared fixed Blender profile.
use super::{check_output, OperationError};
use crate::modules::{
    atomic_file,
    blender::{
        bridge,
        task::{default_config_json, ConversionTask},
    },
    glb::{GlbDocument, GlbSummary},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversionRequest {
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default)]
    pub blender: Option<PathBuf>,
    #[serde(default)]
    pub overwrite: bool,
}
/// Check batch destinations before either front end starts external work.
pub fn validate_batch_outputs(
    requests: &[ConversionRequest],
) -> Result<(), OperationError> {
    let sources: Vec<_> = requests.iter().map(|r| r.input.as_path()).collect();
    for (index, request) in requests.iter().enumerate() {
        check_output(&request.output, &sources, request.overwrite)?;
        if requests[..index]
            .iter()
            .any(|other| super::same_path(&other.output, &request.output))
        {
            return Err(OperationError::validation(
                "Conversion outputs collide within the batch",
            ));
        }
    }
    Ok(())
}
pub fn preflight(request: &ConversionRequest) -> Result<(), OperationError> {
    if !request.input.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Conversion input is not a readable file",
        )
        .into());
    }
    let extension = request
        .input
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["fbx", "obj", "blend"].contains(&extension.as_str()) {
        return Err(OperationError::validation(
            "Converter inputs must be FBX, OBJ, or Blend",
        ));
    }
    if request
        .output
        .extension()
        .and_then(|v| v.to_str())
        .is_none_or(|v| !v.eq_ignore_ascii_case("glb"))
    {
        return Err(OperationError::validation(
            "Converter output must be .glb",
        ));
    }
    check_output(&request.output, &[&request.input], request.overwrite)?;
    std::fs::File::open(&request.input)?;
    if bridge::find_blender(
        request
            .blender
            .as_ref()
            .map(|p| p.to_string_lossy())
            .as_deref(),
    )
    .is_none()
    {
        return Err(OperationError::external(
            "Blender executable not found; install Blender or pass its path",
        ));
    }
    Ok(())
}
pub fn execute(
    request: &ConversionRequest,
    task_id: u64,
) -> Result<GlbSummary, OperationError> {
    preflight(request)?;
    let staged = atomic_file::stage(&request.output)?;
    let task = ConversionTask {
        task_id,
        input: request.input.clone(),
        output: staged.path().to_path_buf(),
        config_json: default_config_json(),
        blender_path: request
            .blender
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned()),
    };
    bridge::run_task(&task)
        .map_err(|e| OperationError::external(e.to_string()))?;
    let summary =
        validate_and_commit(staged, &request.output, request.overwrite)?;
    Ok(summary)
}
pub fn validate_and_commit(
    staged: tempfile::NamedTempFile,
    output: &Path,
    overwrite: bool,
) -> Result<GlbSummary, OperationError> {
    let document = GlbDocument::load(staged.path())?;
    let summary = document.summary();
    atomic_file::commit(staged, output, overwrite)?;
    Ok(summary)
}
