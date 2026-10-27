//! Unicode NDB emission. All offsets and references are assigned afresh.
mod cache;
mod recovery;
pub(super) use recovery::prepare as recover;
pub(crate) use recovery::Recovery;
mod context;
mod spool;
mod store;
mod system;
mod table;
mod table_storage;
#[cfg(test)]
mod tests;
use outlook_pst::UnicodePstFile;
use pstoff_format::ndb::{crc32, Node, Pst};
use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    path::Path,
};

const MAX: usize = 8176;
const AMAP: u64 = 0x4400;
const INTERVAL: u64 = 496 * 8 * 64;
fn p16(b: &mut [u8], i: usize, n: u16) {
    b[i..i + 2].copy_from_slice(&n.to_le_bytes());
}
fn p32(b: &mut [u8], i: usize, n: u32) {
    b[i..i + 4].copy_from_slice(&n.to_le_bytes());
}
fn p64(b: &mut [u8], i: usize, n: u64) {
    b[i..i + 8].copy_from_slice(&n.to_le_bytes());
}
fn signature(at: u64, id: u64) -> u16 {
    let n = at ^ id;
    (n ^ (n >> 16)) as u16
}
struct Partial<'a> {
    path: &'a Path,
    keep: bool,
}
impl Drop for Partial<'_> {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(self.path);
        }
    }
}
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(super) fn validate_table(source: &mut Pst, bid: u64, sub: u64) -> Result<()> {
    table::inspect(source, bid, sub)
}
struct Output<'a> {
    cancel: crate::CancelToken,
    file: File,
    pos: u64,
    next: u64,
    blocks: spool::Spool,
    nodes: &'a [Node],
    differences: Vec<crate::Difference>,
    highwater: u32,
    planned: cache::Plan,
    omitted: std::collections::HashSet<u32>,
    created: std::collections::BTreeMap<u32, String>,
}
impl Output<'_> {
    // Reserve allocation-map slots, including PMAP/FMAP/FPMAP when applicable.
    fn place(&mut self, size: usize, align: u64) -> u64 {
        let mut at = self.pos.div_ceil(align) * align;
        loop {
            let section = (at.saturating_sub(AMAP)) / INTERVAL;
            let base = AMAP + section * INTERVAL;
            let pmap = section.is_multiple_of(8);
            let fmap = pmap && section >= 128 && (section - 128).is_multiple_of(496);
            let fpmap = pmap && section >= 8192 && (section - 8192).is_multiple_of(496);
            let slots = 1 + u64::from(pmap) + u64::from(fmap) + u64::from(fpmap);
            if at < base + slots * 512 {
                at = base + slots * 512;
                continue;
            }
            if at + size as u64 > base + INTERVAL {
                at = base + INTERVAL;
                continue;
            }
            return at;
        }
    }
    fn write(&mut self, at: u64, data: &[u8]) -> Result<()> {
        self.file.seek(SeekFrom::Start(at))?;
        self.file.write_all(data)?;
        self.pos = at + data.len() as u64;
        Ok(())
    }
    fn id(&mut self, internal: bool) -> u64 {
        let id = (self.next * 4) | if internal { 2 } else { 0 };
        self.next += 1;
        id
    }
    fn block(&mut self, bytes: &[u8], internal: bool) -> Result<u64> {
        if bytes.len() > MAX {
            return Err("PST block exceeds 8176 bytes".into());
        }
        let size = (bytes.len() + 16).div_ceil(64) * 64;
        let at = self.place(size, 64);
        let id = self.id(internal);
        let mut b = vec![0; size];
        b[..bytes.len()].copy_from_slice(bytes);
        if !internal {
            let mut inverse: Vec<u8> = (0..=255).collect();
            pstoff_format::crypt::permute_decode(&mut inverse);
            let mut encode = [0u8; 256];
            for (i, v) in inverse.into_iter().enumerate() {
                encode[v as usize] = i as u8;
            }
            for v in &mut b[..bytes.len()] {
                *v = encode[*v as usize];
            }
        }
        let t = size - 16;
        p16(&mut b, t, bytes.len() as u16);
        p16(&mut b, t + 2, signature(at, id));
        let crc = crc32(&b[..bytes.len()]);
        p32(&mut b, t + 4, crc);
        p64(&mut b, t + 8, id);
        self.write(at, &b)?;
        let mut entry = vec![0; 24];
        p64(&mut entry, 0, id);
        p64(&mut entry, 8, at);
        p16(&mut entry, 16, bytes.len() as u16);
        p16(&mut entry, 18, 2);
        self.blocks.append(&entry)?;
        Ok(id)
    }
    fn array(&mut self, parts: &[(u64, u64)], level: u8) -> Result<u64> {
        let mut b = vec![0; 8 + parts.len() * 8];
        b[0] = 1;
        b[1] = level;
        p16(&mut b, 2, parts.len() as u16);
        // cbTotal is the sum of the logical bytes below this XBLOCK, not
        // full-sized slots. Heap pages may be short before the final page.
        let total: u64 = parts.iter().map(|p| p.1).sum();
        p32(&mut b, 4, u32::try_from(total)?);
        for (i, p) in parts.iter().enumerate() {
            p64(&mut b, 8 + i * 8, p.0);
        }
        self.block(&b, true)
    }
    // Keep only one data buffer and one bounded group per XBLOCK level.
    fn source_stream(&mut self, source: &mut Pst, bid: u64, heap: bool) -> Result<u64> {
        self.stream_chunks(
            &mut source.node_chunks(bid)?.map(|b| {
                let bytes = b?;
                if heap && bytes.len() > MAX {
                    return Err("Oversized table heap requires rebuilding".into());
                }
                Ok(bytes)
            }),
            heap,
        )
    }
    fn stream_chunks(
        &mut self,
        chunks: &mut dyn Iterator<Item = Result<Vec<u8>>>,
        heap: bool,
    ) -> Result<u64> {
        let mut leaves = Vec::with_capacity(1021);
        let mut parents = Vec::with_capacity(1021);
        let mut buffer = Vec::with_capacity(MAX);
        for chunk in chunks {
            if self.cancel.is_cancelled() {
                return Ok(0);
            }
            let chunk = chunk?;
            let mut rest = chunk.as_slice();
            while !rest.is_empty() {
                let take = rest.len().min(MAX - buffer.len());
                buffer.extend_from_slice(&rest[..take]);
                rest = &rest[take..];
                if buffer.len() == MAX || heap {
                    leaves.push((self.block(&buffer, false)?, buffer.len() as u64));
                    buffer.clear();
                    if leaves.len() == 1021 {
                        if parents.len() == 1021 {
                            return Err("PST stream exceeds two XBLOCK levels".into());
                        }
                        parents.push((self.array(&leaves, 1)?, leaves.iter().map(|p| p.1).sum()));
                        leaves.clear();
                    }
                }
            }
        }
        if !buffer.is_empty() {
            leaves.push((self.block(&buffer, false)?, buffer.len() as u64));
        }
        if parents.is_empty() {
            return match leaves.len() {
                0 => Ok(0),
                1 => Ok(leaves[0].0),
                _ => self.array(&leaves, 1),
            };
        }
        if !leaves.is_empty() {
            if parents.len() == 1021 {
                return Err("PST stream exceeds two XBLOCK levels".into());
            }
            parents.push((self.array(&leaves, 1)?, leaves.iter().map(|p| p.1).sum()));
        }
        if parents.len() == 1 {
            Ok(parents[0].0)
        } else {
            self.array(&parents, 2)
        }
    }
    fn subtree(&mut self, source: &mut Pst, bid: u64, depth: u32) -> Result<u64> {
        if bid == 0 || self.cancel.is_cancelled() {
            return Ok(0);
        }
        if depth > 32 {
            return Err("Cyclic or excessive subnode depth".into());
        }
        let mut subs: Vec<_> = source.subnodes(bid)?.into_iter().collect();
        subs.sort_by_key(|s| s.0);
        let mut entries = Vec::new();
        for (nid, sub) in subs {
            let (data, below) = self.context(source, sub.data, sub.sub, depth + 1)?;
            entries.push((nid, data, below));
        }
        self.subentries(&entries)
    }
    fn subentries(&mut self, entries: &[(u32, u64, u64)]) -> Result<u64> {
        if entries.is_empty() {
            return Ok(0);
        }
        if entries.len() > 340 * 510 {
            return Err("Subnode tree too large".into());
        }
        let mut leaves = Vec::new();
        for group in entries.chunks(340) {
            let mut b = vec![0; 8 + group.len() * 24];
            b[0] = 2;
            p16(&mut b, 2, group.len() as u16);
            for (i, (nid, data, sub)) in group.iter().enumerate() {
                p64(&mut b, 8 + i * 24, *nid as u64);
                p64(&mut b, 16 + i * 24, *data);
                p64(&mut b, 24 + i * 24, *sub);
            }
            leaves.push((group[0].0, self.block(&b, true)?));
        }
        if leaves.len() == 1 {
            return Ok(leaves[0].1);
        }
        let mut b = vec![0; 8 + leaves.len() * 16];
        b[0] = 2;
        b[1] = 1;
        p16(&mut b, 2, leaves.len() as u16);
        for (i, (nid, bid)) in leaves.iter().enumerate() {
            p64(&mut b, 8 + i * 16, *nid as u64);
            p64(&mut b, 16 + i * 16, *bid);
        }
        self.block(&b, true)
    }
    fn context(&mut self, source: &mut Pst, bid: u64, sub: u64, depth: u32) -> Result<(u64, u64)> {
        self.context_with_cells(source, bid, sub, depth, None, 0)
    }
    fn context_with_cells(
        &mut self,
        source: &mut Pst,
        bid: u64,
        sub: u64,
        depth: u32,
        override_cells: Option<&context::Properties>,
        nid: u32,
    ) -> Result<(u64, u64)> {
        if depth > 32 {
            return Err("Excessive context depth".into());
        }
        if self.cancel.is_cancelled() {
            return Ok((0, 0));
        }
        let first = source.node_chunks(bid)?.next().transpose()?;
        if first
            .as_ref()
            .is_some_and(|b| b.len() > 3 && b[2] == 0xec && matches!(b[3], 0xbc | 0x7c))
        {
            let is_pc = first.as_ref().unwrap()[3] == 0xbc;
            drop(first);
            let cells = if is_pc {
                Some(match override_cells {
                    Some(c) => c.clone(),
                    None => context::relocation_properties(source, bid, sub, &self.cancel)?,
                })
            } else {
                None
            };
            let mut table = if is_pc {
                None
            } else {
                Some(table::read(source, bid, sub, &self.cancel)?)
            };
            if self.cancel.is_cancelled() {
                return Ok((0, 0));
            }
            if let Some(table) = &mut table {
                // Omitted references are global NIDs. Nested recipient/attachment
                // table row IDs live in local namespaces and must remain untouched.
                if matches!(nid & 31, 0xd..=0x10) {
                    table
                        .rows
                        .rewrite(|(row, _, _)| Ok(!self.omitted.contains(row)))?;
                }
                if nid & 31 == 0xd {
                    for n in self.nodes.iter().filter(|n| {
                        self.created.contains_key(&n.nid)
                            && n.nid != (nid & !31 | 2)
                            && n.nid_parent == (nid & !31 | 2)
                    }) {
                        table.add_identity_row(n.nid)?;
                    }
                    // Hierarchy rows cache child counts. Keep those caches in
                    // agreement with the selected folder property contexts.
                    table.rows.rewrite(|(folder, _, cells)| {
                        for (tag, cell) in cells.iter_mut() {
                            let value = match tag >> 16 {
                                0x3602 | 0x3617 => {
                                    let kind = if tag >> 16 == 0x3602 { 4 } else { 8 };
                                    Some(
                                        self.nodes
                                            .iter()
                                            .filter(|n| {
                                                n.nid_parent == *folder && n.nid_type() == kind
                                            })
                                            .count() as u32,
                                    )
                                }
                                0x360a => Some(u32::from(self.nodes.iter().any(|n| {
                                    n.nid != *folder
                                        && n.nid_parent == *folder
                                        && matches!(n.nid_type(), 2 | 3)
                                }))),
                                _ => None,
                            };
                            if let Some(value) = value {
                                let bytes = if cell.kind == 0xb {
                                    vec![u8::from(value != 0)]
                                } else {
                                    value.to_le_bytes().to_vec()
                                };
                                if cell.bytes != bytes {
                                    cell.bytes = bytes;
                                    self.differences.push(crate::Difference {
                                        property: *tag,
                                        item: None,
                                        source: format!("Hierarchy table 0x{nid:x}, Folder 0x{folder:x}"),
                                        reason: "Folder cache reflects Items and children selected for this part.".into(),
                                    });
                                }
                            }
                        }
                        Ok(true)
                    })?;
                }
            }
            if matches!(nid & 31, 0x0e..=0x10) {
                let table_tags = table
                    .as_ref()
                    .ok_or("Contents node is not a table")?
                    .columns
                    .iter()
                    .map(|c| c.0)
                    .collect::<std::collections::BTreeSet<_>>();
                table
                    .as_mut()
                    .ok_or("Contents node is not a table")?
                    .rebuild_contents(
                        |item| {
                            let node = self
                                .nodes
                                .iter()
                                .find(|n| n.nid == item)
                                .ok_or("Missing indexed Item")?;
                            let mut cells = context::selected_properties(
                                source,
                                node.bid_data,
                                node.bid_sub,
                                &table_tags,
                                &self.cancel,
                            )?;
                            self.planned.apply(item, &mut cells)?;
                            Ok(cells)
                        },
                        self.nodes,
                        nid,
                        &mut self.differences,
                    )?;
            }
            let mut entries = vec![];
            let mut subs: Vec<_> = source.subnodes(sub)?.into_iter().collect();
            subs.sort_by_key(|s| s.0);
            let mut next = subs.iter().map(|s| s.0 >> 5).max().unwrap_or(0) + 1;
            for (nid, s) in subs {
                if self.cancel.is_cancelled() {
                    return Ok((0, 0));
                }
                let (data, below) = self.context(source, s.data, s.sub, depth + 1)?;
                entries.push((nid, data, below));
            }
            let mut heap = context::HeapBuilder::new(if is_pc { 0xbc } else { 0x7c })?;
            let mut payload = |blocks: &mut dyn Iterator<Item = Result<Vec<u8>>>| -> Result<u32> {
                if next > 0x07ffffff {
                    return Err("Subnode ID overflow".into());
                }
                let nid = (next << 5) | 0x1f;
                next += 1;
                self.highwater = self.highwater.max(next);
                let data = self.stream_chunks(blocks, true)?;
                entries.push((nid, data, 0));
                Ok(nid)
            };
            let root = if let Some(cells) = cells {
                context::pc(&cells, &mut heap, |bytes| {
                    payload(&mut std::iter::once(Ok(bytes.to_vec())))
                })?
            } else {
                table.unwrap().build(&mut heap, &mut payload)?
            };
            entries.sort_by_key(|s| s.0);
            let below = self.subentries(&entries)?;
            let data = self.stream_chunks(&mut heap.finish(root, &self.cancel)?.iter(), true)?;
            Ok((data, below))
        } else {
            let heap = first.as_ref().is_some_and(|b| b.get(2) == Some(&0xec));
            drop(first);
            let data = self.source_stream(source, bid, heap)?;
            Ok((data, self.subtree(source, sub, depth + 1)?))
        }
    }
    // Every level is a fixed-width disk stream. Only one page and one
    // parent record are resident, independent of the number of entries.
    fn tree(
        &mut self,
        mut entries: spool::Spool,
        kind: u8,
        mut width: usize,
    ) -> Result<(u64, u64)> {
        let mut level = 0u8;
        loop {
            if entries.len == 0 || !entries.len.is_multiple_of(width as u64) {
                return Err("Empty or truncated output index".into());
            }
            let mut parents = spool::Spool::new()?;
            let mut offset = 0;
            let mut root = (0, 0);
            while offset < entries.len {
                if self.cancel.is_cancelled() {
                    return Err("Output index emission cancelled".into());
                }
                let count =
                    ((entries.len - offset) / width as u64).min((488 / width) as u64) as usize;
                let at = self.place(512, 512);
                let id = self.id(false);
                let mut b = [0; 512];
                entries.read(offset, &mut b[..count * width])?;
                offset += (count * width) as u64;
                let key = u64::from_le_bytes(b[..8].try_into()?);
                b[488] = count as u8;
                b[489] = (488 / width) as u8;
                b[490] = width as u8;
                b[491] = level;
                b[496] = kind;
                b[497] = kind;
                p16(&mut b, 498, signature(at, id));
                let crc = crc32(&b[..496]);
                p32(&mut b, 500, crc);
                p64(&mut b, 504, id);
                self.write(at, &b)?;
                let mut parent = [0; 24];
                p64(&mut parent, 0, key);
                p64(&mut parent, 8, id);
                p64(&mut parent, 16, at);
                parents.append(&parent)?;
                root = (id, at);
            }
            if parents.len == 24 {
                return Ok(root);
            }
            entries = parents;
            width = 24;
            level = level.checked_add(1).ok_or("Output index depth overflow")?;
        }
    }
}

