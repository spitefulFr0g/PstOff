//! Selection uses global NIDs only; nested Item subnodes always travel whole.
use crate::*;
use pstoff_format::{
    ltp,
    ndb::{Node, Pst},
};
use std::collections::HashSet;

pub(crate) fn excluded(nodes: &[Node], requested: &[FolderRef]) -> Result<HashSet<u32>, EngineError> {
    let mut excluded = HashSet::new();
    for folder in requested {
        if !nodes
            .iter()
            .any(|n| n.nid == folder.0 && matches!(n.nid_type(), 2 | 3))
        {
            return Err(EngineError {
                kind: ErrorKind::InvalidOptions,
                message: format!("Excluded Folder 0x{:x} does not exist in this source.", folder.0),
                completed_files: vec![],
            });
        }
        excluded.insert(folder.0);
    }
    loop {
        let before = excluded.len();
        for node in nodes.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
            if excluded.contains(&node.nid_parent) {
                excluded.insert(node.nid);
            }
        }
        if before == excluded.len() {
            break;
        }
    }
    Ok(excluded)
}

pub(crate) struct Selection {
    pub recovery: writer::Recovery,
    pub items: Vec<Node>,
    system: HashSet<u32>,
    excluded: HashSet<u32>,
}

fn folder_name(
    source: &mut Pst,
    recovery: &writer::Recovery,
    node: &Node,
) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(name) = recovery.created.get(&node.nid) {
        Ok(name.clone())
    } else {
        Ok(ltp::read_node_pc(source, node)?
            .str(0x3001)
            .unwrap_or("")
            .to_owned())
    }
}

fn selected_evidence(evidence: &mut Evidence, excluded: &HashSet<u32>) {
    match evidence {
        Evidence::NotChecked => (),
        Evidence::Checked { count, references } => {
            references.retain(|i| !excluded.contains(&i.folder.0));
            *count = references.len() as u64;
        }
        Evidence::Incomplete {
            count,
            references,
            unchecked,
        } => {
            references.retain(|i| !excluded.contains(&i.folder.0));
            *count = references.len() as u64;
            unchecked.retain(|i| !excluded.contains(&i.folder.0));
            if unchecked.is_empty() {
                *evidence = Evidence::Checked {
                    count: *count,
                    references: references.clone(),
                };
            }
        }
    }
}

impl Selection {
    pub fn new(
        source: &mut Pst,
        mut recovery: writer::Recovery,
        excluded: HashSet<u32>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Use the exact store-policy hierarchy, not names elsewhere in the tree.
        // System shells survive even when their source contents are excluded.
        let mut system = HashSet::from([0x122]);
        for (parent_name, name) in [
            ("", "Root - Mailbox"),
            ("Root - Mailbox", "Finder"),
            ("Root - Mailbox", "IPM_SUBTREE"),
            ("IPM_SUBTREE", "Deleted Items"),
        ] {
            let mut found = None;
            for node in recovery.nodes.iter().filter(|n| {
                matches!(n.nid_type(), 2 | 3) && system.contains(&n.nid_parent)
            }) {
                let parent = recovery
                    .nodes
                    .iter()
                    .find(|p| p.nid == node.nid_parent)
                    .ok_or("Missing folder parent")?;
                let parent_matches = if parent_name.is_empty() {
                    parent.nid == 0x122
                } else {
                    folder_name(source, &recovery, parent)? == parent_name
                };
                if parent_matches
                    && folder_name(source, &recovery, node)? == name
                    && found.replace(node.nid).is_some()
                {
                    return Err("Ambiguous system folder".into());
                }
            }
            system.insert(found.ok_or("Missing system folder")?);
        }
        recovery.skipped.retain(|s| match &s.reference {
            LocalRef::Folder(f) => !excluded.contains(&f.0),
            LocalRef::Item(i) => !excluded.contains(&i.folder.0),
        });
        selected_evidence(&mut recovery.evidence.headers_only, &excluded);
        selected_evidence(&mut recovery.evidence.unreadable, &excluded);
        selected_evidence(&mut recovery.evidence.external_references, &excluded);
        let mut items: Vec<_> = recovery
            .nodes
            .iter()
            .filter(|n| matches!(n.nid_type(), 4 | 8) && !excluded.contains(&n.nid_parent))
            .cloned()
            .collect();
        items.sort_by_key(|n| n.nid);
        Ok(Self {
            recovery,
            items,
            system,
            excluded,
        })
    }

    pub fn part(&self, items: &[Node], split: bool) -> writer::Recovery {
        let all = &self.recovery.nodes;
        let mut folders = self.system.clone();
        if !split {
            folders.extend(
                all.iter()
                    .filter(|n| {
                        matches!(n.nid_type(), 2 | 3) && !self.excluded.contains(&n.nid)
                    })
                    .map(|n| n.nid),
            );
        }
        folders.extend(items.iter().map(|n| n.nid_parent));
        loop {
            let before = folders.len();
            for node in all.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
                if folders.contains(&node.nid) {
                    folders.insert(node.nid_parent);
                }
            }
            if folders.len() == before {
                break;
            }
        }
        let ids: HashSet<_> = items.iter().map(|n| n.nid).collect();
        let mut part = self.recovery.clone();
        part.nodes.retain(|n| match n.nid_type() {
            2 | 3 => folders.contains(&n.nid),
            4 | 8 => ids.contains(&n.nid),
            0xd..=0x10 => folders.iter().any(|id| id & !31 == n.nid & !31),
            _ => true,
        });
        let retained: HashSet<_> = part.nodes.iter().map(|n| n.nid).collect();
        part.omitted.extend(
            all.iter()
                .filter(|n| !retained.contains(&n.nid))
                .map(|n| n.nid),
        );
        part.created.retain(|id, _| retained.contains(id));
        part
    }
}
