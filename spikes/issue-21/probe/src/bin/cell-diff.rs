// Sanitized semantic difference counts and property tags only.
use pstfree::{
    ltp::{read_node_pc, read_tc},
    ndb::Pst,
};
use std::collections::BTreeMap;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut a = Pst::open(&args[1]).unwrap();
    let mut b = Pst::open(&args[2]).unwrap();
    let ns = a.nodes();
    let bs = b.nodes();
    let (
        mut pcs,
        mut props,
        mut rows,
        mut same_rows,
        mut same_cells,
        mut changed,
        mut added,
        mut removed,
    ) = (
        0,
        0,
        0,
        0,
        0,
        BTreeMap::<u16, usize>::new(),
        BTreeMap::<u16, usize>::new(),
        BTreeMap::<u16, usize>::new(),
    );
    for n in &ns {
        let m = bs.iter().find(|m| m.nid == n.nid).unwrap();
        if n.nid == 0x21 || matches!(n.nid_type(), 2 | 3 | 4 | 8) {
            let aa = read_node_pc(&mut a, n).unwrap();
            let bb = read_node_pc(&mut b, m).unwrap();
            assert_eq!(
                format!("{:?}", aa.props),
                format!("{:?}", bb.props),
                "PC properties changed"
            );
            pcs += 1;
            props += aa.props.len();
        }
        if matches!(n.nid_type(), 11..=18) {
            let aa = read_tc(&mut a, n.bid_data, n.bid_sub).unwrap();
            let bb = read_tc(&mut b, m.bid_data, m.bid_sub).unwrap();
            assert_eq!(aa.rows.len(), bb.rows.len());
            for x in &aa.rows {
                let y = bb.rows.iter().find(|y| y.id == x.id).unwrap();
                rows += 1;
                if format!("{:?}", x.props) == format!("{:?}", y.props) {
                    same_rows += 1
                }
                for (id, v) in &x.props {
                    match y.props.get(id) {
                        None => *removed.entry(*id).or_default() += 1,
                        Some(w) if format!("{v:?}") != format!("{w:?}") => {
                            *changed.entry(*id).or_default() += 1
                        }
                        _ => same_cells += 1,
                    }
                }
                for id in y.props.keys() {
                    if !x.props.contains_key(id) {
                        *added.entry(*id).or_default() += 1
                    }
                }
            }
        }
    }
    let tags = |m: &BTreeMap<u16, usize>| {
        m.iter()
            .map(|(t, c)| format!("\"{t:04x}\":{c}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    println!("{{\"identical_pcs\":{pcs},\"identical_properties\":{props},\"row_ids_preserved\":{rows},\"identical_rows\":{same_rows},\"identical_existing_cells\":{same_cells},\"changed_cells_by_id\":{{{}}},\"added_cells_by_id\":{{{}}},\"removed_cells_by_id\":{{{}}}}}",tags(&changed),tags(&added),tags(&removed));
}