// Sort fixed-width NBT records by their disk-indexed NID without retaining nodes.
fn ordered_nodes(
    mut entries: spool::Spool,
    mut order: spool::Index,
    cancel: &crate::CancelToken,
) -> Result<spool::Spool> {
    let mut sorted = spool::Spool::new()?;
    order.visit(cancel, |_, offset| {
        let mut entry = [0; 32];
        entries.read(offset, &mut entry)?;
        sorted.append(&entry)?;
        Ok(())
    })?;
    Ok(sorted)
}

pub(super) fn write(
    source: &mut Pst,
    recovery: &recovery::Recovery,
    path: &Path,
    display: &str,
    progress: &mut dyn crate::ProgressSink,
    cancel: &crate::CancelToken,
) -> Result<Option<Vec<crate::Difference>>> {
    let nodes = &recovery.nodes;
    let omitted = &recovery.omitted;
    let total = nodes
        .iter()
        .filter(|n| matches!(n.nid_type(), 4 | 8))
        .count() as u64;
    let mut done = 0;
    progress.update(crate::Progress {
        fraction: 0.0,
        current_folder: None,
        done,
        total,
        part: 1,
    });
    if cancel.is_cancelled() {
        return Ok(None);
    }
    let (store_cells, mut differences) = store::prepare(source, nodes, display, &recovery.system)?;
    let Some(planned) =
        cache::prepare(source, nodes, &mut differences, cancel, omitted).map_err(|error| {
            crate::EngineError {
                kind: if error.is::<std::io::Error>() {
                    crate::ErrorKind::Io
                } else {
                    crate::ErrorKind::DamagedSource
                },
                message: format!("Source cache planning failed: {error}"),
                completed_files: vec![],
            }
        })?
    else {
        return Ok(None);
    };
    let file = std::fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(path)?;
    let mut partial = Partial { path, keep: false };
    let mut out = Output {
        cancel: cancel.clone(),
        file,
        pos: AMAP + 2048,
        next: 1,
        blocks: spool::Spool::new()?,
        nodes,
        differences,
        highwater: 1024,
        planned,
        omitted: omitted.clone(),
        created: recovery.created.clone(),
    };
    let mut entries = spool::Spool::new()?;
    let mut order = spool::Index::new()?;
    for node in nodes {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let override_cells = if recovery.created.contains_key(&node.nid) {
            Some(context::folder(
                &recovery.created[&node.nid],
                0,
                0,
                nodes
                    .iter()
                    .any(|n| n.nid != node.nid && n.nid_parent == node.nid),
            ))
        } else if !omitted.is_empty() && matches!(node.nid_type(), 2 | 3) {
            let mut cells = context::properties(source, node.bid_data, node.bid_sub)?;
            for (id, kind) in [(0x3602, 4), (0x3617, 8)] {
                if let Some(cell) = cells.get_mut(&id) {
                    let count = nodes
                        .iter()
                        .filter(|n| n.nid_parent == node.nid && n.nid_type() == kind)
                        .count() as u32;
                    let bytes = count.to_le_bytes().to_vec();
                    if cell.bytes != bytes {
                        out.differences.push(crate::Difference { property: (id as u32) << 16 | cell.kind as u32, item: None, source: format!("Folder 0x{:x}", node.nid), reason: "Folder count reflects Items selected for this part after exclusions and reported skips.".into() });
                        cell.bytes = bytes;
                    }
                }
            }
            if let Some(cell) = cells.get_mut(&0x360a) {
                let has_children = nodes.iter().any(|n| {
                    n.nid != node.nid && n.nid_parent == node.nid && matches!(n.nid_type(), 2 | 3)
                });
                let bytes = if cell.kind == 0xb {
                    vec![u8::from(has_children)]
                } else {
                    u32::from(has_children).to_le_bytes().to_vec()
                };
                if cell.bytes != bytes {
                    cell.bytes = bytes;
                    out.differences.push(crate::Difference {
                        property: 0x360a0000 | cell.kind as u32,
                        item: None,
                        source: format!("Folder 0x{:x}", node.nid),
                        reason: "Folder child flag reflects folders selected for this part.".into(),
                    });
                }
            }
            Some(cells)
        } else if node.nid == 0x21 {
            Some(store_cells.clone())
        } else if matches!(node.nid_type(), 4 | 8) {
            let mut cells =
                context::relocation_properties(source, node.bid_data, node.bid_sub, cancel)?;
            out.planned.apply(node.nid, &mut cells)?;
            Some(cells)
        } else {
            None
        };
        let generated_table = matches!(node.nid_type(), 0xd..=0xf)
            && recovery.created.keys().any(|id| id & !31 == node.nid & !31);
        let (data, sub) = if recovery.created.contains_key(&node.nid) || generated_table {
            let mut heap = context::HeapBuilder::new(if generated_table { 0x7c } else { 0xbc })?;
            let root = if generated_table {
                let rows = if node.nid_type() == 0xd {
                    nodes
                        .iter()
                        .filter(|n| {
                            n.nid != (node.nid & !31 | 2)
                                && n.nid_parent == (node.nid & !31 | 2)
                                && matches!(n.nid_type(), 2 | 3)
                        })
                        .map(|n| n.nid)
                        .collect()
                } else {
                    vec![]
                };
                table::empty(rows)?.build(&mut heap, |_| {
                    Err("Unexpected generated table payload".into())
                })?
            } else {
                out.differences.push(crate::Difference { property: 0x3001001f, item: None,
                    source: format!("Output system folder 0x{:x}", node.nid),
                    reason: "Created empty required PST scaffolding; no source mail recovered into this folder.".into() });
                context::pc(override_cells.as_ref().unwrap(), &mut heap, |_| {
                    Err("Unexpected generated folder payload".into())
                })?
            };
            (
                out.stream_chunks(&mut heap.finish(root, cancel)?.iter(), true)?,
                0,
            )
        } else {
            out.context_with_cells(
                source,
                node.bid_data,
                node.bid_sub,
                0,
                override_cells.as_ref(),
                node.nid,
            )?
        };
        let mut entry = vec![0; 32];
        p64(&mut entry, 0, node.nid as u64);
        p64(&mut entry, 8, data);
        p64(&mut entry, 16, sub);
        p32(&mut entry, 24, node.nid_parent);
        let offset = entries.append(&entry)?;
        if order.insert(node.nid, offset)?.is_some() {
            return Err("Duplicate output NID".into());
        }
        if matches!(node.nid_type(), 4 | 8) {
            done += 1;
            progress.update(crate::Progress {
                fraction: 0.9 * done as f64 / total.max(1) as f64,
                current_folder: Some(crate::FolderRef(node.nid_parent)),
                done,
                total,
                part: 1,
            });
        }
    }
    if cancel.is_cancelled() {
        return Ok(None);
    }
    let sorted = ordered_nodes(entries, order, cancel)?;
    if cancel.is_cancelled() {
        return Ok(None);
    }
    let nbt = match out.tree(sorted, 0x81, 32) {
        Err(_) if cancel.is_cancelled() => return Ok(None),
        result => result?,
    };
    let blocks = std::mem::replace(&mut out.blocks, spool::Spool::new()?);
    let bbt = match out.tree(blocks, 0x80, 24) {
        Err(_) if cancel.is_cancelled() => return Ok(None),
        result => result?,
    };
    let eof = out.pos.div_ceil(512) * 512;
    out.file.set_len(eof)?;
    let mut h = source.header_bytes()?;
    h.resize(564, 0);
    h[24..44].fill(0);
    h[172..256].fill(0);
    h[..4].copy_from_slice(b"!BDN");
    h[8..10].copy_from_slice(b"SM");
    p16(&mut h, 10, 23);
    p16(&mut h, 12, 19);
    h[14] = 1;
    h[15] = 1;
    p64(&mut h, 32, out.next * 4);
    p32(&mut h, 40, 1);
    for i in 0..32 {
        let previous = u32::from_le_bytes(h[44 + i * 4..48 + i * 4].try_into()?);
        let next = nodes
            .iter()
            .filter(|n| n.nid_type() as usize == i)
            .map(|n| (n.nid >> 5) + 1)
            .max()
            .unwrap_or(1024)
            .max(1024)
            .max(previous);
        p32(&mut h, 44 + i * 4, next);
    }
    let local = u32::from_le_bytes(h[168..172].try_into()?);
    p32(&mut h, 168, local.max(out.highwater));
    p64(&mut h, 184, eof);
    p64(&mut h, 192, AMAP + (eof - AMAP - 1) / INTERVAL * INTERVAL);
    p64(&mut h, 216, nbt.0);
    p64(&mut h, 224, nbt.1);
    p64(&mut h, 232, bbt.0);
    p64(&mut h, 240, bbt.1);
    h[256..512].fill(0xff);
    h[512] = 0x80;
    h[513] = 1;
    p64(&mut h, 516, out.next * 4);
    let crc = crc32(&h[8..479]);
    p32(&mut h, 4, crc);
    let crc = crc32(&h[8..524]);
    p32(&mut h, 524, crc);
    out.write(0, &h)?;
    out.file.sync_all()?;
    let differences = std::mem::take(&mut out.differences);
    drop(out);
    let mut pst = UnicodePstFile::open(path)?;
    match pst.finalize_with_cancel(&|| cancel.is_cancelled()) {
        Err(_) if cancel.is_cancelled() => return Ok(None),
        result => result?,
    }
    drop(pst);
    if cancel.is_cancelled() {
        return Ok(None);
    }
    partial.keep = true;
    Ok(Some(differences))
}
