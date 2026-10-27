//! Strict BBT traversal without a whole-store block map or visited-page set.
//!
//! Validation retains one physical page per level (at most 33 * 4096 bytes).
//! Strictly decreasing levels terminate cycles; disjoint key ranges and exact
//! child minima reject aliases as well as duplicate/misplaced leaf entries.
use super::*;

impl Pst {
    fn bbt_page(&mut self, at: Bref) -> Result<(Vec<u8>, usize, usize, u8), String> {
        let bytes = self.read_page(at)?;
        if bytes[self.page.trailer] != PTYPE_BBT {
            return Err("Expected a block-index page".into());
        }
        let f = self.page.footer;
        let (count, size, level) = if self.page.wide_counts {
            (
                u16le(&bytes, f) as usize,
                bytes[f + 4] as usize,
                bytes[f + 5],
            )
        } else {
            (bytes[f] as usize, bytes[f + 2] as usize, bytes[f + 3])
        };
        let w = self.page.w();
        let need = if level == 0 { 2 * w + 4 } else { 3 * w };
        if size < need || count * size > f || u32::from(level) > MAX_BTREE_DEPTH {
            return Err("Invalid block-index page geometry".into());
        }
        Ok((bytes, count, size, level))
    }

    fn validate_bbt_range(
        &mut self,
        at: Bref,
        expected_level: Option<u8>,
        lower: Option<u64>,
        upper: Option<u64>,
    ) -> Result<(), String> {
        let (bytes, count, size, level) = self.bbt_page(at)?;
        if expected_level.is_some_and(|expected| level != expected)
            || (count == 0 && (lower.is_some() || level != 0))
        {
            return Err("Invalid block-index child level or empty child".into());
        }
        let mut previous = None;
        for i in 0..count {
            let entry = &bytes[i * size..(i + 1) * size];
            let key = self.page.wide(entry, 0) & !1;
            if previous.is_some_and(|last| key <= last)
                || (i == 0 && lower.is_some_and(|min| key != min))
                || upper.is_some_and(|max| key >= max)
            {
                return Err("Duplicate or out-of-order block-index key".into());
            }
            previous = Some(key);
            if level != 0 {
                let next = if i + 1 < count {
                    Some(self.page.wide(&bytes, (i + 1) * size) & !1)
                } else {
                    upper
                };
                let child = self.page.bref(entry, self.page.w());
                self.validate_bbt_range(child, Some(level - 1), Some(key), next)?;
            }
        }
        Ok(())
    }

