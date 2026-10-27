//! Selection uses global NIDs only; nested Item subnodes always travel whole.
use crate::nodes::{IdSet, Nodes};
use crate::*;

pub(crate) fn excluded(
    nodes: &Nodes,
    requested: &[FolderRef],
    cancel: &CancelToken,
) -> Result<IdSet, EngineError> {
    let mut excluded = IdSet::default();
    for folder in requested {
        if cancel.is_cancelled() {
            return Err(node_failure("Exclusion planning cancelled".into()));
        }
        if !nodes
            .iter()
            .any(|n| n.nid == folder.0 && matches!(n.nid_type(), 2 | 3))
            .map_err(node_failure)?
        {
            return Err(EngineError {
                kind: ErrorKind::InvalidOptions,
                message: format!(
                    "Excluded Folder 0x{:x} does not exist in this source.",
                    folder.0
                ),
                completed_files: Default::default(),
            });
        }
        excluded
            .insert(folder.0)
            .map_err(|e| node_failure(e.into()))?;
    }
    loop {
        let before = excluded.len();
        for node in nodes.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
            if cancel.is_cancelled() {
                return Err(node_failure("Exclusion planning cancelled".into()));
            }
            let node = node.map_err(node_failure)?;
            if excluded
                .contains(&node.nid_parent)
                .map_err(|e| node_failure(e.into()))?
            {
                excluded
                    .insert(node.nid)
                    .map_err(|e| node_failure(e.into()))?;
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
    pub items: Nodes,
    system: [u32; 4],
    excluded: IdSet,
}

fn selected_evidence(
    evidence: &mut Evidence<Records<ItemRef>>,
    excluded: &IdSet,
    cancel: &CancelToken,
) -> std::io::Result<()> {
    match evidence {
        Evidence::NotChecked => (),
        Evidence::Checked { count, references } => {
            references.try_retain_cancellable(|i| Ok(!excluded.contains(&i.folder.0)?), cancel)?;
            *count = references.len() as u64;
        }
        Evidence::Incomplete {
            count,
            references,
            unchecked,
        } => {
            references.try_retain_cancellable(|i| Ok(!excluded.contains(&i.folder.0)?), cancel)?;
            *count = references.len() as u64;
            unchecked.try_retain_cancellable(|i| Ok(!excluded.contains(&i.folder.0)?), cancel)?;
            if unchecked.is_empty() {
                *evidence = Evidence::Checked {
                    count: *count,
                    references: std::mem::take(references),
                };
            }
        }
    }
    Ok(())
}

impl Selection {
    pub fn new(
        mut recovery: writer::Recovery,
        excluded: IdSet,
        cancel: &CancelToken,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Recovery, selection and store emission share the resolved source roles.
        let system = recovery.system.ids();
        if !excluded.is_empty() {
            recovery.skipped.try_retain_cancellable(
                |s| match &s.reference {
                    LocalRef::Folder(f) => Ok(!excluded.contains(&f.0)?),
                    LocalRef::Item(i) => Ok(!excluded.contains(&i.folder.0)?),
                },
                cancel,
            )?;
            selected_evidence(&mut recovery.evidence.headers_only, &excluded, cancel)?;
            selected_evidence(&mut recovery.evidence.unreadable, &excluded, cancel)?;
            selected_evidence(
                &mut recovery.evidence.external_references,
                &excluded,
                cancel,
            )?;
        }
        let mut items = Nodes::new()?;
        for n in recovery
            .nodes
            .iter()
            .filter(|n| matches!(n.nid_type(), 4 | 8))
        {
            if cancel.is_cancelled() {
                return Err("Selection cancelled".into());
            }
            let n = n?;
            if !excluded.contains(&n.nid_parent)? {
                items.push(n)?;
            }
        }
        let items = items.sorted(cancel)?;
        Ok(Self {
            recovery,
            items,
            system,
            excluded,
        })
    }

    pub fn part(
        &self,
        range: std::ops::Range<usize>,
        split: bool,
        cancel: &CancelToken,
    ) -> Result<writer::Recovery, Box<dyn std::error::Error>> {
        let all = &self.recovery.nodes;
        let mut folders = IdSet::default();
        for id in self.system {
            folders.insert(id)?;
        }
        if !split {
            for n in all.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
                if cancel.is_cancelled() {
                    return Err("Selection cancelled".into());
                }
                let n = n?;
                if !self.excluded.contains(&n.nid)? {
                    folders.insert(n.nid)?;
                }
            }
        }
        let mut selected = writer::spool::Index::new()?;
        for i in range {
            if cancel.is_cancelled() {
                return Err("Selection cancelled".into());
            }
            let n = self.items.get(i)?.ok_or("Invalid selection range")?;
            selected.insert(n.nid, 0)?;
            folders.insert(n.nid_parent)?;
        }
        loop {
            let before = folders.len();
            for node in all.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
                if cancel.is_cancelled() {
                    return Err("Selection cancelled".into());
                }
                let node = node?;
                if folders.contains(&node.nid)? {
                    folders.insert(node.nid_parent)?;
                }
            }
            if folders.len() == before {
                break;
            }
        }
        let mut nodes = Nodes::new()?;
        let mut omitted = self.recovery.omitted.copy(cancel)?;
        for n in all {
            if cancel.is_cancelled() {
                return Err("Selection cancelled".into());
            }
            let n = n?;
            let keep = match n.nid_type() {
                2 | 3 => folders.contains(&n.nid)?,
                4 | 8 => selected.get(n.nid)?.is_some(),
                0xd..=0x10 => folders.contains_group(n.nid)?,
                _ => true,
            };
            if keep {
                nodes.push(n)?;
            } else {
                omitted.insert(n.nid)?;
            }
        }
        let mut created = self.recovery.created.clone();
        for id in self.recovery.created.keys() {
            if nodes.by_id(*id)?.is_none() {
                created.remove(id);
            }
        }
        Ok(writer::Recovery {
            nodes,
            system: self.recovery.system.clone(),
            created,
            omitted,
            skipped: Records::default(),
            evidence: CompletenessEvidence {
                headers_only: Evidence::NotChecked,
                unreadable: Evidence::NotChecked,
                external_references: Evidence::NotChecked,
            },
        })
    }
}
fn node_failure(e: Box<dyn std::error::Error>) -> EngineError {
    EngineError {
        kind: ErrorKind::Io,
        message: format!("Node staging failed: {e}"),
        completed_files: Default::default(),
    }
}
