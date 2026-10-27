use super::*;
use crate::ndb::{crc32, Bref, Crypt, LAYOUT_4K};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
};

struct Fixture {
    pst: Pst,
    path: PathBuf,
    root: u64,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// Generated, file-backed stream with a multilevel BBT. Payload generation keeps
// one block, not the logical stream. Metadata here is test-only, not a ceiling
// corpus builder or a measurement of conversion private commit.
fn fixture(count: usize, fault: &str) -> Fixture {
    static SERIAL: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "pstoff-chunks-{}-{}.bin",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = File::create(&path).unwrap();
    file.set_len(4096).unwrap();
    let mut entries = Vec::new();
    let mut append = |bid: u64, bytes: &[u8], inflated: usize| {
        let at = file.seek(SeekFrom::End(0)).unwrap();
        let size = (bytes.len() + 24).div_ceil(512) * 512;
        let mut block = vec![0; size];
        block[..bytes.len()].copy_from_slice(bytes);
        let t = size - 24;
        block[t..t + 2].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
        block[t + 4..t + 8].copy_from_slice(&crc32(bytes).to_le_bytes());
        block[t + 8..t + 16].copy_from_slice(&bid.to_le_bytes());
        block[t + 18..t + 20].copy_from_slice(&(inflated as u16).to_le_bytes());
        if fault == "crc" && bid == count as u64 * 4 {
            block[0] ^= 1;
        }
        if fault == "stored" && bid == 4 {
            block[t] ^= 1;
        }
        file.write_all(&block).unwrap();
        let mut entry = vec![0; 24];
        entry[..8].copy_from_slice(&bid.to_le_bytes());
        entry[8..16].copy_from_slice(&at.to_le_bytes());
        entry[16..18].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
        entries.push(entry);
    };
    for i in 0..count {
        if matches!(fault, "compressed" | "inflate") {
            let bytes = vec![73; 65535];
            let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&bytes, 6);
            append(
                (i as u64 + 1) * 4,
                &compressed,
                if fault == "inflate" { 65534 } else { 65535 },
            );
            continue;
        }
        let bytes: Vec<_> = (0..7001).map(|j| ((i * 17 + j) % 251) as u8).collect();
        append((i as u64 + 1) * 4, &bytes, bytes.len());
    }
    let mut children = Vec::new();
    let mut next = ((count as u64 + 1) * 4) | 2;
    for group in (0..count).collect::<Vec<_>>().chunks(127) {
        let mut bytes = vec![1, 1, 0, 0, 0, 0, 0, 0];
        bytes[2..4].copy_from_slice(&(group.len() as u16).to_le_bytes());
        let total = group.len() as u32 * 7001;
        bytes[4..8].copy_from_slice(&(total + u32::from(fault == "child-total")).to_le_bytes());
        for i in group {
            bytes.extend_from_slice(&((*i as u64 + 1) * 4).to_le_bytes());
        }
        if fault == "kind" {
            bytes[8..16].copy_from_slice(&next.to_le_bytes());
        }
        append(next, &bytes, bytes.len());
        children.push(next);
        next += 4;
    }
    let root = next;
    let mut bytes = vec![1, 2, 0, 0, 0, 0, 0, 0];
    bytes[2..4].copy_from_slice(&(children.len() as u16).to_le_bytes());
    let total = count as u32 * 7001;
    let total = match fault {
        "short" => total - 1,
        "long" => total + 1,
        _ => total,
    };
    bytes[4..8].copy_from_slice(&total.to_le_bytes());
    for child in children {
        bytes.extend_from_slice(&child.to_le_bytes());
    }
    if fault == "cycle" {
        bytes[8..16].copy_from_slice(&root.to_le_bytes());
    }
    if fault == "level" {
        bytes[1] = 3;
    }
    if fault == "truncated" {
        bytes.pop();
    }
    append(root, &bytes, bytes.len());
    drop(append);
    let mut parents = Vec::new();
    for group in entries.chunks(128) {
        let at = file.seek(SeekFrom::End(0)).unwrap();
        let mut page = vec![0; 4096];
        for (i, entry) in group.iter().enumerate() {
            page[i * 24..i * 24 + 24].copy_from_slice(entry);
        }
        seal_page(&mut page, group.len(), 0, at);
        file.write_all(&page).unwrap();
        let mut parent = group[0][..8].to_vec();
        parent.extend_from_slice(&(at * 4).to_le_bytes());
        parent.extend_from_slice(&at.to_le_bytes());
        parents.push(parent);
    }
    let at = file.seek(SeekFrom::End(0)).unwrap();
    let mut page = vec![0; 4096];
    for (i, p) in parents.iter().enumerate() {
        page[i * 24..i * 24 + 24].copy_from_slice(p);
    }
    seal_page(&mut page, parents.len(), 1, at);
    file.write_all(&page).unwrap();
    let actual_len = file.metadata().unwrap().len();
    drop(file);
    let bref = Bref {
        bid: at * 4,
        ib: at,
    };
    Fixture {
        path: path.clone(),
        root,
        pst: Pst {
            file: File::open(path).unwrap(),
            page: LAYOUT_4K,
            crypt: Crypt::None,
            ver: 36,
            is_ost: true,
            declared_len: actual_len,
            actual_len,
            nbt_root: bref,
            bbt_root: bref,
            bbt: None,
            bbt_validated: false,
            strict: true,
            warnings: vec![],
            suppressed: 0,
        },
    }
}
fn seal_page(page: &mut [u8], count: usize, level: u8, at: u64) {
    page[4056..4058].copy_from_slice(&(count as u16).to_le_bytes());
    page[4058..4060].copy_from_slice(&169u16.to_le_bytes());
    page[4060] = 24;
    page[4061] = level;
    page[4072] = 0x80;
    page[4073] = 0x80;
    let crc = crc32(&page[..4072]);
    page[4076..4080].copy_from_slice(&crc.to_le_bytes());
    page[4080..4088].copy_from_slice(&(at * 4).to_le_bytes());
}

