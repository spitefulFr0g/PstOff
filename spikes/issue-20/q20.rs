// Representation-only trials: preserve all present cell bytes, rows and columns.
use crate::{
    ltp::Heap,
    ndb::{Node, Pst},
};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;
static OVERRIDES: OnceLock<BTreeMap<u64, Vec<Vec<u8>>>> = OnceLock::new();
fn u16b(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as usize
}
fn u32b(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn p16(b: &mut [u8], o: usize, v: usize) {
    b[o..o + 2].copy_from_slice(&(v as u16).to_le_bytes())
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
pub fn plan(p: &mut Pst, ns: &[Node], trial: &str) -> Result<(), String> {
    if ![
        "repack",
        "clone-all",
        "canonical-bits",
        "canonical-layout",
        "canonical-all",
        "zero-absent",
        "move-row-matrix",
        "move-row-index",
        "cells-next-page",
        "cells-next-page-padded",
    ]
    .contains(&trial)
    {
        return Err("unknown representation trial".into());
    }
    let mut out = BTreeMap::new();
    for n in ns.iter().filter(|n| matches!(n.nid_type(), 14 | 15 | 16)) {
        let blocks = p.node_blocks(n.bid_data)?;
        let h = Heap::new(blocks.clone())?;
        let mut info = h.item(h.user_root)?.to_vec();
        let rm = u32b(&info, 14);
        if rm == 0 || rm & 31 != 0 {
            continue;
        }
        let width = u16b(&info, 8);
        let ceb = u16b(&info, 6);
        let mut rows = h.item(rm)?.to_vec();
        if !rows
            .chunks_exact(width)
            .any(|r| matches!(u32b(r, 0), 0x200084 | 0x2000e4 | 0x200184))
        {
            continue;
        }
        let oldcols: Vec<Vec<u8>> = info[22..].chunks_exact(8).map(|c| c.to_vec()).collect();
        let mut cols = oldcols.clone();
        let mut newwidth = width;
        let mut newceb = ceb;
        if trial == "canonical-layout" || trial == "canonical-all" {
            let mut order: Vec<_> = (0..cols.len()).collect();
            order.sort_by_key(|ix| (std::cmp::Reverse(cols[*ix][6]), u32b(&cols[*ix], 0)));
            let mut off = 8;
            let mut ends = [0usize; 3];
            for size in [8, 4, 2, 1] {
                for ix in order.iter().filter(|ix| {
                    oldcols[**ix][6] as usize == size
                        && !matches!(u32b(&oldcols[**ix], 0) >> 16, 0x67f2 | 0x67f3)
                }) {
                    p16(&mut cols[*ix], 4, off);
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
            newceb = off;
            newwidth = off + cols.len().div_ceil(8);
            let mut next = 2u8;
            for c in &mut cols {
                c[7] = match u32b(c, 0) >> 16 {
                    0x67f2 => 0,
                    0x67f3 => 1,
                    _ => {
                        let bit = next;
                        next += 1;
                        bit
                    }
                };
            }
            for j in 0..3 {
                p16(&mut info, 2 + j * 2, ends[j]);
            }
            p16(&mut info, 8, newwidth);
        } else if trial == "canonical-bits" {
            let mut next = 2u8;
            for c in &mut cols {
                c[7] = match u32b(c, 0) >> 16 {
                    0x67f2 => 0,
                    0x67f3 => 1,
                    _ => {
                        let bit = next;
                        next += 1;
                        bit
                    }
                };
            }
        }
        if matches!(
            trial,
            "canonical-layout" | "canonical-all" | "canonical-bits" | "zero-absent"
        ) {
            let mut changed = Vec::new();
            for r in rows.chunks_exact(width) {
                let mut row = vec![0u8; newwidth];
                for (old, new) in oldcols.iter().zip(&cols) {
                    let bit = old[7] as usize;
                    let present = r[ceb + bit / 8] & (0x80 >> (bit % 8)) != 0;
                    let off = u16b(new, 4);
                    let oldoff = u16b(old, 4);
                    let size = old[6] as usize;
                    if present || trial != "zero-absent" {
                        row[off..off + size].copy_from_slice(&r[oldoff..oldoff + size]);
                    }
                    if present {
                        let bit = new[7] as usize;
                        row[newceb + bit / 8] |= 0x80 >> (bit % 8);
                    }
                }
                changed.extend(row);
            }
            rows = changed;
        }
        let nextpage = trial.starts_with("cells-next-page");
        let last = if nextpage {
            blocks.len()
        } else {
            blocks.len() - 1
        };
        let count = if nextpage {
            0
        } else {
            u16b(&blocks[last], u16b(&blocks[last], 0))
        };
        let mut clones = Vec::new();
        if matches!(
            trial,
            "clone-all" | "canonical-all" | "cells-next-page" | "cells-next-page-padded"
        ) {
            for row in rows.chunks_exact_mut(newwidth) {
                for c in &cols {
                    if !matches!(u32b(c, 0) as u16, 0x1f | 0x1e | 0x102) {
                        continue;
                    }
                    let bit = c[7] as usize;
                    if row[newceb + bit / 8] & (0x80 >> (bit % 8)) == 0 {
                        continue;
                    }
                    let off = u16b(c, 4);
                    let hid = u32b(row, off);
                    if hid != 0 && hid & 31 == 0 {
                        clones.push(h.item(hid)?.to_vec());
                        p32(
                            row,
                            off,
                            ((last as u32) << 16) | (((count + clones.len()) as u32) << 5),
                        );
                    }
                }
            }
        }
        info.truncate(22);
        info.extend(cols.concat());
        let mut repl = BTreeMap::new();
        if trial == "move-row-matrix" {
            clones.push(rows.clone());
            let newrm = ((last as u32) << 16) | (((count + clones.len()) as u32) << 5);
            p32(&mut info, 14, newrm);
        } else {
            repl.insert(rm, rows);
        }
        if trial == "move-row-index" {
            let bth = u32b(&info, 10);
            let mut bh = h.item(bth)?.to_vec();
            let index = u32b(&bh, 4);
            clones.push(h.item(index)?.to_vec());
            let newindex = ((last as u32) << 16) | (((count + clones.len()) as u32) << 5);
            p32(&mut bh, 4, newindex);
            repl.insert(bth, bh);
        }
        repl.insert(h.user_root, info);
        // Prove each live allocation is still addressed, and no allocation count overflows.
        let mut seen = HashSet::new();
        let mut rebuilt = Vec::new();
        for (bi, b) in blocks.iter().enumerate() {
            let map = u16b(b, 0);
            let count = u16b(b, map);
            let start = u16b(b, map + 4);
            let extra = if bi == last { clones.len() } else { 0 };
            assert!(count + extra <= 2047);
            let mut data = b[..start].to_vec();
            let mut offsets = Vec::new();
            let mut free = 0u16;
            for ai in 1..=count + extra {
                offsets.push(data.len());
                let hid = ((bi as u32) << 16) | ((ai as u32) << 5);
                let v = if ai > count {
                    clones[ai - count - 1].clone()
                } else {
                    repl.remove(&hid).unwrap_or(h.item(hid)?.to_vec())
                };
                if v.is_empty() {
                    free += 1
                }
                data.extend(v);
                seen.insert(hid);
            }
            offsets.push(data.len());
            if data.len() % 2 != 0 {
                data.push(0)
            }
            let map = data.len();
            data.extend(((count + extra) as u16).to_le_bytes());
            data.extend(free.to_le_bytes());
            for off in offsets {
                data.extend((off as u16).to_le_bytes())
            }
            if data.len() > 8176 {
                return Err("trial requires relocation".into());
            }
            p16(&mut data, 0, map);
            rebuilt.push(data);
        }
        if nextpage {
            let mut data = vec![0u8; 2];
            let mut offsets = Vec::new();
            for v in &clones {
                offsets.push(data.len() as u16);
                data.extend(v);
            }
            offsets.push(data.len() as u16);
            if data.len() % 2 != 0 {
                data.push(0)
            }
            let map = data.len();
            data.extend((clones.len() as u16).to_le_bytes());
            data.extend(0u16.to_le_bytes());
            for o in offsets {
                data.extend(o.to_le_bytes())
            }
            p16(&mut data, 0, map);
            assert!(data.len() <= 8176);
            rebuilt.push(data);
        }
        if trial == "cells-next-page-padded" {
            let last = rebuilt.len() - 1;
            for b in &mut rebuilt[..last] {
                b.resize(8176, 0);
            }
        }
        assert!(repl.is_empty());
        out.insert(n.bid_data, rebuilt);
    }
    OVERRIDES.set(out).map_err(|_| "already planned".into())
}
