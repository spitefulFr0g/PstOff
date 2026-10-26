//! Validate source contexts before emitting anything. Output I/O errors remain fatal.
use super::{context, table, Result};
use crate::*;
use pstoff_format::{
    ltp,
    ndb::{Node, Pst},
};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone)]
pub(crate) struct Recovery {
    pub nodes: Vec<Node>,
    pub created: BTreeMap<u32, String>,
    pub omitted: HashSet<u32>,
    pub skipped: Vec<Skipped>,
    pub evidence: CompletenessEvidence,
}

// Use the same strict property/table readers as emission, including attachment,
// recipient, embedded-message and raw payload subnodes. Never retain payload text
// in errors: reports contain only local references and fixed reason codes.
fn validate(source: &mut Pst, data: u64, sub: u64, depth: u32, expected: Option<u8>) -> Result<()> {
    if depth > 32 {
        return Err("Excessive context depth".into());
    }
    let blocks = source.node_blocks(data)?;
    if let Some(kind) = expected {
        if !blocks
            .first()
            .is_some_and(|b| b.get(2) == Some(&0xec) && b.get(3) == Some(&kind))
        {
            return Err("Invalid typed context signature".into());
        }
        match kind {
            0xbc => {
                context::properties(source, data, sub)?;
            }
            0x7c => {
                table::read(source, data, sub)?;
            }
            _ => return Err("Invalid context".into()),
        }
    }
    for (id, child) in source.subnodes(sub)? {
        let kind = match id & 31 {
            4 | 5 => Some(0xbc),
            0x11 | 0x12 => Some(0x7c),
            _ => None,
        };
        validate(source, child.data, child.sub, depth + 1, kind)?;
    }
    Ok(())
}
fn reference(n: &Node) -> ItemRef {
    ItemRef {
        folder: FolderRef(n.nid_parent),
        node: n.nid,
        folder_path: String::new(),
    }
}
fn evidence(references: Vec<ItemRef>, unchecked: &[ItemRef]) -> Evidence {
    let count = references.len() as u64;
    if unchecked.is_empty() {
        Evidence::Checked { count, references }
    } else {
        Evidence::Incomplete {
            count,
            references,
            unchecked: unchecked.to_vec(),
        }
    }
}

pub(crate) fn prepare(source: &mut Pst, nodes: &[Node], cancel: &CancelToken) -> Result<Recovery> {
    let names = ltp::read_named_properties(source, nodes)?;
    let header_ids: Vec<_> = names
        .iter()
        .filter(|(_, p)| {
            p.guid == [8, 0x20, 6, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0x46]
                && p.name == ltp::PropertyName::Id(0x8578)
        })
        .map(|(&id, _)| id)
        .collect();
    let mut omitted = HashSet::new();
    let mut bad_folders = HashSet::new();
    let mut skipped = vec![];
    for folder in nodes.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
        if cancel.is_cancelled() {
            break;
        }
        let bad = validate(source, folder.bid_data, folder.bid_sub, 0, Some(0xbc)).is_err()
            || nodes
                .iter()
                .filter(|n| matches!(n.nid_type(), 0xd..=0xf) && n.nid & !31 == folder.nid & !31)
                .any(|n| validate(source, n.bid_data, n.bid_sub, 0, Some(0x7c)).is_err());
        if bad {
            bad_folders.insert(folder.nid);
        }
    }
    // Propagate unreadable folder boundaries iteratively; inspection rejects cycles.
    let mut lost = bad_folders.clone();
    loop {
        let old = lost.len();
        for n in nodes.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
            if n.nid != n.nid_parent && lost.contains(&n.nid_parent) {
                lost.insert(n.nid);
            }
        }
        if old == lost.len() {
            break;
        }
    }
    for n in nodes {
        if lost.contains(&n.nid) && matches!(n.nid_type(), 2 | 3) {
            omitted.insert(n.nid);
            skipped.push(Skipped {
                folder_path: String::new(),
                reference: LocalRef::Folder(FolderRef(n.nid)),
                code: if bad_folders.contains(&n.nid) {
                    SkipReason::UnreadableFolder
                } else {
                    SkipReason::UnreadableAncestor
                },
                reason: "Folder or ancestor context could not be read; subtree skipped.".into(),
            });
        }
        if matches!(n.nid_type(), 0xd..=0xf) && lost.iter().any(|id| id & !31 == n.nid & !31) {
            omitted.insert(n.nid);
        }
    }
    let mut headers = vec![];
    let mut external = vec![];
    let mut unreadable = vec![];
    let mut unchecked = vec![];
    let mut header_unchecked = vec![];
    let mut external_unchecked = vec![];
    for n in nodes.iter().filter(|n| matches!(n.nid_type(), 4 | 8)) {
        if cancel.is_cancelled() {
            break;
        }
        let item = reference(n);
        let ancestor = lost.contains(&n.nid_parent);
        if ancestor
            || crate::reader::item_properties(source, n).is_err()
            || validate(source, n.bid_data, n.bid_sub, 0, Some(0xbc)).is_err()
        {
            omitted.insert(n.nid);
            unreadable.push(item.clone());
            unchecked.push(item.clone());
            skipped.push(Skipped {
                folder_path: String::new(),
                reference: LocalRef::Item(item),
                code: if ancestor {
                    SkipReason::UnreadableAncestor
                } else {
                    SkipReason::UnreadableItem
                },
                reason: if ancestor {
                    "Item belongs to a skipped unreadable folder."
                } else {
                    "Item properties or nested content could not be read."
                }
                .into(),
            });
            continue;
        }
        let pc = ltp::read_node_pc(source, n)?;
        let mut header_unknown = false;
        let mut positive = false;
        for id in &header_ids {
            match (pc.types.get(id), pc.props.get(id)) {
                (None, None) => (),
                (Some(3), Some(ltp::Value::Int(v))) if *v == 0 || *v == 1 => {
                    positive |= *v == 1;
                }
                _ => header_unknown = true,
            }
        }
        if positive {
            headers.push(item.clone());
        }
        if header_unknown {
            header_unchecked.push(item.clone());
        }
        // Walk nested Items as well. A reference is evidence of a reference,
        // never proof of missing content or of reachability (conversion is offline).
        let mut pending = vec![(n.bid_sub, 0)];
        let mut has_external = false;
        let mut unknown_external = false;
        while let Some((bid, depth)) = pending.pop() {
            if depth > 32 {
                unknown_external = true;
                break;
            }
            for (id, child) in source.subnodes(bid)? {
                if id & 31 == 5 {
                    let pc = ltp::read_pc(source, child.data, child.sub)?;
                    match (pc.types.get(&0x3705), pc.props.get(&0x3705)) {
                        (Some(3), Some(ltp::Value::Int(2 | 3 | 4 | 7))) => {
                            let web = matches!(pc.props.get(&0x3705), Some(ltp::Value::Int(7)));
                            let ids: &[u16] = if web { &[0x3707] } else { &[0x370d, 0x3708] };
                            let valid = ids.iter().any(|id| {
                                matches!(pc.types.get(id), Some(0x1f | 0x1e)) &&
                                matches!(pc.props.get(id), Some(ltp::Value::Str(s)) if valid_target(s, web))
                            }) && !pc.props.contains_key(&0x3701);
                            has_external |= valid;
                            unknown_external |= !valid;
                        }
                        (Some(3), Some(ltp::Value::Int(1 | 5 | 6))) => (),
                        _ => unknown_external = true,
                    }
                }
                pending.push((child.sub, depth + 1));
            }
        }
        if has_external {
            external.push(item.clone());
        }
        if unknown_external {
            external_unchecked.push(item);
        }
    }
    header_unchecked.extend(unchecked.iter().cloned());
    external_unchecked.extend(unchecked);
    let mut retained: Vec<_> = nodes
        .iter()
        .filter(|n| !omitted.contains(&n.nid))
        .cloned()
        .collect();
    let created = repair_structure(
        source,
        &mut retained,
        nodes.iter().map(|n| n.nid & !31).max().unwrap_or(0x120),
    )?;
    Ok(Recovery {
        nodes: retained,
        created,
        omitted,
        skipped,
        evidence: CompletenessEvidence {
            headers_only: evidence(headers, &header_unchecked),
            unreadable: evidence(unreadable, &[]),
            external_references: evidence(external, &external_unchecked),
        },
    })
}

