//! Read-only store boundary. Format details never escape through the frontend API.
use crate::{EngineError, ErrorKind, Folder, FolderRef, Inspection, Support, UnsupportedReason};
use pstoff_format::{ltp, ndb::Pst};
use std::collections::{BTreeMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

fn error(kind: ErrorKind, message: impl Into<String>) -> EngineError {
    EngineError {
        kind,
        message: message.into(),
        completed_files: Vec::new(),
    }
}
fn open(path: &Path) -> Result<File, EngineError> {
    let mut options = OpenOptions::new();
    options.read(true);
    // Deny concurrent writers and deletion while our source handle is alive.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // FILE_SHARE_READ
    }
    options.open(path).map_err(|e| {
        #[cfg(windows)]
        if matches!(e.raw_os_error(), Some(32 | 33)) {
            return error(ErrorKind::InUse, "Close Outlook and retry this OST.");
        }
        error(ErrorKind::Io, e.to_string())
    })
}

/// Reject unsupported layouts before allowing the lower-level permissive reader access.
fn classify(file: &mut File) -> Result<Support, EngineError> {
    let mut h = Vec::new();
    file.take(564)
        .read_to_end(&mut h)
        .map_err(|e| error(ErrorKind::Io, e.to_string()))?;
    let unsupported = |r| Ok(Support::Unsupported(r));
    if h.len() < 12 || &h[..4] != b"!BDN" || &h[8..10] != b"SO" {
        return Err(error(
            ErrorKind::DamagedSource,
            "The file is not a valid OST header.",
        ));
    }
    let version = u16::from_le_bytes([h[10], h[11]]);
    match version {
        14 | 15 => return unsupported(UnsupportedReason::Ansi),
        21 | 23 => return unsupported(UnsupportedReason::OlderUnicode(version)),
        36 => (),
        _ => return unsupported(UnsupportedReason::UnknownVersion(version)),
    }
    if h.len() < 564 {
        return Err(error(
            ErrorKind::DamagedSource,
            "The OST header is truncated.",
        ));
    }
    if h[513] != 0 {
        return unsupported(UnsupportedReason::Encoding(h[513]));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|e| error(ErrorKind::Io, e.to_string()))?;
    Ok(Support::Supported)
}

// Validate the typed subnode contexts without interpreting Item class. Appointments,
// contacts, tasks, notes and journal entries use exactly the same PC/container seam.
// Untyped subnodes are property payload streams and are resolved by read_pc/read_tc.
pub(crate) fn item_properties(
    store: &mut Pst,
    node: &pstoff_format::ndb::Node,
) -> Result<ltp::Pc, Box<dyn std::error::Error>> {
    let pc = ltp::read_node_pc(store, node)?;
    if pc
        .props
        .values()
        .any(|v| matches!(v, ltp::Value::MissingSubnode(_)))
    {
        return Err("An Item property payload is missing".into());
    }
    let mut pending = vec![(node.bid_sub, 0)];
    let mut visited = HashSet::new();
    while let Some((bid, depth)) = pending.pop() {
        if bid == 0 {
            continue;
        }
        if depth > 32 || !visited.insert(bid) {
            return Err("Cyclic or excessively nested Item subnodes".into());
        }
        for (nid, sub) in store.subnodes(bid)? {
            match nid & 31 {
                4 | 5 => {
                    ltp::read_pc(store, sub.data, sub.sub)?;
                }
                0x11 | 0x12 => {
                    crate::writer::validate_table(store, sub.data, sub.sub)?;
                }
                _ => (),
            }
            pending.push((sub.sub, depth + 1));
        }
    }
    Ok(pc)
}

/// Conversion must retain this handle until its source reads finish. The same
/// classified, read-only handle is used for header, indexes, and all data blocks.
pub(crate) fn read_source(
    path: &Path,
) -> Result<(Pst, Vec<pstoff_format::ndb::Node>), EngineError> {
    read_handle(open(path)?)
}

