use super::*;
use std::{
    cell::Cell,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
#[path = "finalizer_allocations.rs"]
mod allocations;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pstoff-finalizer-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture(path: &Path, intervals: u64) {
    let mut file = File::create(path).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[link(name = "Kernel32")]
        unsafe extern "system" {
            fn DeviceIoControl(
                handle: *mut std::ffi::c_void,
                code: u32,
                input: *const u8,
                input_len: u32,
                output: *mut u8,
                output_len: u32,
                returned: *mut u32,
                overlapped: *mut u8,
            ) -> i32;
        }
        let mut returned = 0;
        // SAFETY: live synchronous file handle; FSCTL_SET_SPARSE takes no buffers.
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    file.as_raw_handle(),
                    0x900c4,
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            },
            0
        );
    }
    let eof = AMAP_FIRST_OFFSET + intervals * AMAP_DATA_SIZE;
    file.set_len(eof).unwrap();
    let nbt = AMAP_FIRST_OFFSET + 4096;
    let bbt = nbt + 512;
    let data = AMAP_FIRST_OFFSET + 2048;
    let root = UnicodeRoot::new(
        eof.into(),
        (eof - AMAP_DATA_SIZE).into(),
        0.into(),
        0.into(),
        UnicodePageRef::new(8.into(), nbt.into()),
        UnicodePageRef::new(12.into(), bbt.into()),
        AmapStatus::Invalid,
    );
    UnicodeHeader::new(root, NdbCryptMethod::None)
        .write(&mut file)
        .unwrap();
    let mut block = [0u8; 128];
    block[..64].fill(37);
    block[112..114].copy_from_slice(&64u16.to_le_bytes());
    block[114..116].copy_from_slice(&PageType::BlockBTree.signature(data, 4).to_le_bytes());
    let crc = crate::crc::compute_crc(0, &block[..64]);
    block[116..120].copy_from_slice(&crc.to_le_bytes());
    block[120..128].copy_from_slice(&4u64.to_le_bytes());
    file.seek(SeekFrom::Start(data)).unwrap();
    file.write_all(&block).unwrap();
    for (at, id, kind, width) in [(nbt, 8u64, 0x81, 32), (bbt, 12, 0x80, 24)] {
        let mut page = [0; 512];
        page[..8].copy_from_slice(&if kind == 0x81 { 0x24u64 } else { 4 }.to_le_bytes());
        page[8..16].copy_from_slice(&if kind == 0x81 { 4u64 } else { data }.to_le_bytes());
        if kind == 0x80 {
            page[16..18].copy_from_slice(&64u16.to_le_bytes());
            page[18..20].copy_from_slice(&2u16.to_le_bytes());
        }
        page[488] = 1;
        page[489] = (488usize / width as usize) as u8;
        page[490] = width;
        page[496] = kind;
        page[497] = kind;
        let sig = ((at ^ id) ^ ((at ^ id) >> 16)) as u16;
        page[498..500].copy_from_slice(&sig.to_le_bytes());
        let crc = crate::crc::compute_crc(0, &page[..496]);
        page[500..504].copy_from_slice(&crc.to_le_bytes());
        page[504..512].copy_from_slice(&id.to_le_bytes());
        file.seek(SeekFrom::Start(at)).unwrap();
        file.write_all(&page).unwrap();
    }
}
#[test]
fn streamed_finalizer_matches_pinned_oracle_and_reopens() {
    let dir = Fixture::new();
    for count in [1, 8, 129, 625, 8193] {
        let bounded = dir.0.join("bounded.pst");
        let legacy = dir.0.join("legacy.pst");
        fixture(&bounded, count);
        fixture(&legacy, count);
        let mut pst = UnicodePstFile::open(&bounded).unwrap();
        pst.finalize_with_cancel(&|| false).unwrap();
        drop(pst);
        let mut pst = UnicodePstFile::open(&legacy).unwrap();
        pst.inner.rebuild_allocation_map_legacy().unwrap();
        pst.lock().unwrap().flush().unwrap();
        drop(pst);
        assert_eq!(
            std::fs::metadata(&bounded).unwrap().len(),
            std::fs::metadata(&legacy).unwrap().len()
        );
        let mut a = File::open(&bounded).unwrap();
        let mut b = File::open(&legacy).unwrap();
        let mut left = [0; 65536];
        let mut right = [0; 65536];
        loop {
            let n = a.read(&mut left).unwrap();
            if n == 0 {
                break;
            }
            b.read_exact(&mut right[..n]).unwrap();
            assert_eq!(left[..n], right[..n]);
        }
        let pst = UnicodePstFile::open(&bounded).unwrap();
        assert_eq!(pst.header().root().amap_is_valid(), AmapStatus::Valid2);
        assert_eq!(pst.read_block(4.into()).unwrap(), [37; 64]);
        assert!(pst.read_node(0x24.into()).is_ok());
    }
}
#[test]
fn finalizer_allocation_stays_bounded_and_cancellation_removes_spool() {
    let dir = Fixture::new();
    for count in [16, 2048] {
        let path = dir.0.join("large.pst");
        fixture(&path, count);
        let (result, peak) = allocations::measure(|| {
            UnicodePstFile::open(&path)
                .unwrap()
                .finalize_with_cancel(&|| false)
        });
        result.unwrap();
        eprintln!("finalizer intervals={count}, allocation delta={peak}");
        assert!(peak < 128 * 1024, "finalizer allocation delta {peak}");
    }
    for threshold in [0, 150, 400] {
        let path = dir.0.join("cancel.pst");
        fixture(&path, 129);
        let calls = Cell::new(0);
        let mut pst = UnicodePstFile::open(&path).unwrap();
        let error = pst
            .finalize_with_cancel(&|| {
                calls.set(calls.get() + 1);
                calls.get() > threshold
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        drop(pst);
        assert_eq!(
            UnicodePstFile::open(&path)
                .unwrap()
                .header()
                .root()
                .amap_is_valid(),
            AmapStatus::Invalid
        );
    }
}
