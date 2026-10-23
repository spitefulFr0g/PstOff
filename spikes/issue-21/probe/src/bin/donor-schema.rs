use pstfree::{ltp::Heap, ndb::Pst};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut p = Pst::open(&args[1]).unwrap();
    let ns = p.nodes();
    let n = ns.iter().find(|n| n.nid == 0x812e).unwrap();
    let h = Heap::new(p.node_blocks(n.bid_data).unwrap()).unwrap();
    let i = h.item(h.user_root).unwrap();
    let rm = u32::from_le_bytes(i[14..18].try_into().unwrap());
    let width = u16::from_le_bytes(i[8..10].try_into().unwrap()) as usize;
    assert!(h
        .item(rm)
        .unwrap()
        .chunks_exact(width)
        .any(|r| r[..4] == 0x2000c4u32.to_le_bytes()));
    assert!(!std::path::Path::new(&args[2]).exists());
    std::fs::write(&args[2], &i[22..]).unwrap();
    println!("donor_table=0000812e donor_row=002000c4 columns={}", i[1]);
}
