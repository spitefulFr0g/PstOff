//! Windows host adapters; no conversion policy lives here.
#[cfg(windows)]
pub mod windows {
    use pstoff_engine::platform::AvailableSpace;
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt};
    use std::path::Path;

    /// Read only, permitting other readers but denying concurrent writes/deletion.
    /// Keep this handle alive while reading, rather than probing and reopening.
    pub fn open_source(path: &Path) -> io::Result<File> {
        OpenOptions::new().read(true).share_mode(1).open(path)
    }

    pub struct WindowsSpace;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }

    impl AvailableSpace for WindowsSpace {
        fn available_bytes(&self, directory: &Path) -> io::Result<u64> {
            // Reject embedded NUL rather than querying an unintended path prefix.
            let mut wide: Vec<u16> = directory.as_os_str().encode_wide().collect();
            if wide.contains(&0) {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"));
            }
            wide.push(0);
            let mut available = 0;
            // SAFETY: wide is terminated and alive; available is writable. The
            // optional total/free pointers are null as permitted by Win32.
            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    wide.as_ptr(),
                    &mut available,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(available)
            }
        }
    }
}
