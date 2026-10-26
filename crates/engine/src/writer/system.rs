//! Resolve source folder roles once. Display names are never authoritative IDs.
use super::{context, recovery::add_folder, Result};
use pstoff_format::ndb::{Node, Pst};
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

pub(super) fn resolve(source: &mut Pst, nodes: &[Node]) -> Result<SourceFolders> {
    let store = nodes
        .iter()
        .find(|n| n.nid == 0x21)
        .ok_or("Missing message store")?;
    let cells = context::properties(source, store.bid_data, store.bid_sub)?;
    let key = cells
        .get(&0x0ff9)
        .filter(|c| c.kind == 0x102 && c.bytes.len() == 16)
        .ok_or("Invalid store record key")?;
    let folders: BTreeMap<_, _> = nodes
        .iter()
        .filter(|n| matches!(n.nid_type(), 2 | 3))
        .map(|n| (n.nid, n))
        .collect();
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
        if nid == 0x122 || !folders.contains_key(&nid) || nid & 31 != 2 {
            return Err(format!("Source system folder property 0x{tag:04x}0102 references no normal folder; verify the source hierarchy before retrying.").into());
        }
        if !descendant(&folders, nid, 0x122) {
            return Err("Source system folder is outside the root hierarchy".into());
        }
        Ok(Some(nid))
    };
    let subtree = entry(0x35e0)?;
    let deleted = entry(0x35e3)?;
    let finder = entry(0x35e7)?;
    let mailbox = subtree.map(|id| folders[&id].nid_parent);
    if let Some(subtree) = subtree {
        if deleted.is_some_and(|id| !descendant(&folders, id, subtree))
            || finder.is_some_and(|id| id == subtree || descendant(&folders, id, subtree))
        {
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

fn descendant(folders: &BTreeMap<u32, &Node>, mut id: u32, parent: u32) -> bool {
    for _ in 0..folders.len() {
        let Some(node) = folders.get(&id) else {
            return false;
        };
        if node.nid_parent == parent && id != parent {
            return true;
        }
        if node.nid_parent == id {
            return false;
        }
        id = node.nid_parent;
    }
    false
}

// Only absent/unreadable source roles receive empty output scaffolding. The
// original role IDs were resolved before recovery removed unreadable subtrees.
pub(super) fn repair(
    identity: SourceFolders,
    nodes: &mut Vec<Node>,
    highwater: u32,
) -> Result<(SystemFolders, BTreeMap<u32, String>)> {
    // Missing role metadata is not proof of a missing source folder. Only
    // recovery can establish that no readable candidates remain at a boundary.
    if identity.subtree.is_none()
        && nodes.iter().any(|n| {
            (matches!(n.nid_type(), 2 | 3) && n.nid != 0x122) || matches!(n.nid_type(), 4 | 8)
        })
    {
        return Err("Cannot identify the source IPM subtree; valid local 0x35e00102 metadata is required. Verify the source store before retrying. No replacement hierarchy was created.".into());
    }
    if identity.deleted.is_none()
        && identity.subtree.is_some_and(|parent| {
            nodes
                .iter()
                .any(|n| matches!(n.nid_type(), 2 | 3) && n.nid_parent == parent && n.nid != parent)
        })
    {
        return Err("Cannot identify the source wastebasket among readable folders; valid local 0x35e30102 metadata is required. Verify the source store before retrying. No replacement folder was created.".into());
    }
    if identity.finder.is_none()
        && identity.mailbox.is_some_and(|parent| {
            nodes.iter().any(|n| {
                matches!(n.nid_type(), 2 | 3)
                    && n.nid_parent == parent
                    && n.nid != parent
                    && Some(n.nid) != identity.subtree
            })
        })
    {
        return Err("Cannot identify the source Finder among readable folders; valid local 0x35e70102 metadata is required. Verify the source store before retrying. No replacement folder was created.".into());
    }
    let retained = |id: Option<u32>| id.filter(|id| nodes.iter().any(|n| n.nid == *id));
    let mailbox = retained(identity.mailbox);
    let subtree = retained(identity.subtree);
    let deleted = retained(identity.deleted);
    let finder = retained(identity.finder);
    let mut created = BTreeMap::new();
    if !nodes.iter().any(|n| n.nid == 0x122) {
        add_folder(nodes, &mut created, 0x122, 0x122, "Root");
    }
    let mut next = highwater.checked_add(32).ok_or("Folder ID overflow")?;
    let mut ensure = |id: Option<u32>, parent: u32, name: &str| -> Result<u32> {
        if let Some(id) = id {
            return Ok(id);
        }
        let id = next | 2;
        next = next.checked_add(32).ok_or("Folder ID overflow")?;
        add_folder(nodes, &mut created, id, parent, name);
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
