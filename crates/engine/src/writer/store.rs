//! The essential PST store policy, separate from byte layout.
use super::{
    context::{self, Cell, Properties},
    Result,
};
use crate::Difference;
use pstoff_format::{
    ltp,
    ndb::{Node, Pst},
};
pub(super) fn prepare(
    source: &mut Pst,
    nodes: &[Node],
    display: &str,
) -> Result<(Properties, Vec<Difference>)> {
    let node = nodes
        .iter()
        .find(|n| n.nid == 0x21)
        .ok_or("The source has no message store")?;
    let mut cells = context::properties(source, node.bid_data, node.bid_sub)?;
    let key = cells
        .get(&0x0ff9)
        .filter(|c| c.kind == 0x102 && c.bytes.len() == 16)
        .ok_or("Invalid store record key")?
        .bytes
        .clone();
    let mut names = vec![];
    for n in nodes.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
        let pc = ltp::read_node_pc(source, n)?;
        names.push((n.nid, n.nid_parent, pc.str(0x3001).unwrap_or("").to_owned()));
    }
    let child = |parent: u32, name: &str| -> Result<u32> {
        let found: Vec<_> = names
            .iter()
            .filter(|n| n.1 == parent && n.2 == name)
            .collect();
        if found.len() != 1 {
            return Err(format!(
                "A required store folder is missing or ambiguous under 0x{parent:x}"
            )
            .into());
        }
        Ok(found[0].0)
    };
    let mailbox = child(0x122, "Root - Mailbox")?;
    let subtree = child(mailbox, "IPM_SUBTREE")?;
    let deleted = child(subtree, "Deleted Items")?;
    let finder = child(mailbox, "Finder")?;
    let mut changes = vec![];
    let mut replace = |id: u16, kind: u16, bytes: Vec<u8>, reason: &str| {
        if cells
            .get(&id)
            .is_none_or(|c| c.kind != kind || c.bytes != bytes)
        {
            cells.insert(
                id,
                Cell {
                    kind,
                    bytes,
                    subnode: None,
                },
            );
            changes.push(Difference {
                property: (id as u32) << 16 | kind as u32,
                item: None,
                source: "Source store and system folder hierarchy".into(),
                reason: reason.into(),
            });
        }
    };
    replace(
        0x3001,
        0x1f,
        display.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        "PST display name comes from the source file name.",
    );
    replace(
        0x67ff,
        3,
        0u32.to_le_bytes().to_vec(),
        "The converted PST has no password.",
    );
    replace(
        0x35df,
        3,
        0x89u32.to_le_bytes().to_vec(),
        "Identify valid PST system folders.",
    );
    for (id, nid) in [(0x35e0, subtree), (0x35e3, deleted), (0x35e7, finder)] {
        let mut entry = vec![0; 4];
        entry.extend_from_slice(&key);
        entry.extend_from_slice(&nid.to_le_bytes());
        replace(
            id,
            0x102,
            entry,
            "PST entry ID uses the source record key and corresponding system folder.",
        );
    }
    Ok((cells, changes))
}
