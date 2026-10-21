// Same-candidate diagnostic only; no claim of independent semantic validation.
use pstfree::{ndb::Pst, ltp::{read_node_pc, read_tc}};
fn main() {
    let path = std::env::args().nth(1).expect("input required");
    let mut pst = Pst::open(&path).unwrap();
    for node in pst.nodes() {
        if node.nid == 0x21 || matches!(node.nid_type(), 0x02 | 0x03 | 0x04 | 0x08) {
            match read_node_pc(&mut pst, &node) {
                Ok(pc) => {
                    println!("PC {:08x} type={} props={} from_subnode={}", node.nid, node.nid_type(), pc.props.len(), pc.from_subnode);
                    for (id, value) in pc.props {
                        println!("VALUE {:08x}:{:04x} {:?}", node.nid, id, value);
                    }
                },
                Err(e) => println!("PC_ERROR {:08x} {}", node.nid, e),
            }
        } else if matches!(node.nid_type(), 0x0b..=0x12) {
            match read_tc(&mut pst, node.bid_data, node.bid_sub) {
                Ok(tc) => {
                    println!("TC {:08x} rows={}", node.nid, tc.rows.len());
                    for row in tc.rows {
                        println!("TCROW {:08x}:{:08x} {:?}", node.nid, row.id, row.props);
                    }
                },
                Err(e) => println!("TC_ERROR {:08x} {}", node.nid, e),
            }
        }
    }
}
