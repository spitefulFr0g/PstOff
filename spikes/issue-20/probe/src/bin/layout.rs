use pstfree::{
    ltp::{read_node_pc, read_tc, Heap},
    ndb::Pst,
};
fn u16b(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as usize
}
fn u32b(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn main() {
    let mut p = Pst::open(&std::env::args().nth(1).unwrap()).unwrap();
    let ns = p.nodes();
    for n in ns.iter().filter(|n| matches!(n.nid_type(), 14 | 15 | 16)) {
        let blocks = p.node_blocks(n.bid_data).unwrap();
        let h = Heap::new(blocks.clone()).unwrap();
        let i = h.item(h.user_root).unwrap();
        let rm = u32b(i, 14);
        if rm == 0 || rm & 31 != 0 {
            continue;
        }
        let width = u16b(i, 8);
        let ceb = u16b(i, 6);
        let tc = read_tc(&mut p, n.bid_data, n.bid_sub).unwrap();
        for r in h.item(rm).unwrap().chunks_exact(width) {
            let nid = u32b(r, 0);
            if ![0x200084, 0x2000e4, 0x200184].contains(&nid) {
                continue;
            }
            let pc = read_node_pc(&mut p, ns.iter().find(|n| n.nid == nid).unwrap()).unwrap();
            let tr = tc.rows.iter().find(|r| r.id == nid).unwrap();
            println!(
                "TABLE {:x} row={:x} blocks={:?} ends={:?} root={:x} bth={:x} rm={:x}",
                n.nid,
                nid,
                blocks.iter().map(|b| b.len()).collect::<Vec<_>>(),
                (2..10).step_by(2).map(|o| u16b(i, o)).collect::<Vec<_>>(),
                h.user_root,
                u32b(i, 10),
                rm
            );
            for c in i[22..].chunks_exact(8) {
                let tag = u32b(c, 0);
                let off = u16b(c, 4);
                let bit = c[7] as usize;
                let present = r[ceb + bit / 8] & (0x80 >> (bit % 8)) != 0;
                let variable = matches!(tag as u16, 0x102 | 0x1f | 0x1e);
                let hnid = if present && variable { u32b(r, off) } else { 0 };
                println!("COL {:08x} off={} cb={} bit={} present={} ref={:x} len={:?} structural_alias={} pc_present={} equal={}",tag,off,c[6],bit,present,hnid,if hnid!=0&&hnid&31==0{h.item(hnid).ok().map(|v|v.len())}else{None},hnid!=0&&[h.user_root,u32b(i,10),rm].contains(&hnid),pc.props.contains_key(&((tag>>16) as u16)),tr.props.get(&((tag>>16) as u16)).zip(pc.props.get(&((tag>>16) as u16))).is_some_and(|(a,b)|format!("{:?}",a)==format!("{:?}",b)));
            }
        }
    }
}
