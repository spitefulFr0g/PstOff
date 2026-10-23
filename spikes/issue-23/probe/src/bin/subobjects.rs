use pstfree::{
    convert::q22,
    ltp::{read_tc, Heap},
    ndb::{Node, Pst},
};
use std::collections::BTreeSet;
fn u16b(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as usize
}
fn u32b(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn size(p: &mut Pst, bid: u64, sub: u64) -> usize {
    let mut total = if bid == 0 {
        0
    } else {
        p.node_blocks(bid).unwrap().iter().map(Vec::len).sum()
    };
    for (_, s) in p.subnodes(sub).unwrap() {
        total += size(p, s.data, s.sub);
    }
    total
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mut p = Pst::open(&args[1]).unwrap();
    for n in p
        .nodes()
        .iter()
        .filter(|n| matches!(n.nid, 0x692 | 0x671 | 0x200084 | 0x2000e4 | 0x200184))
    {
        if matches!(n.nid, 0x692 | 0x671) {
            tc(&mut p, n.nid, n.nid, n.bid_data, n.bid_sub);
            continue;
        }
        let props = q22::pc(&mut p, n).unwrap();
        let subs = p.subnodes(n.bid_sub).unwrap();
        println!("{{\"kind\":\"message\",\"nid\":\"{:08x}\",\"subnodes\":{},\"stream_and_subnode_bytes\":{},\"size_matches_stream_sum\":{}}}",n.nid,subs.len(),size(&mut p,n.bid_data,n.bid_sub),props.get(&0x0e080003).is_some_and(|v|u32b(v,0) as usize==size(&mut p,n.bid_data,n.bid_sub)));
        let mut displays = [Vec::<String>::new(), Vec::new(), Vec::new()];
        let mut attach = 0;
        for (id, s) in &subs {
            let bs = p.node_blocks(s.data).unwrap();
            let sig = bs.first().and_then(|b| b.get(3)).copied().unwrap_or(0);
            println!("{{\"kind\":\"subnode\",\"message\":\"{:08x}\",\"nid\":\"{:08x}\",\"type\":{},\"client_signature\":{},\"block_lengths\":{:?},\"nested_subnodes\":{}}}",n.nid,id,id&31,sig,bs.iter().map(Vec::len).collect::<Vec<_>>(),p.subnodes(s.sub).unwrap().len());
            if sig == 0x7c {
                tc(&mut p, n.nid, *id, s.data, s.sub);
                let t = read_tc(&mut p, s.data, s.sub).unwrap();
                if id & 31 == 0x12 {
                    for r in &t.rows {
                        let rt = r.int(0x0c15).unwrap_or(0);
                        if (1..=3).contains(&rt) {
                            displays[rt as usize - 1].push(r.str(0x3001).unwrap_or("").to_string());
                        }
                    }
                }
                if id & 31 == 0x11 {
                    attach = t.rows.len();
                }
            }
            if id & 31 == 5 {
                let sn = Node {
                    nid: *id,
                    nid_parent: 0,
                    bid_data: s.data,
                    bid_sub: s.sub,
                };
                let ap = q22::pc(&mut p, &sn).unwrap();
                println!(
                    "{{\"kind\":\"attachment_pc\",\"message\":\"{:08x}\",\"properties\":{}}}",
                    n.nid,
                    ap.len()
                );
            }
        }
        let flag = props
            .get(&0x0e070003)
            .map(|v| u32b(v, 0) & 0x10 != 0)
            .unwrap_or(false);
        println!("{{\"kind\":\"attachments\",\"message\":\"{:08x}\",\"table_rows\":{},\"objects\":{},\"flag_agrees\":{},\"pc_hasattach_agrees\":{}}}",n.nid,attach,subs.keys().filter(|k|**k&31==5).count(),flag==(attach>0),props.get(&0x0e1b000b).map(|v|v[0]!=0).unwrap_or(flag)==(attach>0));
        for (ix, tag) in [0x0e04001fu32, 0x0e03001f, 0x0e02001f].iter().enumerate() {
            let encode = |s: String| {
                s.encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>()
            };
            let v = props.get(tag).cloned().unwrap_or_default();
            println!("{{\"kind\":\"display\",\"message\":\"{:08x}\",\"tag\":\"{:08x}\",\"recipients\":{},\"pc_present\":{},\"matches_semicolon\":{},\"matches_semicolon_space\":{}}}",n.nid,tag,displays[ix].len(),props.contains_key(tag),v==encode(displays[ix].join(";")),v==encode(displays[ix].join("; ")));
        }
    }
}
fn tc(p: &mut Pst, parent: u32, id: u32, bid: u64, sub: u64) {
    let bs = p.node_blocks(bid).unwrap();
    let h = Heap::new(bs.clone()).unwrap();
    let info = h.item(h.user_root).unwrap();
    let cols = info[22..].chunks_exact(8).collect::<Vec<_>>();
    let tags = cols.iter().map(|c| u32b(c, 0)).collect::<BTreeSet<_>>();
    let required: Vec<u32> = if id & 31 == 0x12 {
        vec![
            0x0c150003, 0x0e0f000b, 0x0ff90102, 0x0ffe0003, 0x0fff0102, 0x3001001f, 0x3002001f,
            0x3003001f, 0x300b0102, 0x39000003, 0x39ff001f, 0x3a40000b, 0x67f20003, 0x67f30003,
        ]
    } else if id & 31 == 0x11 {
        vec![
            0x0e200003, 0x3704001f, 0x37050003, 0x370b0003, 0x67f20003, 0x67f30003,
        ]
    } else {
        vec![]
    };
    let missing = required
        .iter()
        .filter(|t| !tags.contains(t))
        .map(|t| format!("\"{t:08x}\""))
        .collect::<Vec<_>>()
        .join(",");
    let width = u16b(info, 8);
    let ceb = u16b(info, 6);
    let rm = u32b(info, 14);
    let data = if rm == 0 {
        vec![]
    } else if rm & 31 == 0 {
        h.item(rm).unwrap().to_vec()
    } else {
        let ss = p.subnodes(sub).unwrap();
        p.node_blocks(ss[&rm].data).unwrap().concat()
    };
    let bth = h.item(u32b(info, 10)).unwrap();
    assert_eq!(&bth[..4], &[0xb5, 4, 4, 0]);
    let index = h.item(u32b(bth, 4)).unwrap();
    let index_valid = index.len() % 8 == 0
        && index.chunks_exact(8).all(|e| {
            let ix = u32b(e, 4) as usize;
            ix * width + width <= data.len() && u32b(&data, ix * width) == u32b(e, 0)
        });
    let mut bits = BTreeSet::new();
    let mut bytes = BTreeSet::new();
    let valid = cols.iter().all(|c| {
        let off = u16b(c, 4);
        let sz = c[6] as usize;
        let bit = c[7] as usize;
        off + sz <= ceb
            && ceb + bit / 8 < width
            && bits.insert(bit)
            && (off..off + sz).all(|o| bytes.insert(o))
    });
    let t = read_tc(p, bid, sub).unwrap();
    println!("{{\"kind\":\"tc\",\"message\":\"{parent:08x}\",\"nid\":\"{id:08x}\",\"columns\":{},\"rows\":{},\"missing_template_columns\":[{}],\"bounds_and_unique_ceb\":{},\"row_index_valid\":{},\"matrix_integral_rows\":{},\"index_rows\":{},\"row_ids\":{:?},\"heap_blocks\":{},\"external_matrix\":{}}}",cols.len(),t.rows.len(),missing,valid,index_valid,data.len()%width==0,index.len()/8,t.rows.iter().map(|r|r.id).collect::<Vec<_>>(),bs.len(),rm&31!=0);
}