pub(crate) fn conversion_source(
    path: &Path,
) -> Result<(Pst, Vec<pstoff_format::ndb::Node>, String), EngineError> {
    use sha2::{Digest, Sha256};
    let file = open(path)?;
    let mut identity = file
        .try_clone()
        .map_err(|e| error(ErrorKind::Io, e.to_string()))?;
    let (store, nodes) = read_handle(file)?;
    identity
        .seek(SeekFrom::Start(0))
        .map_err(|e| error(ErrorKind::Io, e.to_string()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = identity
            .read(&mut buffer)
            .map_err(|e| error(ErrorKind::Io, e.to_string()))?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok((store, nodes, format!("{:x}", digest.finalize())))
}

fn read_handle(mut file: File) -> Result<(Pst, Vec<pstoff_format::ndb::Node>), EngineError> {
    match classify(&mut file)? {
        Support::Unsupported(reason) => {
            return Err(error(
                ErrorKind::Unsupported(reason),
                "This OST format is not supported.",
            ));
        }
        Support::InUse => return Err(error(ErrorKind::InUse, "Close Outlook and retry this OST.")),
        Support::Supported => (),
    }
    let mut store = Pst::from_file(file).map_err(|e| error(ErrorKind::DamagedSource, e))?;
    if !store.warnings.is_empty() {
        return Err(error(
            ErrorKind::DamagedSource,
            "The OST header failed integrity checks.",
        ));
    }
    let nodes = store.nodes();
    if !store.warnings.is_empty() {
        return Err(error(
            ErrorKind::DamagedSource,
            "The OST node index could not be read completely.",
        ));
    }
    ltp::read_named_properties(&mut store, &nodes).map_err(|_| {
        error(
            ErrorKind::DamagedSource,
            "The named-property map could not be read.",
        )
    })?;
    Ok((store, nodes))
}

pub(super) fn inspect(path: &Path) -> Result<Inspection, EngineError> {
    let mut folders = Vec::new();
    let support;
    match read_source(path) {
        Err(e) => {
            support = match e.kind {
                ErrorKind::Unsupported(reason) => Support::Unsupported(reason),
                ErrorKind::InUse => Support::InUse,
                _ => return Err(e),
            };
        }
        Ok((mut store, nodes)) => {
            support = Support::Supported;
            let mut flat = BTreeMap::new();
            let mut parents = BTreeMap::new();
            let mut children_by_parent: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            let mut items_by_folder: BTreeMap<u32, Vec<_>> = BTreeMap::new();
            let mut tables_by_folder: BTreeMap<u32, Vec<_>> = BTreeMap::new();
            for node in &nodes {
                if node.nid_type() == 4 {
                    items_by_folder
                        .entry(node.nid_parent)
                        .or_default()
                        .push(node);
                }
                if matches!(node.nid_type(), 0x0d..=0x0f) {
                    tables_by_folder
                        .entry(node.nid & !31)
                        .or_default()
                        .push(node);
                }
            }
            for node in nodes
                .iter()
                .filter(|n| n.nid_type() == 2 || n.nid_type() == 3)
            {
                let mut folder = Folder {
                    id: FolderRef(node.nid),
                    name: String::new(),
                    item_count: 0,
                    bytes: 0,
                    unreadable: None,
                    children: vec![],
                };
                match ltp::read_node_pc(&mut store, node) {
                    Ok(pc) => {
                        if let Some(ltp::Value::Str(name)) = pc.props.get(&ltp::PID_DISPLAY_NAME) {
                            folder.name = name.clone();
                        }
                    }
                    Err(_) => {
                        folder.unreadable = Some("Folder properties could not be read.".into())
                    }
                }
                for item in items_by_folder.get(&node.nid).into_iter().flatten() {
                    folder.item_count += 1;
                    match item_properties(&mut store, item) {
                        Ok(pc) => match pc.props.get(&ltp::PID_MESSAGE_SIZE) {
                            Some(ltp::Value::Int(size)) if *size >= 0 => {
                                folder.bytes = folder.bytes.saturating_add(*size as u64)
                            }
                            _ => {
                                folder.unreadable =
                                    Some("An Item has no readable size property.".into())
                            }
                        },
                        Err(e) => {
                            if e.is::<std::io::Error>() {
                                return Err(error(
                                    ErrorKind::Io,
                                    format!("Table staging failed: {e}"),
                                ));
                            }
                            folder.unreadable = Some("One or more Items could not be read.".into())
                        }
                    }
                }
                for table in tables_by_folder
                    .get(&(node.nid & !31))
                    .into_iter()
                    .flatten()
                {
                    if let Err(e) =
                        crate::writer::validate_table(&mut store, table.bid_data, table.bid_sub)
                    {
                        if e.is::<std::io::Error>() {
                            return Err(error(ErrorKind::Io, format!("Table staging failed: {e}")));
                        }
                        folder.unreadable = Some("A folder table could not be read.".into());
                    }
                }
                // MS-PST's root folder points to itself. That one self-reference
                // terminates the hierarchy; every other cycle remains damage.
                let parent = if node.nid == ltp::NID_ROOT_FOLDER && node.nid_parent == node.nid {
                    0
                } else {
                    node.nid_parent
                };
                children_by_parent.entry(parent).or_default().push(node.nid);
                parents.insert(node.nid, parent);
                flat.insert(node.nid, folder);
            }
            // A cycle is damage, not an excuse to silently hide folders or recurse forever.
            let mut resolved = HashSet::new();
            for &id in flat.keys() {
                let mut seen = HashSet::new();
                let mut current = id;
                while flat.contains_key(&current) && !resolved.contains(&current) {
                    if !seen.insert(current) {
                        return Err(error(
                            ErrorKind::DamagedSource,
                            "The folder hierarchy contains a cycle.",
                        ));
                    }
                    current = parents[&current];
                }
                resolved.extend(seen);
            }
            let roots: Vec<_> = flat
                .keys()
                .copied()
                .filter(|id| !flat.contains_key(&parents[id]))
                .collect();
            // Iterative assembly keeps hostile folder depth off the call stack.
            let mut pending: Vec<_> = roots.iter().map(|&id| (id, false)).collect();
            let mut built = BTreeMap::new();
            while let Some((id, visited)) = pending.pop() {
                if visited {
                    let mut folder = flat.remove(&id).expect("indexed folder");
                    let children = children_by_parent.remove(&id).unwrap_or_default();
                    for child in children {
                        if let Some(folder_child) = built.remove(&child) {
                            folder.children.push(folder_child);
                        }
                    }
                    built.insert(id, folder);
                } else {
                    pending.push((id, true));
                    pending.extend(
                        children_by_parent
                            .get(&id)
                            .into_iter()
                            .flatten()
                            .map(|&child| (child, false)),
                    );
                }
            }
            for id in roots {
                folders.push(built.remove(&id).expect("built root"));
            }
        }
    }
    Ok(Inspection {
        source: path.into(),
        support,
        folders,
    })
}
