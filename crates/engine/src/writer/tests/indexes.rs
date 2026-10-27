//! Original in-memory writer tree retained only as a byte-equivalence oracle.
use super::*;
impl Output<'_> {
    fn tree_legacy(
        &mut self,
        mut entries: Vec<(u64, Vec<u8>)>,
        kind: u8,
        mut width: usize,
    ) -> Result<(u64, u64)> {
        let mut level = 0;
        loop {
            let mut parents = Vec::new();
            for group in entries.chunks(488 / width) {
                let at = self.place(512, 512);
                let id = self.id(false);
                let mut b = vec![0; 512];
                for (i, (_, entry)) in group.iter().enumerate() {
                    b[i * width..(i + 1) * width].copy_from_slice(entry);
                }
                b[488] = group.len() as u8;
                b[489] = (488 / width) as u8;
                b[490] = width as u8;
                b[491] = level;
                b[496] = kind;
                b[497] = kind;
                p16(&mut b, 498, signature(at, id));
                let crc = crc32(&b[..496]);
                p32(&mut b, 500, crc);
                p64(&mut b, 504, id);
                self.write(at, &b)?;
                let mut parent = vec![0; 24];
                p64(&mut parent, 0, group[0].0);
                p64(&mut parent, 8, id);
                p64(&mut parent, 16, at);
                parents.push((group[0].0, parent));
            }
            if parents.len() == 1 {
                let b = &parents[0].1;
                return Ok((
                    u64::from_le_bytes(b[8..16].try_into()?),
                    u64::from_le_bytes(b[16..24].try_into()?),
                ));
            }
            if parents.is_empty() {
                return Err("An empty index cannot be published".into());
            }
            entries = parents;
            width = 24;
            level += 1;
        }
    }
}
#[test]
fn disk_index_pages_match_original_writer_byte_for_byte() {
    let fixture = support::Fixture::invalid();
    for width in [24, 32] {
        let a = fixture.root.join("spooled.bin");
        let b = fixture.root.join("original.bin");
        let mut bounded = tables::output(&a);
        let mut original = tables::output(&b);
        let mut records = spool::Spool::new().unwrap();
        let mut legacy = vec![];
        for key in 1..4096u64 {
            let mut entry = vec![0; width];
            p64(&mut entry, 0, key * 4);
            p64(&mut entry, 8, key * 128);
            records.append(&entry).unwrap();
            legacy.push((key * 4, entry));
        }
        let kind = if width == 24 { 0x80 } else { 0x81 };
        assert_eq!(
            bounded.tree(records, kind, width).unwrap(),
            original.tree_legacy(legacy, kind, width).unwrap()
        );
        drop(bounded);
        drop(original);
        assert_eq!(std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
    }
}
