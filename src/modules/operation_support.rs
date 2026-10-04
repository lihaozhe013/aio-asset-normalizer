//! Shared error classification and filesystem identities for domain services.
use crate::modules::{bvh::BvhError, glb::GlbError, retarget::RetargetError};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorKind {
    Validation,
    Io,
    External,
}

#[derive(Debug, Clone)]
pub struct OperationError {
    pub kind: ErrorKind,
    pub message: String,
}
impl OperationError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Validation,
            message: message.into(),
        }
    }
    pub fn external(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::External,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for OperationError {}
impl From<std::io::Error> for OperationError {
    fn from(error: std::io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: error.to_string(),
        }
    }
}
impl From<GlbError> for OperationError {
    fn from(error: GlbError) -> Self {
        match error {
            GlbError::Io(e) => e.into(),
            other => Self::validation(other.to_string()),
        }
    }
}
impl From<BvhError> for OperationError {
    fn from(error: BvhError) -> Self {
        match error {
            BvhError::Io(e) => e.into(),
            other => Self::validation(other.to_string()),
        }
    }
}
impl From<RetargetError> for OperationError {
    fn from(error: RetargetError) -> Self {
        match error {
            RetargetError::Io(e) => e.into(),
            other => Self::validation(other.to_string()),
        }
    }
}
impl From<crate::modules::retarget_export::RetargetExportError>
    for OperationError
{
    fn from(
        error: crate::modules::retarget_export::RetargetExportError,
    ) -> Self {
        use crate::modules::retarget_export::RetargetExportError::*;
        match error {
            Retarget(e) => e.into(),
            Glb(e) => e.into(),
            Bvh(e) => e.into(),
        }
    }
}

pub fn load_json<T: for<'de> serde::Deserialize<'de>>(
    path: &Path,
    label: &str,
) -> Result<T, OperationError> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| {
        OperationError::validation(format!("Invalid {label}: {e}"))
    })
}
pub fn file_sha256(path: &Path) -> Result<String, OperationError> {
    Ok(crate::modules::retarget::sha256_hex(&std::fs::read(path)?))
}

/// Resolve existing ancestors as well as nonexistent output leaves.
pub fn path_identity(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => {
                result.push(other.as_os_str());
                // Resolve before processing a following parent component so that
                // symlink/.. follows filesystem semantics rather than lexical ones.
                if let Ok(resolved) = std::fs::canonicalize(&result) {
                    result = resolved;
                }
            }
        }
    }
    result
}
pub fn same_path(left: &Path, right: &Path) -> bool {
    let left = path_identity(left);
    let right = path_identity(right);
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}
pub fn check_output(
    path: &Path,
    sources: &[&Path],
    overwrite: bool,
) -> Result<(), OperationError> {
    if sources.iter().any(|source| same_path(path, source)) {
        return Err(OperationError::validation(
            "Output must not replace an input file",
        ));
    }
    if path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Output path is a directory",
        )
        .into());
    }
    let mut ancestor = path.parent().filter(|p| !p.as_os_str().is_empty());
    while let Some(parent) = ancestor {
        if parent.exists() {
            if !parent.is_dir() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotADirectory,
                    "Output parent is not a directory",
                )
                .into());
            }
            break;
        }
        ancestor = parent.parent();
    }
    if path.exists() && !overwrite {
        return Err(OperationError::validation(format!(
            "Output already exists: {} (enable overwrite)",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unreadable_fingerprint_returns_io_error() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            file_sha256(&dir.path().join("missing.glb"))
                .unwrap_err()
                .kind,
            ErrorKind::Io
        );
    }
    #[test]
    fn output_parent_file_is_an_io_error_before_staging() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"original").unwrap();
        let error =
            check_output(&blocker.join("out.glb"), &[], false).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Io);
        assert_eq!(std::fs::read(blocker).unwrap(), b"original");
    }
    #[cfg(unix)]
    #[test]
    fn source_protection_resolves_symlinks_before_parent_components() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::create_dir(dir.path().join("other")).unwrap();
        std::os::unix::fs::symlink(&nested, dir.path().join("other/link"))
            .unwrap();
        let source = dir.path().join("source.glb");
        std::fs::write(&source, b"source").unwrap();
        let output = dir.path().join("other/link/../source.glb");
        assert!(same_path(&source, &output));
        assert!(check_output(&output, &[&source], true).is_err());
    }
}
