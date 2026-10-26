//! Unicode NDB emission. All offsets and references are assigned afresh.
mod cache;
mod context;
mod store;
mod table;
use outlook_pst::{PstFile, UnicodePstFile};
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
struct Output {
    file: File,
    pos: u64,
    next: u64,
    blocks: Vec<(u64, Vec<u8>)>,
    nodes: Vec<Node>,
    differences: Vec<crate::Difference>,
    highwater: u32,
    planned: std::collections::BTreeMap<u32, context::Properties>,
}
impl Output {
    // Reserve allocation-map slots, including PMAP/FMAP/FPMAP when applicable.
    fn place(&mut self, size: usize, align: u64) -> u64 {
        let mut at = self.pos.div_ceil(align) * align;
        loop {
            let section = (at.saturating_sub(AMAP)) / INTERVAL;
            let base = AMAP + section * INTERVAL;
            let pmap = section % 8 == 0;
            let fmap = pmap && section >= 128 && (section - 128) % 496 == 0;
            let fpmap = pmap && section >= 8192 && (section - 8192) % 496 == 0;
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
        self.blocks.push((id, entry));
        Ok(id)
    }
    fn stream(&mut self, blocks: &[Vec<u8>]) -> Result<u64> {
        let mut parts = Vec::new();
        for block in blocks {
            for b in block.chunks(MAX) {
                parts.push((self.block(b, false)?, b.len() as u64));
            }
        }
        if parts.is_empty() {
            return Ok(0);
        }
        if parts.len() == 1 {
            return Ok(parts[0].0);
        }
        if parts.len() > 1021 * 1021 {
            return Err("PST stream exceeds two XBLOCK levels".into());
        }
        if parts.len() <= 1021 {
            return self.array(&parts, 1);
        }
        let mut parents = Vec::new();
        for group in parts.chunks(1021) {
            parents.push((self.array(group, 1)?, group.iter().map(|p| p.1).sum()));
        }
        self.array(&parents, 2)
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
    fn subtree(&mut self, source: &mut Pst, bid: u64, depth: u32) -> Result<u64> {
        if bid == 0 {
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
        let blocks = source.node_blocks(bid)?;
        if blocks
            .first()
            .is_some_and(|b| b.len() > 3 && b[2] == 0xec && matches!(b[3], 0xbc | 0x7c))
        {
            let is_pc = blocks[0][3] == 0xbc;
            let cells = if is_pc {
                Some(match override_cells {
                    Some(c) => c.clone(),
                    None => context::properties(source, bid, sub)?,
                })
            } else {
                None
            };
            let mut table = if is_pc {
                None
            } else {
                Some(table::read(source, bid, sub)?)
            };
            if matches!(nid & 31, 0x0e..=0x10) {
                table
                    .as_mut()
                    .ok_or("Contents node is not a table")?
                    .rebuild_contents(&self.planned, &self.nodes, nid, &mut self.differences)?;
            }
            let mut entries = vec![];
            let mut subs: Vec<_> = source.subnodes(sub)?.into_iter().collect();
            subs.sort_by_key(|s| s.0);
            let mut next = subs.iter().map(|s| s.0 >> 5).max().unwrap_or(0) + 1;
            for (nid, s) in subs {
                let (data, below) = self.context(source, s.data, s.sub, depth + 1)?;
                entries.push((nid, data, below));
            }
            let mut heap = context::HeapBuilder::new(if is_pc { 0xbc } else { 0x7c });
            let mut payload = |blocks: &[Vec<u8>]| -> Result<u32> {
                if next > 0x07ffffff {
                    return Err("Subnode ID overflow".into());
                }
                let nid = (next << 5) | 0x1f;
                next += 1;
                self.highwater = self.highwater.max(next);
                let data = self.stream(blocks)?;
                entries.push((nid, data, 0));
                Ok(nid)
            };
            let root = if let Some(cells) = cells {
                context::pc(&cells, &mut heap, |bytes| payload(&[bytes.to_vec()]))?
            } else {
                table.unwrap().build(&mut heap, &mut payload)?
            };
            entries.sort_by_key(|s| s.0);
            let below = self.subentries(&entries)?;
            let data = self.stream(&heap.finish(root))?;
            Ok((data, below))
        } else {
            if blocks
                .first()
                .is_some_and(|b| b.len() > MAX && b.get(2) == Some(&0xec))
            {
                return Err("Oversized table heap requires rebuilding".into());
            }
            let data = if blocks.first().is_some_and(|b| b.get(2) == Some(&0xec)) {
                self.stream(&blocks)?
            } else {
                self.stream(&[blocks.concat()])?
            };
            Ok((data, self.subtree(source, sub, depth + 1)?))
        }
    }
    fn tree(
        &mut self,
        mut entries: Vec<(u64, Vec<u8>)>,
        kind: u8,
        mut width: usize,
    ) -> Result<(u64, u64)> {
        let mut level = 0;
        loop {
            let mut parents = Vec::new();
            for group in entries.chunks(488 / width) {
                let at = self.place(512, 512);
                let id = self.id(false);
                let mut b = vec![0; 512];
                for (i, (_, entry)) in group.iter().enumerate() {
                    b[i * width..(i + 1) * width].copy_from_slice(entry);
                }
                b[488] = group.len() as u8;
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
                let mut parent = vec![0; 24];
                p64(&mut parent, 0, group[0].0);
                p64(&mut parent, 8, id);
                p64(&mut parent, 16, at);
                parents.push((group[0].0, parent));
            }
            if parents.len() == 1 {
                let b = &parents[0].1;
                return Ok((
                    u64::from_le_bytes(b[8..16].try_into()?),
                    u64::from_le_bytes(b[16..24].try_into()?),
                ));
            }
            if parents.is_empty() {
                return Err("An empty index cannot be published".into());
            }
            entries = parents;
            width = 24;
            level += 1;
        }
    }
}

pub(super) fn write(
    source: &mut Pst,
    nodes: &[Node],
    path: &Path,
    display: &str,
    progress: &mut dyn crate::ProgressSink,
    cancel: &crate::CancelToken,
) -> Result<Option<Vec<crate::Difference>>> {
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
    let (store_cells, mut differences) = store::prepare(source, nodes, display)?;
    let Some(planned) = cache::prepare(source, nodes, &mut differences, cancel)? else {
        return Ok(None);
    };
    let file = std::fs::OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(path)?;
    let mut partial = Partial { path, keep: false };
    let mut out = Output {
        file,
        pos: AMAP + 2048,
        next: 1,
        blocks: vec![],
        nodes: nodes.to_vec(),
        differences,
        highwater: 1024,
        planned,
    };
    let mut entries = Vec::new();
    for node in nodes {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let override_cells = if node.nid == 0x21 {
            Some(store_cells.clone())
        } else {
            out.planned.get(&node.nid).cloned()
        };
        let (data, sub) = out.context_with_cells(
            source,
            node.bid_data,
            node.bid_sub,
            0,
            override_cells.as_ref(),
            node.nid,
        )?;
        let mut entry = vec![0; 32];
        p64(&mut entry, 0, node.nid as u64);
        p64(&mut entry, 8, data);
        p64(&mut entry, 16, sub);
        p32(&mut entry, 24, node.nid_parent);
        entries.push((node.nid as u64, entry));
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
    entries.sort_by_key(|e| e.0);
    let nbt = out.tree(entries, 0x81, 32)?;
    let blocks = std::mem::take(&mut out.blocks);
    let bbt = out.tree(blocks, 0x80, 24)?;
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
    pst.lock()?.flush()?;
    partial.keep = true;
    Ok(Some(differences))
}
