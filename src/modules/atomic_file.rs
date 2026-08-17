use std::io;
use std::path::Path;

#[cfg(any(not(windows), test))]
use std::fs;

pub(crate) fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        replace_windows(source, destination)
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)
    }
}

#[cfg(windows)]
fn replace_windows(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
    const REPLACEFILE_WRITE_THROUGH: u32 = 0x0000_0001;

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let destination_exists = destination.exists();
    let source = wide_path(source);
    let destination = wide_path(destination);
    let result = unsafe {
        if destination_exists {
            ReplaceFileW(
                destination.as_ptr(),
                source.as_ptr(),
                ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        } else {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut c_void,
            reserved: *mut c_void,
        ) -> i32;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_overwrites_an_existing_file() {
        let base = std::env::temp_dir().join(format!(
            "aio-asset-normalizer-atomic-file-{}",
            std::process::id()
        ));
        let source = base.with_extension("tmp");
        fs::write(&base, b"old").unwrap();
        fs::write(&source, b"new").unwrap();

        replace(&source, &base).unwrap();

        assert_eq!(fs::read(&base).unwrap(), b"new");
        assert!(!source.exists());
        let _ = fs::remove_file(&base);
    }
}
