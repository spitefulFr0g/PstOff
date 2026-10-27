//! Resolve source folder roles once. Display names are never authoritative IDs.
use super::{context, recovery::add_folder, Result};
use pstoff_format::ndb::Pst;
use std::collections::BTreeMap;

#[derive(Clone)]
pub(crate) struct SystemFolders {
    pub subtree: u32,
    pub deleted: u32,
    pub finder: u32,
}
impl SystemFolders {
    pub fn ids(&self) -> [u32; 4] {
        [0x122, self.subtree, self.deleted, self.finder]
    }
}

pub(super) struct SourceFolders {
    mailbox: Option<u32>,
    subtree: Option<u32>,
    deleted: Option<u32>,
    finder: Option<u32>,
}

pub(super) fn resolve(source: &mut Pst, nodes: &crate::nodes::Nodes) -> Result<SourceFolders> {
    let store = nodes.by_id(0x21)?.ok_or("Missing message store")?;
    let cells = context::properties(source, store.bid_data, store.bid_sub)?;
    let key = cells
        .get(&0x0ff9)
        .filter(|c| c.kind == 0x102 && c.bytes.len() == 16)
        .ok_or("Invalid store record key")?;
    let entry = |tag: u16| -> Result<Option<u32>> {
        let Some(cell) = cells.get(&tag) else {
            return Ok(None);
        };
        if cell.kind != 0x102
            || cell.bytes.len() != 24
            || cell.bytes[..4] != [0; 4]
            || cell.bytes[4..20] != key.bytes[..]
        {
            return Err(format!("Invalid source system folder EntryID property 0x{tag:04x}0102; verify local store identity before retrying.").into());
        }
        let nid = u32::from_le_bytes(cell.bytes[20..24].try_into()?);
        if nid == 0x122
            || !nodes
                .by_id(nid)?
                .is_some_and(|n| matches!(n.nid_type(), 2 | 3))
            || nid & 31 != 2
        {
            return Err(format!("Source system folder property 0x{tag:04x}0102 references no normal folder; verify the source hierarchy before retrying.").into());
        }
        if !descendant(nodes, nid, 0x122)? {
            return Err("Source system folder is outside the root hierarchy".into());
        }
        Ok(Some(nid))
    };
    let subtree = entry(0x35e0)?;
    let deleted = entry(0x35e3)?;
    let finder = entry(0x35e7)?;
    let mailbox = if let Some(id) = subtree {
        Some(nodes.by_id(id)?.ok_or("Missing subtree")?.nid_parent)
    } else {
        None
    };
    if let Some(subtree) = subtree {
        if (if let Some(id) = deleted {
            !descendant(nodes, id, subtree)?
        } else {
            false
        }) || (if let Some(id) = finder {
            id == subtree || descendant(nodes, id, subtree)?
        } else {
            false
        }) {
            return Err("Conflicting source system folder roles".into());
        }
    } else if deleted.is_some() {
        return Err("Source wastebasket has no identifiable IPM subtree".into());
    }
    Ok(SourceFolders {
        mailbox,
        subtree,
        deleted,
        finder,
    })
}

fn descendant(nodes: &crate::nodes::Nodes, mut id: u32, parent: u32) -> Result<bool> {
    for _ in 0..nodes.len() {
        let Some(node) = nodes.by_id(id)?.filter(|n| matches!(n.nid_type(), 2 | 3)) else {
            return Ok(false);
        };
        if node.nid_parent == parent && id != parent {
            return Ok(true);
        }
        if node.nid_parent == id {
            return Ok(false);
        }
        id = node.nid_parent;
    }
    Ok(false)
}

// Only absent/unreadable source roles receive empty output scaffolding. The
// original role IDs were resolved before recovery removed unreadable subtrees.
pub(super) fn repair(
    identity: SourceFolders,
    nodes: &mut crate::nodes::Nodes,
    highwater: u32,
) -> Result<(SystemFolders, BTreeMap<u32, String>)> {
    // Missing role metadata is not proof of a missing source folder. Only
    // recovery can establish that no readable candidates remain at a boundary.
    if identity.subtree.is_none()
        && nodes.iter().any(|n| {
            (matches!(n.nid_type(), 2 | 3) && n.nid != 0x122) || matches!(n.nid_type(), 4 | 8)
        })?
    {
        return Err("Cannot identify the source IPM subtree; valid local 0x35e00102 metadata is required. Verify the source store before retrying. No replacement hierarchy was created.".into());
    }
    if identity.deleted.is_none()
        && if let Some(parent) = identity.subtree {
            nodes.iter().any(|n| {
                matches!(n.nid_type(), 2 | 3) && n.nid_parent == parent && n.nid != parent
            })?
        } else {
            false
        }
    {
        return Err("Cannot identify the source wastebasket among readable folders; valid local 0x35e30102 metadata is required. Verify the source store before retrying. No replacement folder was created.".into());
    }
    if identity.finder.is_none()
        && if let Some(parent) = identity.mailbox {
            nodes.iter().any(|n| {
                matches!(n.nid_type(), 2 | 3)
                    && n.nid_parent == parent
                    && n.nid != parent
                    && Some(n.nid) != identity.subtree
            })?
        } else {
            false
        }
    {
        return Err("Cannot identify the source Finder among readable folders; valid local 0x35e70102 metadata is required. Verify the source store before retrying. No replacement folder was created.".into());
    }
    let retained = |id: Option<u32>| -> Result<Option<u32>> {
        if let Some(id) = id {
            Ok(nodes.by_id(id)?.map(|n| n.nid))
        } else {
            Ok(None)
        }
    };
    let mailbox = retained(identity.mailbox)?;
    let subtree = retained(identity.subtree)?;
    let deleted = retained(identity.deleted)?;
    let finder = retained(identity.finder)?;
    let mut created = BTreeMap::new();
    if nodes.by_id(0x122)?.is_none() {
        add_folder(nodes, &mut created, 0x122, 0x122, "Root")?;
    }
    let mut next = highwater.checked_add(32).ok_or("Folder ID overflow")?;
    let mut ensure = |id: Option<u32>, parent: u32, name: &str| -> Result<u32> {
        if let Some(id) = id {
            return Ok(id);
        }
        let id = next | 2;
        next = next.checked_add(32).ok_or("Folder ID overflow")?;
        add_folder(nodes, &mut created, id, parent, name)?;
        Ok(id)
    };
    let mailbox = ensure(mailbox, 0x122, "Root - Mailbox")?;
    let subtree = ensure(subtree, mailbox, "IPM_SUBTREE")?;
    let deleted = ensure(deleted, subtree, "Deleted Items")?;
    let finder = ensure(finder, mailbox, "Finder")?;
    Ok((
        SystemFolders {
            subtree,
            deleted,
            finder,
        },
        created,
    ))
}