#[test]
fn large_stream_order_is_bounded_and_source_is_unchanged() {
    let mut f = fixture(4096, ""); // >27 MiB, unequal to writer block boundaries
    let before = std::fs::read(&f.path).unwrap();
    let mut cursor = f.pst.node_chunks(f.root).unwrap();
    let mut count = 0;
    while let Some(chunk) = cursor.next() {
        let chunk = chunk.unwrap();
        assert_eq!(chunk.len(), 7001);
        for (j, byte) in chunk.iter().enumerate() {
            assert_eq!(*byte, ((count * 17 + j) % 251) as u8);
        }
        assert!(cursor.frames.len() <= 2);
        assert!(cursor.frames.iter().all(|f| f.bytes.len() <= 65535));
        count += 1;
    }
    assert_eq!(count, 4096);
    drop(cursor);
    assert!(f.pst.bbt.is_none());
    assert!(f.pst.bbt_validated);
    assert_eq!(before, std::fs::read(&f.path).unwrap());
}

#[test]
fn corruption_is_rejected_including_late_crc_and_both_totals() {
    for fault in [
        "crc",
        "stored",
        "child-total",
        "short",
        "long",
        "kind",
        "cycle",
        "level",
        "truncated",
    ] {
        let mut f = fixture(260, fault);
        let mut cursor = f.pst.node_chunks(f.root).unwrap();
        let mut rejected = false;
        for chunk in cursor.by_ref() {
            if chunk.is_err() {
                rejected = true;
                break;
            }
        }
        assert!(rejected, "{fault}");
        assert!(
            cursor.next().is_none(),
            "failed cursor must be fused: {fault}"
        );
    }
}

#[test]
fn empty_and_abandoned_cursors_do_not_reopen_or_write_source() {
    let mut f = fixture(260, "");
    assert!(f.pst.node_chunks(0).unwrap().next().is_none());
    let first = f.pst.node_chunks(f.root).unwrap().next().unwrap().unwrap();
    assert_eq!(first.len(), 7001);
    // Restart on the same original handle after cancellation/early drop.
    assert_eq!(f.pst.node_chunks(f.root).unwrap().count(), 260);
    assert!(f.pst.bbt.is_none());
}

#[test]
fn decoded_chunks_allow_full_u16_lengths_and_reject_inflation_mismatch() {
    let mut f = fixture(1, "compressed");
    let mut cursor = f.pst.node_chunks(4).unwrap();
    assert_eq!(cursor.next().unwrap().unwrap(), vec![73; 65535]);
    assert!(cursor.next().is_none());
    let mut f = fixture(1, "inflate");
    assert!(f.pst.node_chunks(4).unwrap().next().unwrap().is_err());
}
