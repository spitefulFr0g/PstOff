// Diagnostic-only PC additions: raw values copied from same-item rebuilt rows.
use crate::{
    ltp::{bth_records, Heap},
    ndb::{Node, Pst},
};
use std::collections::BTreeMap;
fn u16b(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as usize
}
fn u32b(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub fn edit_pc(bs: Vec<Vec<u8>>, props: &BTreeMap<u32, Vec<u8>>) -> Result<Vec<Vec<u8>>, String> {
    let h = Heap::new(bs.clone())?;
    let bh = h.item(h.user_root)?;
    if bh[..4] != [0xb5, 2, 6, 0] {
        return Err("unsupported PC tree".into());
    }
    let root = u32b(bh, 4);
    let (_, mut rs) = bth_records(&h)?;
    let last = bs.len() - 1;
    let count = u16b(&bs[last], u16b(&bs[last], 0));
    let mut extra = Vec::new();
    for (tag, v) in props {
        let id = (tag >> 16) as u16;
        let typ = *tag as u16;
        if rs.iter().any(|r| u16b(r, 0) == id as usize) {
            return Err("diagnostic refuses existing PC identifier".into());
        }
        let mut r = id.to_le_bytes().to_vec();
        r.extend(typ.to_le_bytes());
        let inline = match typ {
            2 => Some(2),
            3 | 4 | 10 => Some(4),
            11 => Some(1),
            _ => None,
        };
        if let Some(sz) = inline {
            if v.len() != sz {
                return Err("inline size".into());
            }
            r.extend(v);
            r.resize(8, 0);
        } else {
            extra.push(v.clone());
            r.extend((((last as u32) << 16) | (((count + extra.len()) as u32) << 5)).to_le_bytes());
        }
        rs.push(r);
    }
    rs.sort_by_key(|r| u16b(r, 0));
    super::q21::repack(bs, BTreeMap::from([(root, rs.concat())]), extra)
}
// Deliberately exports raw decoding only inside the sandbox; reports omit values.
pub fn pc(p: &mut Pst, n: &Node) -> Result<BTreeMap<u32, Vec<u8>>, String> {
    super::q21::pc(p, n, &BTreeMap::new(), &BTreeMap::new())
}
