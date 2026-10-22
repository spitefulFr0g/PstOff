use pstfree::{convert::convert, ndb::Pst};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len()>3 { if args[3].starts_with("clear:") {} else if args[3]=="status" {std::env::set_var("PSTOFF_DIAG_STATUS","1");} else {std::env::set_var("PSTOFF_DIAG_CLEAR",&args[3]);} }
    let mut p = Pst::open(&args[1]).unwrap();
    let nodes = p.nodes();
    if args.len()>3 && args[3].starts_with("clear:") {let tags:Vec<_>=args[3][6..].split(',').map(|s|u32::from_str_radix(s,16).unwrap()).collect();pstfree::convert::q19::plan(&mut p,&nodes,&tags,args.get(4).is_some_and(|s|s.split(',').any(|s|s=="align0042")),args.get(4).is_some_and(|s|s.split(',').any(|s|s=="flags")),&args.get(4).map(|s|s.split(',').filter_map(|s|s.strip_prefix("zero:")).map(|s|u32::from_str_radix(s,16).unwrap()).collect::<Vec<_>>()).unwrap_or_default(),args.get(4).is_some_and(|s|s.split(',').any(|s|s=="promote")),args.get(4).is_some_and(|s|s.split(',').any(|s|s=="align3013")),args.get(4).and_then(|s|s.split(',').find_map(|s|s.strip_prefix("promote:")).map(|s|u16::from_str_radix(s,16).unwrap())),&args.get(4).map(|s|s.split(',').filter_map(|s|s.strip_prefix("remove:")).map(|s|u32::from_str_radix(s,16).unwrap()).collect::<Vec<_>>()).unwrap_or_default(),args.get(4).and_then(|s|s.split(',').find_map(|s|s.strip_prefix("item:")).map(|s|u32::from_str_radix(s,16).unwrap()))).unwrap();}
    let r = convert(&mut p, &nodes, &args[2], &mut |_, _| {}).unwrap();
    println!(
        "nodes={} dropped={} problems={:?}",
        r.nodes, r.dropped_nodes, r.problems
    );
    assert_eq!(r.dropped_nodes, 0);
}
