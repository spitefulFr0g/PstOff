//! Read-only store boundary. Format details never escape through the frontend API.
use crate::{EngineError, ErrorKind, Folder, FolderRef, Inspection, Support, UnsupportedReason};
use pstoff_format::{ltp, ndb::Pst};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

fn error(kind: ErrorKind, message: impl Into<String>) -> EngineError {
    EngineError {
        kind,
        message: message.into(),
        completed_files: Default::default(),
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
    let pc = crate::writer::evidence_properties(
        store,
        node.bid_data,
        node.bid_sub,
        &[ltp::PID_MESSAGE_SIZE],
        &crate::CancelToken::default(),
    )?;
    if pc
        .props
        .values()
        .any(|v| matches!(v, ltp::Value::MissingSubnode(_)))
    {
        return Err("An Item property payload is missing".into());
    }
    let mut pending = crate::nodes::Pending::new(node.bid_sub)?;
    let mut visited = crate::writer::spool::WideIndex::new()?;
    while let Some((bid, depth)) = pending.pop()? {
        if bid == 0 {
            continue;
        }
        if depth > 32 || visited.insert(bid, 0)?.is_some() {
            return Err("Cyclic or excessively nested Item subnodes".into());
        }
        for n in crate::nodes::Nodes::namespace(store, bid, &crate::CancelToken::default())?.iter()
        {
            let n = n?;
            let nid = n.nid;
            let sub = pstoff_format::ndb::Sub {
                data: n.bid_data,
                sub: n.bid_sub,
            };
            match nid & 31 {
                4 | 5 => {
                    crate::writer::evidence_properties(
                        store,
                        sub.data,
                        sub.sub,
                        &[],
                        &crate::CancelToken::default(),
                    )?;
                }
                0x11 | 0x12 => {
                    crate::writer::validate_table(store, sub.data, sub.sub)?;
                }
                _ => (),
            }
            pending.push(sub.sub, depth + 1)?;
        }
    }
    Ok(pc)
}

/// Conversion must retain this handle until its source reads finish. The same
/// classified, read-only handle is used for header, indexes, and all data blocks.
pub(crate) fn read_source(path: &Path) -> Result<(Pst, crate::nodes::Nodes), EngineError> {
    read_handle(open(path)?, &crate::CancelToken::default())
}

pub(crate) fn conversion_source(
    path: &Path,
    cancel: &crate::CancelToken,
) -> Result<(Pst, crate::nodes::Nodes, String), EngineError> {
    use sha2::{Digest, Sha256};
    let file = open(path)?;
    let mut identity = file
        .try_clone()
        .map_err(|e| error(ErrorKind::Io, e.to_string()))?;
    let (store, nodes) = read_handle(file, cancel)?;
    identity
        .seek(SeekFrom::Start(0))
        .map_err(|e| error(ErrorKind::Io, e.to_string()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        if cancel.is_cancelled() {
            return Err(error(ErrorKind::Io, "Source identity cancelled"));
        }
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

fn read_handle(
    mut file: File,
    cancel: &crate::CancelToken,
) -> Result<(Pst, crate::nodes::Nodes), EngineError> {
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
    let mut nodes = crate::nodes::Nodes::new().map_err(|e| error(ErrorKind::Io, e.to_string()))?;
    store
        .visit_nodes::<Box<dyn std::error::Error>>(|n| {
            if cancel.is_cancelled() {
                return Err("Source inventory cancelled".into());
            }
            nodes.push(n)
        })
        .map_err(|e| {
            error(
                if e.is::<std::io::Error>() {
                    ErrorKind::Io
                } else {
                    ErrorKind::DamagedSource
                },
                e.to_string(),
            )
        })?;
    if !store.warnings.is_empty() {
        return Err(error(
            ErrorKind::DamagedSource,
            "The OST node index could not be read completely.",
        ));
    }
    ltp::read_named_properties(
        &mut store,
        &nodes
            .iter()
            .find(|n| n.nid == ltp::NID_NAME_TO_ID_MAP)
            .map_err(|e| error(ErrorKind::Io, e.to_string()))?
            .into_iter()
            .collect::<Vec<_>>(),
    )
    .map_err(|_| {
        error(
            ErrorKind::DamagedSource,
            "The named-property map could not be read.",
        )
    })?;
    Ok((store, nodes))
}

pub(super) fn inspect(path: &Path) -> Result<Inspection, EngineError> {
    let folders = Vec::new();
    let support = match read_source(path) {
        Err(e) => match e.kind {
            ErrorKind::Unsupported(reason) => Support::Unsupported(reason),
            ErrorKind::InUse => Support::InUse,
            _ => return Err(e),
        },
        Ok((mut store, nodes)) => {
            return inspect_nodes(path, &mut store, &nodes);
        }
    };
    Ok(Inspection {
        source: path.into(),
        support,
        folders,
    })
}

// Conversion borrows its already classified source and node inventory. No second
// read-only handle, node vector or source block cache is opened for inspection.
pub(crate) fn inspect_nodes(
    path: &Path,
    store: &mut Pst,
    nodes: &crate::nodes::Nodes,
) -> Result<Inspection, EngineError> {
    let mut folders = Vec::new();
    let mut flat = BTreeMap::new();
    let mut parents = BTreeMap::new();
    let mut children_by_parent: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for node in nodes
        .iter()
        .filter(|n| n.nid_type() == 2 || n.nid_type() == 3)
    {
        let node = node.map_err(|e| error(ErrorKind::Io, e.to_string()))?;
        let mut folder = Folder {
            id: FolderRef(node.nid),
            name: String::new(),
            item_count: 0,
            bytes: 0,
            unreadable: None,
            children: vec![],
        };
        match crate::writer::evidence_properties(
            store,
            node.bid_data,
            node.bid_sub,
            &[ltp::PID_DISPLAY_NAME],
            &crate::CancelToken::default(),
        ) {
            Ok(mut pc) => {
                if let Some(ltp::Value::Str(name)) = pc.props.remove(&ltp::PID_DISPLAY_NAME) {
                    folder.name = name;
                }
            }
            Err(e) if e.is::<std::io::Error>() => {
                return Err(error(
                    ErrorKind::Io,
                    format!("Property staging failed: {e}"),
                ));
            }
            Err(_) => folder.unreadable = Some("Folder properties could not be read.".into()),
        }
        for item in nodes
            .iter()
            .filter(|n| n.nid_type() == 4 && n.nid_parent == node.nid)
        {
            let item = item.map_err(|e| error(ErrorKind::Io, e.to_string()))?;
            folder.item_count += 1;
            match item_properties(store, &item) {
                Ok(pc) => match pc.props.get(&ltp::PID_MESSAGE_SIZE) {
                    Some(ltp::Value::Int(size)) if *size >= 0 => {
                        folder.bytes = folder.bytes.saturating_add(*size as u64)
                    }
                    _ => folder.unreadable = Some("An Item has no readable size property.".into()),
                },
                Err(e) => {
                    if e.is::<std::io::Error>() {
                        return Err(error(ErrorKind::Io, format!("Table staging failed: {e}")));
                    }
                    folder.unreadable = Some("One or more Items could not be read.".into())
                }
            }
        }
        for table in nodes
            .iter()
            .filter(|n| matches!(n.nid_type(), 0x0d..=0x0f) && n.nid & !31 == node.nid & !31)
        {
            let table = table.map_err(|e| error(ErrorKind::Io, e.to_string()))?;
            if let Err(e) = crate::writer::validate_table(store, table.bid_data, table.bid_sub) {
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
    for &id in flat.keys() {
        // Floyd's walk needs no folder-count-sized visited/resolved sets.
        let parent = |id| parents.get(&id).copied().filter(|p| flat.contains_key(p));
        let mut slow = parent(id);
        let mut fast = parent(id).and_then(parent);
        while let (Some(s), Some(f)) = (slow, fast) {
            if s == f {
                return Err(error(
                    ErrorKind::DamagedSource,
                    "The folder hierarchy contains a cycle.",
                ));
            }
            slow = parent(s);
            fast = parent(f).and_then(parent);
        }
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
    Ok(Inspection {
        source: path.into(),
        support: Support::Supported,
        folders,
    })
}

// Conversion needs structural hierarchy validation, not the public display tree.
// Disk lookups and Floyd's walk retain no folder names or per-folder strings.
pub(crate) fn validate_hierarchy(
    nodes: &crate::nodes::Nodes,
    cancel: &crate::CancelToken,
) -> Result<(), EngineError> {
    let parent = |id: u32| -> Result<Option<u32>, EngineError> {
        let node = nodes
            .by_id(id)
            .map_err(|e| error(ErrorKind::Io, e.to_string()))?;
        Ok(node
            .filter(|n| matches!(n.nid_type(), 2 | 3))
            .and_then(|n| {
                if n.nid == ltp::NID_ROOT_FOLDER && n.nid_parent == n.nid {
                    None
                } else {
                    Some(n.nid_parent)
                }
            }))
    };
    for node in nodes.iter().filter(|n| matches!(n.nid_type(), 2 | 3)) {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let node = node.map_err(|e| error(ErrorKind::Io, e.to_string()))?;
        let mut slow = parent(node.nid)?;
        let mut fast = match slow {
            Some(id) => parent(id)?,
            None => None,
        };
        while let (Some(s), Some(f)) = (slow, fast) {
            if cancel.is_cancelled() {
                return Ok(());
            }
            if s == f {
                return Err(error(
                    ErrorKind::DamagedSource,
                    "The folder hierarchy contains a cycle.",
                ));
            }
            slow = parent(s)?;
            fast = match parent(f)? {
                Some(id) => parent(id)?,
                None => None,
            };
        }
    }
    Ok(())
}
