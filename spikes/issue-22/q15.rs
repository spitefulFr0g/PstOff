// Accepted Q15 only: validate all same-NID observations; preserve existing PCs.
use crate::{
    ltp::{bth_records, Heap},
    ndb::{Node, Pst},
};
use std::collections::BTreeMap;
pub type Pair = [Vec<u8>; 2];
pub type Plan = BTreeMap<u32, BTreeMap<u16, Vec<u8>>>;
fn u16b(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as usize
}
fn u32b(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub fn valid_pair(p: &Pair) -> bool {
    if !(17..=24).contains(&p[0].len()) {
        return false;
    }
    let mut off = 0;
    let mut contains = false;
    let mut previous: Option<&[u8]> = None;
    while off < p[1].len() {
        let len = p[1][off] as usize;
        off += 1;
        if !(17..=24).contains(&len) || off + len > p[1].len() {
            return false;
        }
        let guid = &p[1][off..off + 16];
        if previous.is_some_and(|old| old >= guid) {
            return false;
        }
        previous = Some(guid);
        contains |= p[1][off..off + len] == p[0];
        off += len;
    }
    contains
}
pub fn select(
    existing: &[Option<(u16, Vec<u8>)>; 2],
    obs: &[Result<Pair, String>],
    tables_valid: bool,
) -> Result<BTreeMap<u16, Vec<u8>>, &'static str> {
    if !tables_valid {
        return Err("unresolved_table_integrity");
    }
    if obs.is_empty() {
        return Err("unresolved_missing_cache");
    }
    if obs
        .iter()
        .any(|o| o.as_ref().map_or(true, |p| !valid_pair(p)))
    {
        return Err("unresolved_invalid_or_missing_pair");
    }
    let pair = obs[0].as_ref().unwrap();
    if obs.iter().any(|p| p.as_ref().unwrap() != pair) {
        return Err("unresolved_conflicting_occurrences");
    }
    for i in 0..2 {
        if existing[i]
            .as_ref()
            .is_some_and(|(typ, v)| *typ != 0x102 || *v != pair[i])
        {
            return Err("unresolved_existing_conflict");
        }
    }
    if existing.iter().all(Option::is_some) {
        return Err("preserved_existing");
    }
    Ok((0..2)
        .filter(|i| existing[*i].is_none())
        .map(|i| ([0x65e2, 0x65e3][i], pair[i].clone()))
        .collect())
}
pub fn plan(p: &mut Pst, ns: &[Node]) -> Result<Plan, String> {
    let mut observations: BTreeMap<u32, Vec<(u32, Result<Pair, String>)>> = BTreeMap::new();
    let mut errors = 0;
    for n in ns.iter().filter(|n| matches!(n.nid_type(), 14 | 15 | 16)) {
        let rows = (|| -> Result<Vec<(u32, Result<Pair, String>)>, String> {
            let blocks = p.node_blocks(n.bid_data)?;
            for (bi, b) in blocks.iter().enumerate() {
                if b.len() < 2 {
                    return Err("short heap page".into());
                }
                let map = u16b(b, 0);
                if map + 4 > b.len() {
                    return Err("heap map bounds".into());
                }
                let count = u16b(b, map);
                if count > 2047 || map + 4 + 2 * (count + 1) > b.len() {
                    return Err("heap map length".into());
                }
                let offsets = (0..=count)
                    .map(|i| u16b(b, map + 4 + i * 2))
                    .collect::<Vec<_>>();
                if offsets[0] < (if bi == 0 { 12 } else { 2 })
                    || offsets[count] > map
                    || offsets.windows(2).any(|w| w[0] > w[1])
                {
                    return Err("heap allocation bounds".into());
                }
                if offsets.windows(2).filter(|w| w[0] == w[1]).count() != u16b(b, map + 2) {
                    return Err("heap free count".into());
                }
            }
            let h = Heap::new(blocks.clone())?;
            let info = h.item(h.user_root)?;
            if info.len() < 22 {
                return Err("short TC".into());
            }
            let rm = u32b(info, 14);
            let subs = p.subnodes(n.bid_sub)?;
            let external = if rm & 31 != 0 {
                Some(p.node_blocks(subs.get(&rm).ok_or("missing row matrix")?.data)?)
            } else {
                None
            };
            // Q12's validator verifies header, row-index, same-NID identity, row count, CEB.
            let ids = super::q12::observations(blocks.clone(), external.clone())?;
            let width = u16b(info, 8);
            let ceb = u16b(info, 6);
            let cols: Vec<_> = info[22..].chunks_exact(8).collect();
            if !cols
                .iter()
                .any(|c| matches!(u32b(c, 0) >> 16, 0x65e2 | 0x65e3))
            {
                return Ok(Vec::new());
            }
            let mut bits = std::collections::HashSet::new();
            let mut bytes = std::collections::HashSet::new();
            if cols.windows(2).any(|w| u32b(w[0], 0) >= u32b(w[1], 0)) {
                return Err("column order".into());
            }
            for c in &cols {
                let off = u16b(c, 4);
                let size = c[6] as usize;
                let bit = c[7] as usize;
                if off + size > ceb
                    || ceb + bit / 8 >= width
                    || !bits.insert(bit)
                    || !(off..off + size).all(|o| bytes.insert(o))
                {
                    return Err("column overlap/bounds/CEB".into());
                }
            }
            let data = if let Some(e) = external {
                e.concat()
            } else {
                h.item(rm)?.to_vec()
            };
            let mut out = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for (nid, _) in ids {
                if !seen.insert(nid) {
                    return Err("duplicate NID".into());
                }
                let row = data
                    .chunks_exact(width)
                    .find(|r| u32b(r, 0) == nid)
                    .ok_or("row identity")?;
                let pair = (|| -> Result<Pair, String> {
                    let mut pair = [Vec::new(), Vec::new()];
                    for (i, id) in [0x65e2, 0x65e3].iter().enumerate() {
                        let cs: Vec<_> = cols.iter().filter(|c| u32b(c, 0) >> 16 == *id).collect();
                        if cs.len() != 1 {
                            return Err("missing/duplicate pair descriptor".into());
                        }
                        let c = cs[0];
                        if u32b(c, 0) != (*id << 16 | 0x102) || c[6] != 4 {
                            return Err("pair type/size".into());
                        }
                        let bit = c[7] as usize;
                        if row[ceb + bit / 8] & (0x80 >> (bit % 8)) == 0 {
                            return Err("missing pair cell".into());
                        }
                        let hnid = u32b(row, u16b(c, 4));
                        if [
                            h.user_root,
                            u32b(info, 10),
                            rm,
                            u32b(h.item(u32b(info, 10))?, 4),
                        ]
                        .contains(&hnid)
                        {
                            return Err("pair aliases structure".into());
                        }
                        pair[i] = if hnid == 0 {
                            return Err("null pair cell".into());
                        } else if hnid & 31 == 0 {
                            h.item(hnid)?.to_vec()
                        } else {
                            p.node_blocks(subs.get(&hnid).ok_or("missing cell subnode")?.data)?
                                .concat()
                        };
                    }
                    if !valid_pair(&pair) {
                        return Err("XID/PCL serialization or pair consistency".into());
                    }
                    Ok(pair)
                })();
                out.push((nid, pair));
            }
            Ok(out)
        })();
        match rows {
            Ok(rows) => {
                for (nid, pair) in rows {
                    observations.entry(nid).or_default().push((n.nid, pair));
                }
            }
            Err(_) => {
                errors += 1;
                println!("Q15_TABLE table={:08x} unresolved_integrity=true", n.nid);
            }
        }
    }
    let mut result = Plan::new();
    for n in ns.iter().filter(|n| matches!(n.nid_type(), 4 | 8)) {
        let h = Heap::new(p.node_blocks(n.bid_data)?)?;
        let (key, records) = bth_records(&h)?;
        if key != 2 || records.iter().any(|r| r.len() != 8) {
            return Err("invalid PC".into());
        }
        let mut existing = [None, None];
        for (i, id) in [0x65e2, 0x65e3].iter().enumerate() {
            if let Some(r) = records.iter().find(|r| u16b(r, 0) == *id) {
                let typ = u16b(r, 2) as u16;
                let hnid = u32b(r, 4);
                let value = if typ == 0x102 {
                    if hnid & 31 == 0 {
                        h.item(hnid)?.to_vec()
                    } else {
                        let subs = p.subnodes(n.bid_sub)?;
                        p.node_blocks(subs.get(&hnid).ok_or("missing PC cell")?.data)?
                            .concat()
                    }
                } else {
                    Vec::new()
                };
                existing[i] = Some((typ, value));
            }
        }
        let obs = observations.get(&n.nid).cloned().unwrap_or_default();
        let values = obs.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>();
        match select(&existing, &values, errors == 0) {
            Ok(props) => {
                println!("Q15 nid={:08x} reason=exact_validated_same_nid_pair added={} source_tables={} occurrences={}",n.nid,props.len(),obs.iter().map(|(id,_)|format!("{:08x}",id)).collect::<Vec<_>>().join(","),obs.len());
                result.insert(n.nid, props);
            }
            Err(reason) => println!("Q15 nid={:08x} reason={reason}", n.nid),
        }
    }
    Ok(result)
}
pub fn promote(
    blocks: Vec<Vec<u8>>,
    props: &BTreeMap<u16, Vec<u8>>,
) -> Result<Vec<Vec<u8>>, String> {
    let h = Heap::new(blocks.clone())?;
    let bh = h.item(h.user_root)?;
    if bh.len() != 8 || bh[..4] != [0xb5, 2, 6, 0] {
        return Err("unsupported PC BTH".into());
    }
    let root = u32b(bh, 4);
    let mut rs: Vec<_> = h.item(root)?.chunks_exact(8).map(|r| r.to_vec()).collect();
    let last = blocks.len() - 1;
    let count = u16b(&blocks[last], u16b(&blocks[last], 0));
    let mut extras = Vec::new();
    for (id, value) in props {
        if ![0x65e2, 0x65e3].contains(id) || rs.iter().any(|r| u16b(r, 0) == *id as usize) {
            return Err("Q15 existing/unsupported property".into());
        }
        extras.push(value.clone());
        let hid = ((last as u32) << 16) | (((count + extras.len()) as u32) << 5);
        let mut rec = id.to_le_bytes().to_vec();
        rec.extend(0x102u16.to_le_bytes());
        rec.extend(hid.to_le_bytes());
        rs.push(rec);
    }
    if count + extras.len() > 2047 {
        return Err("HID overflow".into());
    }
    rs.sort_by_key(|r| u16b(r, 0));
    let updated = rs.concat();
    let mut result = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        let map = u16b(b, 0);
        let count = u16b(b, map);
        let start = u16b(b, map + 4);
        let extra = if bi == last { extras.len() } else { 0 };
        let mut out = b[..start].to_vec();
        let mut offsets = Vec::new();
        let mut free = 0u16;
        for ai in 1..=count + extra {
            offsets.push(out.len() as u16);
            let hid = ((bi as u32) << 16) | ((ai as u32) << 5);
            let v = if ai > count {
                &extras[ai - count - 1][..]
            } else if hid == root {
                &updated[..]
            } else {
                h.item(hid)?
            };
            if v.is_empty() {
                free += 1
            }
            out.extend(v)
        }
        offsets.push(out.len() as u16);
        if out.len() % 2 != 0 {
            out.push(0)
        }
        let map = out.len();
        out.extend(((count + extra) as u16).to_le_bytes());
        out.extend(free.to_le_bytes());
        for o in offsets {
            out.extend(o.to_le_bytes())
        }
        if out.len() > 8176 {
            return Err("Q15 requires heap relocation".into());
        }
        out[..2].copy_from_slice(&(map as u16).to_le_bytes());
        result.push(out)
    }
    Ok(result)
}
