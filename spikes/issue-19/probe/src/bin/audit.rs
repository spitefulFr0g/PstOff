// Conversion-output regression: bytes and graph preservation plus SCANPST fault seams.
use pstfree::{
    ltp::{read_node_pc, read_tc, Heap},
    ndb::Pst,
};
fn u16b(b: &[u8], p: usize) -> usize {
    u16::from_le_bytes(b[p..p + 2].try_into().unwrap()) as usize
}
fn u32b(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
fn blocks(p: &mut Pst, b: u64) -> Vec<Vec<u8>> {
    if b == 0 {
        vec![]
    } else {
        p.node_blocks(b).unwrap()
    }
}
fn subs(a: &mut Pst, ab: u64, b: &mut Pst, bb: u64, depth: usize) -> usize {
    assert!(depth < 100);
    let aa = a.subnodes(ab).unwrap();
    let ba = b.subnodes(bb).unwrap();
    assert_eq!(aa.len(), ba.len());
    let mut count = 0;
    for (id, x) in aa {
        let y = ba.get(&id).unwrap();
        assert_eq!(
            blocks(a, x.data),
            blocks(b, y.data),
            "subnode bytes changed"
        );
        count += 1 + subs(a, x.sub, b, y.sub, depth + 1)
    }
    count
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut a = Pst::open(&args[1]).unwrap();
    let mut b = Pst::open(&args[2]).unwrap();
    let an = a.nodes();
    let bn = b.nodes();
    assert_eq!(an.len(), bn.len());
    let mut subcount = 0;
    let mut streamcount = 0;
    let mut rowcount = 0;
    let mut missing = 0;
    let mut aliases = 0;
    let mut tables = 0;
    let mut no_timestamp = 0;
    let mut cached_timestamp = 0;
    for x in &an {
        let y = bn.iter().find(|y| y.nid == x.nid).unwrap();
        assert_eq!(x.nid_parent, y.nid_parent);
        subcount += subs(&mut a, x.bid_sub, &mut b, y.bid_sub, 0);
        if x.nid != 0x21 && x.nid_type() != 14 && !matches!(x.nid_type(),4|8) {
            assert_eq!(
                blocks(&mut a, x.bid_data),
                blocks(&mut b, y.bid_data),
                "unchanged stream {:x}",
                x.nid
            );
            streamcount += 1
        }
        if matches!(x.nid_type(), 4 | 8) {
            let pc = read_node_pc(&mut b, y).unwrap();
            let source_pc=read_node_pc(&mut a,x).unwrap();
            for (id,v) in &source_pc.props { assert_eq!(format!("{:?}",v),format!("{:?}",pc.props.get(id).unwrap()),"existing item value changed"); }
            // Diagnostic promotions may append properties; existing decoded values above must match.
            if !pc.props.contains_key(&0x3008) {
                no_timestamp += 1
            }
        }
        if matches!(x.nid_type(), 11..=18) {
            let at = read_tc(&mut a, x.bid_data, x.bid_sub).unwrap();
            let bt = read_tc(&mut b, y.bid_data, y.bid_sub).unwrap();
            assert_eq!(
                at.rows
                    .iter()
                    .map(|r| (r.id, format!("{:?}", r.props)))
                    .collect::<Vec<_>>(),
                bt.rows
                    .iter()
                    .map(|r| (r.id, format!("{:?}", r.props)))
                    .collect::<Vec<_>>(),
                "row values changed {:x}",
                x.nid
            );
            rowcount += at.rows.len();
            cached_timestamp += at
                .rows
                .iter()
                .filter(|r| r.props.contains_key(&0x3008))
                .count();
        }
        if x.nid_type() != 14 {
            continue;
        }
        let h = Heap::new(blocks(&mut b, y.bid_data)).unwrap();
        let i = h.item(h.user_root).unwrap();
        let cols: Vec<_> = i[22..22 + i[1] as usize * 8].chunks_exact(8).collect();
        if !cols.iter().any(|c| u32b(c, 0) == 0x0037001f) {
            continue;
        }
        tables += 1;
        for tag in [
            0x0e300102u32,
            0x0e330014,
            0x0e340102,
            0x0e3d0102,
            0x0e3c0102,
            0x0e380003,
        ] {
            if !cols.iter().any(|c| u32b(c, 0) == tag) {
                missing += 1
            }
        }
        let bth = u32b(i, 10);
        let rm = u32b(i, 14);
        let mut used = std::collections::HashSet::from([h.user_root, bth, rm]);
        let bh = h.item(bth).unwrap();
        used.insert(u32b(bh, 4));
        for row in h.item(rm).unwrap().chunks_exact(u16b(i, 8)) {
            for c in &cols {
                if matches!(u32b(c, 0) as u16, 0x1f | 0x1e | 0x102) {
                    let bit = c[7] as usize;
                    if row[u16b(i, 6) + bit / 8] & (0x80 >> (bit % 8)) != 0 {
                        let hid = u32b(row, u16b(c, 4));
                        if hid != 0 && hid & 31 == 0 && !used.insert(hid) {
                            aliases += 1
                        }
                    }
                }
            }
        }
    }
    let header = b.header_bytes().unwrap();
    let badmap = header[384..512].iter().any(|v| *v != 255);
    println!("nodes={} byte_identical_non_contents_non_store_streams={} byte_identical_subnodes={} identical_table_rows={} contents_tables={} missing_columns={} shared_cell_allocations={} bad_header_fp={} message_pcs_without_modtime={} table_rows_with_modtime={}",an.len(),streamcount,subcount,rowcount,tables,missing,aliases,badmap,no_timestamp,cached_timestamp);
    assert_eq!(no_timestamp,0,"Q12 output must have every eligible timestamp");
    assert!(
        missing == 0 && aliases == 0 && !badmap,
        "writer structural regression"
    );
}
