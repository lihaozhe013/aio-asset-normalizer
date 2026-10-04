//! Small path and hashing helpers shared by the command modules.

use std::path::PathBuf;

pub use aio_asset_normalizer::modules::operations::same_path;

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
