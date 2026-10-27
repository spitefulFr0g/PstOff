//! The essential PST store policy, separate from byte layout.
use super::{
    context::{self, Cell, Properties},
    Result,
};
use crate::Difference;
use pstoff_format::ndb::{Node, Pst};
pub(super) fn prepare(
    source: &mut Pst,
    nodes: &[Node],
    display: &str,
    system: &super::system::SystemFolders,
    cancel: &crate::CancelToken,
) -> Result<(Properties, crate::Records<Difference>)> {
    let node = nodes
        .iter()
        .find(|n| n.nid == 0x21)
        .ok_or("The source has no message store")?;
    let mut cells = context::store_properties(
        source,
        node.bid_data,
        node.bid_sub,
        display.len().saturating_mul(2),
        cancel,
    )?;
    let key = cells
        .get(&0x0ff9)
        .filter(|c| c.kind == 0x102 && c.bytes.len() == 16)
        .ok_or("Invalid store record key")?
        .bytes
        .clone();
    let mut changes = crate::Records::default();
    let mut replace = |id: u16, kind: u16, bytes: Vec<u8>, reason: &str| -> Result<()> {
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
            })?;
        }
        Ok(())
    };
    replace(
        0x3001,
        0x1f,
        display.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        "PST display name comes from the source file name.",
    )?;
    replace(
        0x67ff,
        3,
        0u32.to_le_bytes().to_vec(),
        "The converted PST has no password.",
    )?;
    replace(
        0x35df,
        3,
        0x89u32.to_le_bytes().to_vec(),
        "Identify valid PST system folders.",
    )?;
    for (id, nid) in [
        (0x35e0, system.subtree),
        (0x35e3, system.deleted),
        (0x35e7, system.finder),
    ] {
        let mut entry = vec![0; 4];
        entry.extend_from_slice(&key);
        entry.extend_from_slice(&nid.to_le_bytes());
        replace(
            id,
            0x102,
            entry,
            "PST entry ID uses the source record key and corresponding system folder.",
        )?;
    }
    Ok((cells, changes))
}