    pub(super) fn indexed_block(&mut self, bid: u64) -> Result<Block, String> {
        if !self.bbt_validated {
            if !self.warnings.is_empty() {
                return Err("The block index failed integrity checks".into());
            }
            self.validate_bbt_range(self.bbt_root, None, None, None)?;
            self.bbt_validated = true;
        }
        let key = bid & !1;
        let mut at = self.bbt_root;
        // The source handle is immutable during conversion. No lookup populates bbt.
        for _ in 0..=MAX_BTREE_DEPTH {
            let (bytes, count, size, level) = self.bbt_page(at)?;
            let found = (0..count)
                .take_while(|&i| self.page.wide(&bytes, i * size) & !1 <= key)
                .last();
            let Some(i) = found else { break };
            let entry = &bytes[i * size..(i + 1) * size];
            let w = self.page.w();
            if level == 0 {
                if self.page.wide(entry, 0) & !1 == key {
                    return Ok(Block {
                        bid: self.page.wide(entry, 0),
                        ib: self.page.wide(entry, w),
                        cb: u16le(entry, 2 * w),
                        cref: u16le(entry, 2 * w + 2),
                    });
                }
                break;
            }
            at = self.page.bref(entry, w);
        }
        Err(format!("block {bid} is not in the block index"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture {
        pst: Pst,
        path: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    // File-backed BBT-only corpus. Neither the builder nor lookup retains a
    // vector of records/pages. This is an index test, not a conversion fixture.
    fn fixture(count: u64, layout: Layout) -> Fixture {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "pstoff-bbt-{}-{}.bin",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        fn tree(file: &mut File, layout: Layout, first: u64, count: u64) -> Bref {
            const FANOUT: u64 = 16;
            let mut page = vec![0; layout.size as usize];
            let w = layout.w();
            let mut level = 0u8;
            let mut span = FANOUT;
            while span < count {
                level += 1;
                span *= FANOUT;
            }
            let width = if level == 0 { 2 * w + 4 } else { 3 * w };
            let step = if level == 0 { 1 } else { span / FANOUT };
            let entries = count.div_ceil(step);
            let put = |bytes: &mut [u8], at: usize, n: u64| {
                bytes[at..at + w].copy_from_slice(&n.to_le_bytes()[..w]);
            };
            for i in 0..entries {
                let ordinal = first + i * step;
                let offset = i as usize * width;
                put(&mut page, offset, ordinal * 4);
                if level == 0 {
                    put(&mut page, offset + w, 0x100000 + ordinal * 64);
                    page[offset + 2 * w..offset + 2 * w + 2].copy_from_slice(&13u16.to_le_bytes());
                } else {
                    let child = tree(file, layout, ordinal, step.min(count - i * step));
                    put(&mut page, offset + w, child.bid);
                    put(&mut page, offset + 2 * w, child.ib);
                }
            }
            let f = layout.footer;
            if layout.wide_counts {
                page[f..f + 2].copy_from_slice(&(entries as u16).to_le_bytes());
                page[f + 4] = width as u8;
                page[f + 5] = level;
            } else {
                page[f] = entries as u8;
                page[f + 2] = width as u8;
                page[f + 3] = level;
            }
            let ib = file.seek(SeekFrom::End(0)).unwrap().max(layout.size);
            file.seek(SeekFrom::Start(ib)).unwrap();
            let at = Bref { bid: ib * 4, ib };
            let t = layout.trailer;
            page[t] = PTYPE_BBT;
            page[t + 1] = PTYPE_BBT;
            let crc = crc32(&page[..t]);
            if layout.narrow {
                put(&mut page, t + 4, at.bid);
                page[t + 8..t + 12].copy_from_slice(&crc.to_le_bytes());
            } else {
                page[t + 4..t + 8].copy_from_slice(&crc.to_le_bytes());
                put(&mut page, t + 8, at.bid);
            }
            file.write_all(&page).unwrap();
            at
        }
        let root = tree(&mut file, layout, 1, count);
        let actual_len = file.metadata().unwrap().len();
        Fixture {
            path,
            pst: Pst {
                file,
                page: layout,
                crypt: Crypt::None,
                ver: 36,
                is_ost: true,
                declared_len: actual_len,
                actual_len,
                nbt_root: root,
                bbt_root: root,
                bbt: None,
                bbt_validated: false,
                warnings: vec![],
                strict: true,
                suppressed: 0,
            },
        }
    }

    #[test]
    fn strict_lookup_matches_legacy_walk_without_populating_map() {
        for layout in [LAYOUT_ANSI, LAYOUT_512, LAYOUT_4K] {
            for count in [1, 128, 4096, 65536] {
                let mut f = fixture(count, layout);
                // The comparison deliberately uses the old materialized API.
                let expected = f.pst.blocks();
                assert_eq!(expected.len(), count as usize);
                for ordinal in [1, count / 2 + 1, count] {
                    let found = f.pst.indexed_block((ordinal * 4) | 1).unwrap();
                    let old = &expected[ordinal as usize - 1];
                    assert_eq!((found.bid, found.ib, found.cb), (old.bid, old.ib, old.cb));
                }
                assert!(f.pst.indexed_block((count + 1) * 4).is_err());
                assert!(f.pst.bbt.is_none());
                assert!(f.pst.bbt_validated);
            }
        }
    }

    #[test]
    fn strict_block_read_uses_disk_index_even_when_payload_is_truncated() {
        let mut f = fixture(128, LAYOUT_4K);
        assert!(f.pst.block(4).unwrap_err().contains("file ends"));
        assert!(f.pst.bbt.is_none());
        assert!(f.pst.bbt_validated);
    }

    fn edit_root(f: &mut Fixture, edit: impl FnOnce(&mut [u8])) {
        let root = f.pst.bbt_root;
        let mut bytes = f.pst.read_page(root).unwrap();
        edit(&mut bytes);
        let t = f.pst.page.trailer;
        let crc = crc32(&bytes[..t]);
        bytes[t + 4..t + 8].copy_from_slice(&crc.to_le_bytes());
        f.pst.file.seek(SeekFrom::Start(root.ib)).unwrap();
        f.pst.file.write_all(&bytes).unwrap();
    }

    #[test]
    fn strict_lookup_rejects_damage_outside_requested_branch() {
        let mut f = fixture(128, LAYOUT_4K);
        // A duplicate separator in the last branch must fail even for the first key.
        edit_root(&mut f, |b| {
            let previous = b[6 * 24..6 * 24 + 8].to_vec();
            b[7 * 24..7 * 24 + 8].copy_from_slice(&previous);
        });
        assert!(f.pst.indexed_block(4).is_err());
        assert!(!f.pst.bbt_validated);
    }

    #[test]
    fn strict_lookup_checks_unrequested_child_crc() {
        let mut f = fixture(128, LAYOUT_4K);
        let root = f.pst.read_page(f.pst.bbt_root).unwrap();
        let last = f.pst.page.bref(&root, 7 * 24 + 8);
        f.pst.file.seek(SeekFrom::Start(last.ib)).unwrap();
        f.pst.file.write_all(&[0xff]).unwrap();
        assert!(f.pst.indexed_block(4).is_err());
        assert!(!f.pst.bbt_validated);
    }

    #[test]
    fn strict_lookup_checks_geometry_and_child_minimum() {
        for geometry in [false, true] {
            let mut f = fixture(128, LAYOUT_4K);
            edit_root(&mut f, |b| {
                if geometry {
                    b[LAYOUT_4K.footer + 4] = 0;
                } else {
                    // Ordered, but no longer the child's actual minimum key.
                    b[24..32].copy_from_slice(&70u64.to_le_bytes());
                }
            });
            assert!(f.pst.indexed_block(4).is_err());
        }
    }

    #[test]
    fn strict_lookup_rejects_cycles_and_child_aliases() {
        for cycle in [false, true] {
            let mut f = fixture(128, LAYOUT_4K);
            let root = f.pst.bbt_root;
            edit_root(&mut f, |b| {
                if cycle {
                    b[8..16].copy_from_slice(&root.bid.to_le_bytes());
                    b[16..24].copy_from_slice(&root.ib.to_le_bytes());
                } else {
                    let child = b[8..24].to_vec();
                    b[32..48].copy_from_slice(&child);
                }
            });
            assert!(f.pst.indexed_block(4).is_err());
        }
    }
}
