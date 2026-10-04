//! Immutable BVH processing and checked output submission.
use super::{check_output, OperationError};
use crate::modules::bvh::BvhDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BvhProcessRequest {
    #[serde(default)]
    pub command: Option<String>,
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default)]
    pub trim: Option<[f32; 2]>,
    #[serde(default)]
    pub overwrite: bool,
}
pub fn prepare(
    document: &BvhDocument,
    trim: Option<[f32; 2]>,
) -> Result<BvhDocument, OperationError> {
    let mut output = document.clone();
    if let Some([start, end]) = trim {
        output.trim(start, end)?;
    }
    Ok(output)
}
pub fn validate_output(
    document: &BvhDocument,
    output: &Path,
    input: &Path,
    overwrite: bool,
) -> Result<String, OperationError> {
    check_output(output, &[input], overwrite)?;
    let text = document.to_text()?;
    BvhDocument::parse(&text)?;
    Ok(text)
}

pub fn write(
    document: &BvhDocument,
    output: &Path,
    input: &Path,
    overwrite: bool,
) -> Result<(), OperationError> {
    let text = validate_output(document, output, input, overwrite)?;
    crate::modules::atomic_file::write(output, text.as_bytes(), overwrite)?;
    Ok(())
}
