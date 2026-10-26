//! Source-backed Item properties, planned once before any output is created.
use super::{
    context::{self, Properties},
    table, Result,
};
use crate::{Difference, FolderRef, ItemRef};
use pstoff_format::ndb::{Node, Pst};
use std::collections::BTreeMap;

pub(super) fn prepare(
    source: &mut Pst,
    nodes: &[Node],
    differences: &mut Vec<Difference>,
    cancel: &crate::CancelToken,
    omitted: &std::collections::HashSet<u32>,
) -> Result<Option<BTreeMap<u32, Properties>>> {
    let mut planned = BTreeMap::new();
    let mut observations: BTreeMap<u32, Vec<(u32, bool, bool, Properties)>> = BTreeMap::new();
    for node in nodes
        .iter()
        .filter(|n| n.bid_data != 0 && matches!(n.nid & 31, 0x0e..=0x10))
    {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let table = table::read(source, node.bid_data, node.bid_sub)?;
        for (nid, _, cells) in table.rows {
            if omitted.contains(&nid) {
                continue;
            }
            let item = nodes
                .iter()
                .find(|n| n.nid == nid && matches!(n.nid & 31, 4 | 8))
                .ok_or("Cache row has no indexed Item")?;
            if node.nid & 31 != 0x10 && (node.nid & !31) != (item.nid_parent & !31) {
                return Err("Cache row belongs to another folder".into());
            }
            observations.entry(nid).or_default().push((
                node.nid,
                table.pair_cache,
                table.time_cache,
                cells
                    .into_iter()
                    .map(|(tag, cell)| ((tag >> 16) as u16, cell))
                    .collect(),
            ));
        }
    }
    for node in nodes.iter().filter(|n| matches!(n.nid & 31, 4 | 8)) {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let mut own = context::properties(source, node.bid_data, node.bid_sub)?;
        if let std::collections::btree_map::Entry::Vacant(entry) = own.entry(0x3008) {
            if let Some(all) = observations.get(&node.nid) {
                let rows: Vec<_> = all.iter().filter(|r| r.2).collect();
                if !rows.is_empty() {
                    let first = rows[0].3.get(&0x3008);
                    if let Some(value) = first.filter(|v| {
                        v.kind == 0x40 && v.bytes.len() == 8 && {
                            let time = u64::from_le_bytes(v.bytes[..].try_into().unwrap());
                            time > 0 && time <= i64::MAX as u64
                        }
                    }) {
                        if rows.iter().all(|(_, _, _, c)| {
                            c.get(&0x3008)
                                .is_some_and(|v| v.kind == value.kind && v.bytes == value.bytes)
                        }) {
                            entry.insert(value.clone());
                            differences.push(Difference { property:0x30080040, item:Some(ItemRef {folder:FolderRef(node.nid_parent),node:node.nid}), source:format!("Same Item cache tables {:?}",rows.iter().map(|r| r.0).collect::<Vec<_>>()), reason:"Q12: validated, agreeing source FILETIME; no server-history claim.".into() });
                        } else {
                            return Err(format!("Q12 conflict for Item 0x{:x}", node.nid).into());
                        }
                    } else {
                        return Err(format!(
                            "Q12 invalid or missing cache for Item 0x{:x}",
                            node.nid
                        )
                        .into());
                    }
                }
            }
        }
        if let Some(rows) = observations.get(&node.nid) {
            let pairs: Vec<_> = rows.iter().filter(|(_, pair, _, _)| *pair).collect();
            if let Some((first_table, _, _, first)) = pairs.first() {
                // Keep diagnostics local: identity, table and property tags,
                // never cache payloads, subjects or addresses.
                let diagnostic = |problem: &str, table: u32, id: u16| {
                    format!(
                        "Q15 {problem} for Folder 0x{:x}, Item 0x{:x}, property 0x{id:04x}0102 in contents table 0x{table:x}; no cache value chosen. Review this Item's cache occurrences and property context before retrying.",
                        node.nid_parent, node.nid
                    )
                };
                let key = first
                    .get(&0x65e2)
                    .ok_or_else(|| diagnostic("missing ChangeKey", *first_table, 0x65e2))?;
                let pcl = first
                    .get(&0x65e3)
                    .ok_or_else(|| diagnostic("missing PCL", *first_table, 0x65e3))?;
                if key.kind != 0x102
                    || pcl.kind != 0x102
                    || !consistent_pair(&key.bytes, &pcl.bytes)
                {
                    return Err(diagnostic("invalid XID/PCL pair", *first_table, 0x65e2).into());
                }
                for (table, _, _, cells) in &pairs {
                    for id in [0x65e2, 0x65e3] {
                        let value = cells
                            .get(&id)
                            .ok_or_else(|| diagnostic("missing pair member", *table, id))?;
                        let expected = &first[&id];
                        if value.kind != expected.kind
                            || value.bytes != expected.bytes
                            || own.get(&id).is_some_and(|v| {
                                v.kind != expected.kind || v.bytes != expected.bytes
                            })
                        {
                            return Err(diagnostic(
                                "conflict with another occurrence or Item property",
                                *table,
                                id,
                            )
                            .into());
                        }
                    }
                }
                for id in [0x65e2, 0x65e3] {
                    if let std::collections::btree_map::Entry::Vacant(entry) = own.entry(id) {
                        entry.insert(first[&id].clone());
                        differences.push(Difference {
                            property: (id as u32) << 16 | 0x102,
                            item: Some(ItemRef {
                                folder: FolderRef(node.nid_parent),
                                node: node.nid,
                            }),
                            source: format!(
                                "Same Item cache tables {:?}",
                                pairs.iter().map(|r| r.0).collect::<Vec<_>>()
                            ),
                            reason:
                                "Q15: validated agreeing XID/PCL pair; no server-lineage claim."
                                    .into(),
                        });
                    }
                }
            }
        }
        planned.insert(node.nid, own);
    }
    Ok(Some(planned))
}

// MS-OXCFXICS 2.2.2.2/2.2.2.3: bounded XIDs, strictly sorted namespaces,
// complete SizedXids, and a predecessor in the key's namespace covering its ID.
fn consistent_pair(key: &[u8], mut pcl: &[u8]) -> bool {
    if !(17..=24).contains(&key.len()) || pcl.is_empty() {
        return false;
    }
    let mut previous: Option<&[u8]> = None;
    let mut contains = false;
    while !pcl.is_empty() {
        let size = pcl[0] as usize;
        if !(17..=24).contains(&size) || pcl.len() < size + 1 {
            return false;
        }
        let xid = &pcl[1..size + 1];
        if previous.is_some_and(|p| p >= &xid[..16]) {
            return false;
        }
        if xid[..16] == key[..16] {
            if xid.len() != key.len() {
                return false;
            }
            contains = xid[16..] >= key[16..];
        }
        previous = Some(&xid[..16]);
        pcl = &pcl[size + 1..];
    }
    contains
}
