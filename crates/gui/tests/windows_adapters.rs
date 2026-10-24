#![cfg(windows)]

use pstoff_engine::platform::{classify_windows_io, AvailableSpace, IoFailure};
use pstoff_gui::windows::{open_source, WindowsSpace};
use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct SyntheticDirectory(PathBuf);

impl SyntheticDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pstoff-adapter-{}-{}-synthetic-\u{00e9}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for SyntheticDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn incompatible_sharing_is_in_use_and_does_not_change_source() {
    let directory = SyntheticDirectory::new();
    let path = directory.0.join("synthetic source.ost");
    let original = b"synthetic adapter fixture; not an OST";
    fs::write(&path, original).unwrap();
    let exclusive = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    let error = open_source(&path).unwrap_err();
    assert_eq!(
        classify_windows_io(error.raw_os_error().unwrap() as u32),
        IoFailure::InUse
    );
    drop(exclusive);
    let source = open_source(&path).unwrap();
    assert!(OpenOptions::new().write(true).open(&path).is_err());
    drop(source);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn missing_source_is_not_classified_as_in_use() {
    let directory = SyntheticDirectory::new();
    let error = open_source(&directory.0.join("absent.ost")).unwrap_err();
    assert_eq!(
        classify_windows_io(error.raw_os_error().unwrap() as u32),
        IoFailure::Other(2)
    );
}

#[test]
fn available_space_handles_unicode_directory_and_rejects_nul() {
    let directory = SyntheticDirectory::new();
    assert!(WindowsSpace.available_bytes(&directory.0).unwrap() > 0);
    let invalid = directory.0.join("invalid\0suffix");
    assert_eq!(
        WindowsSpace.available_bytes(&invalid).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
}
