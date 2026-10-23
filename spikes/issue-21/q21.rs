// Q16: rebuild item rows from post-Q12/Q15 PCs. Never write any PC here.
use crate::{
    ltp::{bth_records, Heap},
    ndb::{Node, Pst},
};
use std::{
    collections::{BTreeMap, HashSet},
    sync::OnceLock,
};
static OVERRIDES: OnceLock<BTreeMap<u64, Vec<Vec<u8>>>> = OnceLock::new();
type Props = BTreeMap<u32, Vec<u8>>;
fn u16b(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as usize
}
fn u32b(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn p32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes())
}
pub fn blocks(bid: u64, original: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    OVERRIDES
        .get()
        .and_then(|m| m.get(&bid))
        .cloned()
        .unwrap_or(original)
}
fn fixed(t: u16) -> Option<usize> {
    match t {
        2 => Some(2),
        3 | 4 | 10 => Some(4),
        11 => Some(1),
        5 | 6 | 7 | 0x14 | 0x40 => Some(8),
        _ => None,
    }
}
fn variable(t: u16) -> bool {
    matches!(t, 0x1e | 0x1f | 0x102)
}
fn pc(
    p: &mut Pst,
    n: &Node,
    q12: &BTreeMap<u32, [u8; 8]>,
    q15: &super::q15::Plan,
) -> Result<Props, String> {
    let mut bs = p.node_blocks(n.bid_data)?;
    if let Some(v) = q12.get(&n.nid) {
        bs = super::q12::promote(bs, *v)?;
    }
    if let Some(v) = q15.get(&n.nid) {
        bs = super::q15::promote(bs, v)?;
    }
    let h = Heap::new(bs)?;
    if h.client_sig != 0xbc {
        return Err("not PC".into());
    }
    let (key, rs) = bth_records(&h)?;
    if key != 2 || rs.iter().any(|r| r.len() != 8) {
        return Err("PC record size".into());
    }
    let subs = p.subnodes(n.bid_sub)?;
    let mut out = Props::new();
    for r in rs {
        let id = u16b(&r, 0) as u32;
        let typ = u16b(&r, 2) as u16;
        let hn = u32b(&r, 4);
        let v = if let Some(sz) = fixed(typ).filter(|s| *s <= 4) {
            r[4..4 + sz].to_vec()
        } else if hn == 0 {
            Vec::new()
        } else if hn & 31 == 0 {
            h.item(hn)?.to_vec()
        } else {
            p.node_blocks(subs.get(&hn).ok_or("PC subnode missing")?.data)?
                .concat()
        };
        if out.insert(id << 16 | typ as u32, v).is_some() {
            return Err("duplicate PC tag".into());
        }
    }
    Ok(out)
}
// Explicitly scoped derivations; unsupported/missing properties preserve the cache.
pub fn derive(tag: u32, props: &Props, nid: u32, parent: u32) -> Option<(Vec<u8>, &'static str)> {
    if tag == 0x67f20003 {
        return Some((nid.to_le_bytes().to_vec(), "row_id"));
    }
    if tag == 0x67f10003 {
        return Some((parent.to_le_bytes().to_vec(), "parent_nid"));
    }
    if matches!(
        tag,
        0x0e170003
            | 0x0e300102
            | 0x0e300003
            | 0x0e330014
            | 0x0e340102
            | 0x0e380003
            | 0x0e3c0102
            | 0x0e3d0102
            | 0x67f30003
    ) {
        return None;
    }
    if tag == 0x65c60003 || tag == 0x40760003 {
        return Some((vec![0; 4], "ms_pst_get_zero"));
    }
    if matches!(tag, 0x0e02001f | 0x0e03001f | 0x0e04001f) {
        return Some((
            props.get(&tag).cloned().unwrap_or_default(),
            "ms_pst_display",
        ));
    }
    if tag == 0x0e1b000b {
        if let Some(f) = props.get(&0x0e070003).filter(|v| v.len() == 4) {
            return Some((vec![u8::from(u32b(f, 0) & 0x10 != 0)], "ms_pst_hasattach"));
        }
    }
    // MS-PST 2.5.3.1.1: PC subject may carry two metadata code units.
    if tag == 0x0037001f {
        if let Some(v) = props.get(&tag) {
            if v.len() % 2 != 0 {
                return None;
            }
            if v.starts_with(&[1, 0]) {
                if v.len() < 4 {
                    return None;
                }
                let prefix = u16b(v, 2);
                if prefix == 0 || 2 + (prefix - 1) > v.len() / 2 {
                    return None;
                }
                return Some((v[4..].to_vec(), "ms_pst_subject_without_pc_metadata"));
            }
            return Some((v.clone(), "ms_pst_subject_plain"));
        }
    }
    if let Some(v) = props.get(&tag) {
        return Some((v.clone(), "exact_pc"));
    }
    if tag == 0x30130102 && props.get(&0x3016000b).is_some_and(|v| v == &[1]) {
        if let Some(ix) = props
            .get(&0x00710102)
            .filter(|v| v.len() >= 22 && v[0] == 1)
        {
            return Some((ix[6..22].to_vec(), "ms_pst_5_4_index"));
        }
    }
    None
}
fn repack(
    blocks: Vec<Vec<u8>>,
    mut repl: BTreeMap<u32, Vec<u8>>,
    extras: Vec<Vec<u8>>,
) -> Result<Vec<Vec<u8>>, String> {
    let h = Heap::new(blocks.clone())?;
    let last = blocks.len() - 1;
    let mut result = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        let map = u16b(b, 0);
        let count = u16b(b, map);
        let start = u16b(b, map + 4);
        let extra = if bi == last { extras.len() } else { 0 };
        if count + extra > 2047 {
            return Err("HID overflow".into());
        }
        let mut data = b[..start].to_vec();
        let mut offsets = Vec::new();
        let mut free = 0u16;
        for ai in 1..=count + extra {
            offsets.push(data.len());
            let hid = (bi as u32) << 16 | (ai as u32) << 5;
            let v = if ai > count {
                extras[ai - count - 1].clone()
            } else {
                repl.remove(&hid).unwrap_or(h.item(hid)?.to_vec())
            };
            if v.is_empty() {
                free += 1
            }
            data.extend(v);
        }
        offsets.push(data.len());
        if data.len() % 2 != 0 {
            data.push(0)
        }
        let map = data.len();
        data.extend(((count + extra) as u16).to_le_bytes());
        data.extend(free.to_le_bytes());
        for o in offsets {
            data.extend((o as u16).to_le_bytes());
        }
        if data.len() > 8176 {
            return Err("Q16 requires heap relocation".into());
        }
        data[..2].copy_from_slice(&(map as u16).to_le_bytes());
        result.push(data);
    }
    if !repl.is_empty() {
        return Err("replacement HID missing".into());
    }
    Ok(result)
}
pub fn plan(
    p: &mut Pst,
    ns: &[Node],
    q12: &BTreeMap<u32, [u8; 8]>,
    q15: &super::q15::Plan,
    mode: &str,
) -> Result<(), String> {
    let diagnostic: HashSet<u32> = if let Some(s) = mode.strip_prefix("diagnostic-clear:") {
        s.split(',')
            .map(|s| u32::from_str_radix(s, 16).map_err(|_| "bad diagnostic tag"))
            .collect::<Result<_, _>>()?
    } else {
        HashSet::new()
    };
    if diagnostic
        .iter()
        .any(|t| !matches!(*t, 0x0e170003 | 0x66120003 | 0x66280014))
    {
        return Err("diagnostic tag outside bounded cache-only set".into());
    }
    let only: Option<HashSet<u32>> = if let Some(s) = mode.strip_prefix("only:") {
        Some(
            s.split(',')
                .map(|s| u32::from_str_radix(s, 16).map_err(|_| "invalid tag"))
                .collect::<Result<_, _>>()?,
        )
    } else if mode == "all" || mode == "shape" || mode.starts_with("diagnostic-clear:") {
        None
    } else {
        return Err("unknown Q16 mode".into());
    };
    let mut pcs = BTreeMap::new();
    for n in ns.iter().filter(|n| matches!(n.nid_type(), 4 | 8)) {
        pcs.insert(n.nid, (n.nid_parent, pc(p, n, q12, q15)?));
    }
    let mut out = BTreeMap::new();
    for n in ns.iter().filter(|n| matches!(n.nid_type(), 14 | 15 | 16)) {
        let source = p.node_blocks(n.bid_data)?;
        let sh = Heap::new(source.clone())?;
        let si = sh.item(sh.user_root)?;
        if si.len() < 22 {
            return Err("short TC".into());
        }
        let rm = u32b(si, 14);
        if rm == 0 {
            continue;
        }
        // Inherited validator verifies row-index/row identities/counts before edits.
        let external = if rm & 31 != 0 {
            let subs = p.subnodes(n.bid_sub)?;
            Some(p.node_blocks(subs.get(&rm).ok_or("missing matrix")?.data)?)
        } else {
            None
        };
        super::q12::observations(source.clone(), external.clone())?;
        let blocks = if n.nid_type() == 14 {
            super::pst_tc_schema(source)?
        } else {
            source
        };
        let h = Heap::new(blocks.clone())?;
        let i = h.item(h.user_root)?;
        let width = u16b(i, 8);
        let ceb = u16b(i, 6);
        let cols: Vec<_> = i[22..].chunks_exact(8).collect();
        let mut bits = HashSet::new();
        let mut bytes = HashSet::new();
        for c in &cols {
            let off = u16b(c, 4);
            let sz = c[6] as usize;
            let bit = c[7] as usize;
            if off + sz > ceb
                || ceb + bit / 8 >= width
                || !bits.insert(bit)
                || !(off..off + sz).all(|o| bytes.insert(o))
            {
                return Err("Q16 column overlap/bounds".into());
            }
        }
        let mut rows = if let Some(bs) = &external {
            bs.concat()
        } else {
            h.item(rm)?.to_vec()
        };
        let last = blocks.len() - 1;
        let count = u16b(&blocks[last], u16b(&blocks[last], 0));
        let mut extras = Vec::new();
        for r in rows.chunks_exact_mut(width) {
            let nid = u32b(r, 0);
            let Some((parent, props)) = pcs.get(&nid) else {
                return Err("Q16 row missing item PC".into());
            };
            if (only.is_some() || mode == "shape" || !diagnostic.is_empty())
                && !matches!(nid, 0x200084 | 0x2000e4 | 0x200184)
            {
                continue;
            }
            for c in &cols {
                let tag = u32b(c, 0);
                if only.as_ref().is_some_and(|s| !s.contains(&tag)) {
                    continue;
                }
                let typ = tag as u16;
                let off = u16b(c, 4);
                let sz = c[6] as usize;
                let bit = c[7] as usize;
                let mask = 0x80 >> (bit % 8);
                let present = r[ceb + bit / 8] & mask != 0;
                if diagnostic.contains(&tag) {
                    r[ceb + bit / 8] &= !mask;
                    println!("Q16_DIAGNOSTIC table={:08x} row={:08x} tag={:08x} action=clear_presence_only was_present={present}",n.nid,nid,tag);
                    continue;
                }
                let Some((v, reason)) = derive(tag, props, nid, *parent) else {
                    println!("Q16 table={:08x} row={:08x} tag={:08x} action=retain_cached reason=no_supported_derivation present={present}",n.nid,nid,tag);
                    continue;
                };
                if !(variable(typ) && sz == 4 || fixed(typ) == Some(sz) && v.len() == sz) {
                    println!("Q16 table={:08x} row={:08x} tag={:08x} action=retain_cached reason=unsupported_type_size present={present}",n.nid,nid,tag);
                    continue;
                }
                let old = if !present {
                    None
                } else if variable(typ) {
                    let hn = u32b(r, off);
                    Some(if hn == 0 {
                        Vec::new()
                    } else if hn & 31 == 0 {
                        h.item(hn)?.to_vec()
                    } else {
                        let subs = p.subnodes(n.bid_sub)?;
                        p.node_blocks(subs.get(&hn).ok_or("TC cell subnode missing")?.data)?
                            .concat()
                    })
                } else {
                    Some(r[off..off + sz].to_vec())
                };
                let differs = old.as_ref() != Some(&v);
                // Allocate changed variable cells independently, never overwrite shared/structural allocations.
                if differs {
                    if variable(typ) {
                        if v.is_empty() {
                            p32(r, off, 0)
                        } else {
                            extras.push(v);
                            p32(
                                r,
                                off,
                                (last as u32) << 16 | ((count + extras.len()) as u32) << 5,
                            )
                        }
                    } else {
                        r[off..off + sz].copy_from_slice(&v)
                    }
                    r[ceb + bit / 8] |= mask;
                }
                println!("Q16 table={:08x} row={:08x} tag={:08x} action=regenerate reason={reason} differed={differs}",n.nid,nid,tag);
            }
        }
        let mut repl = BTreeMap::new();
        if mode == "shape"
            && rows
                .chunks_exact(width)
                .any(|r| matches!(u32b(r, 0), 0x200084 | 0x2000e4 | 0x200184))
        {
            let donor = std::fs::read("/output/donor-columns.bin").map_err(|e| e.to_string())?;
            if donor.len() % 8 != 0 {
                return Err("donor descriptors".into());
            }
            let ranks: BTreeMap<_, _> = donor
                .chunks_exact(8)
                .map(|c| (u32b(c, 0), c[7] as usize))
                .collect();
            let mut order: Vec<_> = (0..cols.len()).collect();
            order.sort_by_key(|ix| {
                (
                    ranks
                        .get(&u32b(cols[*ix], 0))
                        .copied()
                        .unwrap_or(256 + cols[*ix][7] as usize),
                    u32b(cols[*ix], 0),
                )
            });
            let mut nc: Vec<Vec<u8>> = cols.iter().map(|c| c.to_vec()).collect();
            let mut off = 8;
            let mut ends = [0; 3];
            for size in [8, 4, 2, 1] {
                for ix in &order {
                    let c = &mut nc[*ix];
                    if c[6] as usize != size || matches!(u32b(c, 0) >> 16, 0x67f2 | 0x67f3) {
                        continue;
                    }
                    c[4..6].copy_from_slice(&(off as u16).to_le_bytes());
                    off += size;
                }
                if size == 4 {
                    ends[0] = off
                } else if size == 2 {
                    ends[1] = off
                } else if size == 1 {
                    ends[2] = off
                }
            }
            let newceb = off;
            let newwidth = off + cols.len().div_ceil(8);
            let mut bit = 2u8;
            for ix in &order {
                let c = &mut nc[*ix];
                c[7] = match u32b(c, 0) >> 16 {
                    0x67f2 => 0,
                    0x67f3 => 1,
                    _ => {
                        let b = bit;
                        bit += 1;
                        b
                    }
                };
            }
            let mut nr = Vec::new();
            for row in rows.chunks_exact(width) {
                let mut r = vec![0; newwidth];
                for (old, new) in cols.iter().zip(&nc) {
                    let o = u16b(old, 4);
                    let d = u16b(new, 4);
                    let sz = old[6] as usize;
                    r[d..d + sz].copy_from_slice(&row[o..o + sz]);
                    let b = old[7] as usize;
                    if row[ceb + b / 8] & (0x80 >> (b % 8)) != 0 {
                        let b = new[7] as usize;
                        r[newceb + b / 8] |= 0x80 >> (b % 8)
                    }
                }
                nr.extend(r)
            }
            rows = nr;
            let mut info = i[..22].to_vec();
            for j in 0..3 {
                info[2 + j * 2..4 + j * 2].copy_from_slice(&(ends[j] as u16).to_le_bytes())
            }
            info[8..10].copy_from_slice(&(newwidth as u16).to_le_bytes());
            info.extend(nc.concat());
            repl.insert(h.user_root, info);
            println!("Q16_SHAPE table={:08x} source_columns={} donor_intersection={} all_source_columns_retained=true",n.nid,cols.len(),cols.iter().filter(|c|ranks.contains_key(&u32b(c,0))).count());
        }

        if let Some(bs) = external {
            let sub = p.subnodes(n.bid_sub)?[&rm];
            let mut off = 0;
            let mut rebuilt = Vec::new();
            for b in bs {
                rebuilt.push(rows[off..off + b.len()].to_vec());
                off += b.len();
            }
            assert_eq!(off, rows.len());
            out.insert(sub.data, rebuilt);
        } else {
            repl.insert(rm, rows);
        }
        out.insert(n.bid_data, repack(blocks, repl, extras)?);
    }
    OVERRIDES.set(out).map_err(|_| "already planned".into())
}
