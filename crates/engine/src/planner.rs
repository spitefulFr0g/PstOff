//! Selection uses global NIDs only; nested Item subnodes always travel whole.
use crate::*;
use pstoff_format::ndb::Node;
use std::collections::HashSet;

pub(crate) fn excluded(
    nodes: &[Node],
    requested: &[FolderRef],
) -> Result<HashSet<u32>, EngineError> {
    let mut excluded = HashSet::new();
    for folder in requested {
        if !nodes
            .iter()
            .any(|n| n.nid == folder.0 && matches!(n.nid_type(), 2 | 3))
        {
            return Err(EngineError {
                kind: ErrorKind::InvalidOptions,
                message: format!(
                    "Excluded Folder 0x{:x} does not exist in this source.",
                    folder.0
                ),
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
                    references: std::mem::take(references),
                };
            }
        }
    }
}

impl Selection {
    pub fn new(mut recovery: writer::Recovery, excluded: HashSet<u32>) -> Self {
        // Recovery, selection and store emission share the resolved source roles.
        let system = HashSet::from(recovery.system.ids());
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
        Self {
            recovery,
            items,
            system,
            excluded,
        }
    }

    pub fn part(&self, items: &[Node], split: bool) -> writer::Recovery {
        let all = &self.recovery.nodes;
        let mut folders = self.system.clone();
        if !split {
            folders.extend(
                all.iter()
                    .filter(|n| matches!(n.nid_type(), 2 | 3) && !self.excluded.contains(&n.nid))
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
        let mut part = writer::Recovery {
            nodes: all
                .iter()
                .filter(|n| match n.nid_type() {
                    2 | 3 => folders.contains(&n.nid),
                    4 | 8 => items.binary_search_by_key(&n.nid, |n| n.nid).is_ok(),
                    0xd..=0x10 => folders.iter().any(|id| id & !31 == n.nid & !31),
                    _ => true,
                })
                .cloned()
                .collect(),
            system: self.recovery.system.clone(),
            created: self.recovery.created.clone(),
            omitted: self.recovery.omitted.clone(),
            // Evidence belongs to the conversion report, never to a candidate
            // Split part. Copying it here multiplies item-count ownership.
            skipped: Vec::new(),
            evidence: CompletenessEvidence {
                headers_only: Evidence::NotChecked,
                unreadable: Evidence::NotChecked,
                external_references: Evidence::NotChecked,
            },
        };
        part.omitted.extend(
            all.iter()
                .filter(|n| !part.nodes.iter().any(|kept| kept.nid == n.nid))
                .map(|n| n.nid),
        );
        part.created
            .retain(|id, _| part.nodes.iter().any(|n| n.nid == *id));
        part
    }
}
