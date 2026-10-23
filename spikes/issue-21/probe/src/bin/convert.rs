use pstfree::{convert::convert, ndb::Pst};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut p = Pst::open(&args[1]).unwrap();
    let ns = p.nodes();
    std::env::set_var(
        "PSTOFF_Q16",
        args.get(3).map(String::as_str).unwrap_or("all"),
    );
    assert!(!std::path::Path::new(&args[2]).exists());
    let r = convert(&mut p, &ns, &args[2], &mut |_, _| {}).unwrap();
    println!(
        "nodes={} dropped={} problems={:?}",
        r.nodes, r.dropped_nodes, r.problems
    );
    assert_eq!(r.dropped_nodes, 0);
    assert!(r.problems.is_empty());
}
