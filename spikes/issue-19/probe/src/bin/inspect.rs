use pstfree::{ltp::Heap, ndb::Pst};
fn u16b(b: &[u8], p: usize) -> usize {
    u16::from_le_bytes(b[p..p + 2].try_into().unwrap()) as usize
}
fn u32b(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn main() {
    let mut p = Pst::open(&std::env::args().nth(1).unwrap()).unwrap();
    for n in p.nodes() {
        if n.bid_data == 0 {
            continue;
        }
        let h = match Heap::new(p.node_blocks(n.bid_data).unwrap()) {
            Ok(h) => h,
            Err(_) => continue,
        };
        if n.nid_type() == 4 || n.nid_type() == 8 {
            let bth = h.item(h.user_root).unwrap();
            assert_eq!(bth[3], 0);
            let rs: Vec<_> = h
                .item(u32b(bth, 4))
                .unwrap()
                .chunks_exact(8)
                .map(|r| r.to_vec())
                .collect();
            for r in rs {
                if u16b(&r, 0) == 0x3008 {
                    println!(
                        "PC {:x} modtime type={:x} ref={:x} allocation={:?}",
                        n.nid,
                        u16b(&r, 2),
                        u32b(&r, 4),
                        h.item(u32b(&r, 4)).map(|b| b.len())
                    );
                }
            }
        }
        if n.nid_type() != 0xe {
            continue;
        }
        let i = h.item(h.user_root).unwrap();
        let rm = u32b(i, 14);
        println!(
            "TC {:x} cols={} ends={:?} rm={:x}",
            n.nid,
            i[1],
            &i[2..10],
            rm
        );
        if rm == 0 {
            continue;
        }
        let rows = h.item(rm).unwrap();
        for c in i[22..].chunks_exact(8) {
            let tag = u32b(c, 0);
            if tag as u16 == 0x1f || tag as u16 == 0x102 {
                for row in rows.chunks_exact(u16b(i, 8)) {
                    let off = u16b(c, 4);
                    let bit = c[7] as usize;
                    if row[u16b(i, 6) + bit / 8] & (0x80 >> (bit % 8)) != 0 {
                        let hn = u32b(row, off);
                        println!(
                            "  tag={:08x} hn={:x} heap={:?}",
                            tag,
                            hn,
                            h.item(hn).map(|b| b.len())
                        );
                    }
                }
            }
        }
    }
}