// Validate a local reference's shape without opening or contacting its target.
fn valid_target(s: &str, web: bool) -> bool {
    if s.is_empty() || s.chars().any(|c| c.is_control()) {
        return false;
    }
    if web {
        if s.chars().any(|c| c.is_whitespace()) {
            return false;
        }
        let Some(rest) = s
            .strip_prefix("https://")
            .or_else(|| s.strip_prefix("http://"))
        else {
            return false;
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        !authority.is_empty() && !authority.contains('@') && !authority.contains('\\')
    } else {
        Path::new(s).is_absolute()
    }
}

// Required PST scaffolding is output metadata, not recovered mail. Missing source
// folders stay skipped; newly created empty folders have explicit differences.
fn repair_structure(
    source: &mut Pst,
    nodes: &mut Vec<Node>,
    highwater: u32,
) -> Result<BTreeMap<u32, String>> {
    let mut names = BTreeMap::new();
    for n in nodes.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
        let pc = ltp::read_node_pc(source, n)?;
        names.insert(n.nid, pc.str(0x3001).unwrap_or("").to_owned());
    }
    let mut created = BTreeMap::new();
    let mut next = highwater.checked_add(32).ok_or("Folder ID overflow")?;
    if !names.contains_key(&0x122) {
        add_folder(nodes, &mut created, 0x122, 0x122, "Root");
    }
    let mut ensure = |nodes: &mut Vec<Node>, parent: u32, name: &str| -> Result<u32> {
        let found: Vec<_> = nodes
            .iter()
            .filter(|n| n.nid_parent == parent && names.get(&n.nid).is_some_and(|v| v == name))
            .map(|n| n.nid)
            .collect();
        if found.len() > 1 {
            return Err("Ambiguous required folder identity".into());
        }
        if let Some(id) = found.first() {
            return Ok(*id);
        }
        let id = next | 2;
        next = next.checked_add(32).ok_or("Folder ID overflow")?;
        add_folder(nodes, &mut created, id, parent, name);
        Ok(id)
    };
    let mailbox = ensure(nodes, 0x122, "Root - Mailbox")?;
    ensure(nodes, mailbox, "Finder")?;
    let subtree = ensure(nodes, mailbox, "IPM_SUBTREE")?;
    ensure(nodes, subtree, "Deleted Items")?;
    Ok(created)
}
fn add_folder(
    nodes: &mut Vec<Node>,
    created: &mut BTreeMap<u32, String>,
    id: u32,
    parent: u32,
    name: &str,
) {
    nodes.push(Node {
        nid: id,
        nid_parent: parent,
        bid_data: 0,
        bid_sub: 0,
    });
    for kind in [0xd, 0xe, 0xf] {
        nodes.push(Node {
            nid: (id & !31) | kind,
            nid_parent: 0,
            bid_data: 0,
            bid_sub: 0,
        });
    }
    created.insert(id, name.into());
}
