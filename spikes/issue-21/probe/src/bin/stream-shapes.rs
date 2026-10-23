use pstfree::{ltp::Heap, ndb::Pst};
fn main() {
    let mut p = Pst::open(&std::env::args().nth(1).unwrap()).unwrap();
    let mut heaps = 0;
    let mut oversized = 0;
    for n in p.nodes() {
        if n.bid_data == 0 {
            continue;
        }
        let b = p.node_blocks(n.bid_data).unwrap();
        if Heap::new(b.clone()).is_ok() {
            heaps += 1;
            oversized += b.iter().filter(|b| b.len() > 8176).count();
        }
        if matches!(
            n.nid,
            0x218e | 0x21ce | 0x80090 | 0x200084 | 0x2000e4 | 0x200184
        ) {
            println!(
                "nid={:08x} block_lengths={:?}",
                n.nid,
                b.iter().map(|b| b.len()).collect::<Vec<_>>()
            );
        }
    }
    println!("top_level_heaps={heaps} oversized_top_level_heap_blocks={oversized}");
}
