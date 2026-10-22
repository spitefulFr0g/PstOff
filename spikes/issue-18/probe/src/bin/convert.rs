use pstfree::{convert::convert, ndb::Pst};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut p = Pst::open(&args[1]).unwrap();
    let nodes = p.nodes();
    let r = convert(&mut p, &nodes, &args[2], &mut |_, _| {}).unwrap();
    println!(
        "nodes={} dropped={} problems={:?}",
        r.nodes, r.dropped_nodes, r.problems
    );
    assert_eq!(r.dropped_nodes, 0);
}
