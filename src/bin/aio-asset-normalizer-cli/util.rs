//! Small path and hashing helpers shared by the command modules.

use std::path::{Path, PathBuf};

use aio_asset_normalizer::modules::retarget::sha256_hex;

pub fn file_sha256(path: &Path) -> String {
    std::fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_default()
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    let left =
        std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right =
        std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());

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

/// Longest shared parent directory of the given files. A single input maps to
/// its own directory so batch output keeps the input tree below it.
pub fn common_parent(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut iter = paths.iter();
    let mut common = iter.next()?.parent()?.to_path_buf();
    for path in iter {
        let parent = path.parent()?;
        while !parent.starts_with(&common) {
            common = common.parent()?.to_path_buf();
        }
    }
    Some(common)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_parent_collapses_to_the_shared_directory() {
        let parent = common_parent(&[
            PathBuf::from("/tmp/assets/a.glb"),
            PathBuf::from("/tmp/assets/nested/b.glb"),
        ]);
        assert_eq!(parent, Some(PathBuf::from("/tmp/assets")));
    }

    #[test]
    fn same_path_accepts_identical_files() {
        let path = PathBuf::from("assets/model.glb");
        assert!(same_path(&path, &path));
    }
}
