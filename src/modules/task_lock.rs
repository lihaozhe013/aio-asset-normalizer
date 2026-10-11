//! Cross-process serialization for asset computations on one user profile.

use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

static PROCESS_ASSET_TASK_LOCKED: AtomicBool = AtomicBool::new(false);

struct ProcessLockGuard;

impl Drop for ProcessLockGuard {
    fn drop(&mut self) {
        PROCESS_ASSET_TASK_LOCKED.store(false, Ordering::Release);
    }
}

pub struct AssetTaskGuard {
    _file: File,
    _process_guard: ProcessLockGuard,
}

#[derive(Debug)]
pub enum AssetTaskLockError {
    Busy,
    Unavailable(String),
}

impl std::fmt::Display for AssetTaskLockError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => formatter.write_str("Another asset computation is active for this user"),
            Self::Unavailable(message) => {
                write!(formatter, "Cannot acquire asset task lock: {message}")
            }
        }
    }
}

impl AssetTaskGuard {
    pub fn try_acquire() -> Result<Self, AssetTaskLockError> {
        if PROCESS_ASSET_TASK_LOCKED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(AssetTaskLockError::Busy);
        }
        let process_guard = ProcessLockGuard;
        let directory = lock_directory()?;
        fs::create_dir_all(&directory)
            .map_err(|error| AssetTaskLockError::Unavailable(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|error| AssetTaskLockError::Unavailable(error.to_string()))?;
        }
        let path = directory.join("asset-task.lock");
        let file = open_lock_file(&path)
            .map_err(|error| AssetTaskLockError::Unavailable(error.to_string()))?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Self {
                _file: file,
                _process_guard: process_guard,
            }),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                Err(AssetTaskLockError::Busy)
            }
            Err(error) => Err(AssetTaskLockError::Unavailable(error.to_string())),
        }
    }
}

fn lock_directory() -> Result<PathBuf, AssetTaskLockError> {
    dirs::data_local_dir()
        .map(|root| root.join("AIO Asset Normalizer").join("locks"))
        .ok_or_else(|| {
            AssetTaskLockError::Unavailable("user data directory is unavailable".to_owned())
        })
}

#[cfg(unix)]
fn open_lock_file(path: &std::path::Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_lock_file(path: &std::path::Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_lock_allows_only_one_owner_at_a_time() {
        let first = match AssetTaskGuard::try_acquire() {
            Ok(guard) => guard,
            Err(AssetTaskLockError::Busy) => return,
            Err(error) => panic!("first lock acquisition failed: {error}"),
        };
        assert!(matches!(
            AssetTaskGuard::try_acquire(),
            Err(AssetTaskLockError::Busy)
        ));
        drop(first);
        assert!(AssetTaskGuard::try_acquire().is_ok());
    }
}
