//! Raw property values are never decoded and re-encoded during heap relocation.
use super::{p16, p32, Result, MAX};
use pstoff_format::{ltp::Heap, ndb::Pst};
use std::collections::BTreeMap;

#[derive(Clone)]
pub(super) struct Cell {
    pub kind: u16,
    pub bytes: Vec<u8>,
    // PC payload subnodes are relocated recursively with their original local
    // NID. Preserve that reference rather than also copying the payload into
    // the heap (which inflates embedded-object composite sizes).
    pub subnode: Option<u32>,
}
pub(super) type Properties = BTreeMap<u16, Cell>;
fn u32at(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4).ok_or("Truncated context")?.try_into()?,
    ))
}
fn inline(kind: u16) -> bool {
    matches!(kind, 2 | 3 | 4 | 0x0a | 0x0b)
}
pub(super) fn properties(source: &mut Pst, bid: u64, sub: u64) -> Result<Properties> {
    let heap = Heap::new(source.node_blocks(bid)?)?;
    let header = heap.item(heap.user_root)?;
    if header.len() != 8 || header[..3] != [0xb5, 2, 6] {
        return Err("Invalid property BTH".into());
    }
    let mut pending = vec![(u32at(header, 4)?, header[3])];
    let mut cells = Properties::new();
    let subs = source.subnodes(sub)?;
    while let Some((hid, level)) = pending.pop() {
        if level > 32 {
            return Err("Excessive property BTH depth".into());
        }
        let data = heap.item(hid)?;
        let width = if level == 0 { 8 } else { 6 };
        if data.len() % width != 0 {
            return Err("Partial BTH entry".into());
        }
        for r in data.chunks_exact(width) {
            if level != 0 {
                pending.push((u32at(r, 2)?, level - 1));
                continue;
            }
            let id = u16::from_le_bytes(r[..2].try_into()?);
            let kind = u16::from_le_bytes(r[2..4].try_into()?);
            let value = u32at(r, 4)?;
            let bytes = if inline(kind) {
                r[4..8].to_vec()
            } else if value & 31 == 0 {
                heap.item(value)?.to_vec()
            } else {
                let node = subs.get(&value).ok_or("Missing property payload subnode")?;
                source.node_blocks(node.data)?.concat()
            };
            let subnode = (!inline(kind) && value & 31 != 0).then_some(value);
            if cells
                .insert(
                    id,
                    Cell {
                        kind,
                        bytes,
                        subnode,
                    },
                )
                .is_some()
            {
                return Err("Duplicate property".into());
            }
        }
    }
    Ok(cells)
}

pub(super) struct HeapBuilder {
    pages: Vec<Vec<u8>>,
    offsets: Vec<Vec<u16>>,
    kind: u8,
}
impl HeapBuilder {
    pub fn new(kind: u8) -> Self {
        Self {
            pages: vec![vec![0; 12]],
            offsets: vec![vec![]],
            kind,
        }
    }
    pub fn alloc(&mut self, value: &[u8]) -> Result<u32> {
        if value.is_empty() {
            return Ok(0);
        }
        if value.len() > 3580 {
            return Err("Heap allocation requires a payload subnode".into());
        }
        let mut i = self.pages.len() - 1;
        if self.pages[i].len() + value.len() + 6 + (self.offsets[i].len() + 1) * 2 > MAX
            || self.offsets[i].len() >= 2047
        {
            i += 1;
            if i > 65535 {
                return Err("Heap page index overflow".into());
            }
            let header = if i % 128 == 8 { 66 } else { 2 };
            self.pages.push(vec![0; header]);
            self.offsets.push(vec![]);
        }
        self.offsets[i].push(self.pages[i].len() as u16);
        self.pages[i].extend_from_slice(value);
        Ok((i as u32) << 16 | (self.offsets[i].len() as u32) << 5)
    }
    pub fn bth(&mut self, records: &[(Vec<u8>, Vec<u8>)], key: usize, value: usize) -> Result<u32> {
        let mut level = 0;
        let mut records = records.to_vec();
        let mut width = key + value;
        let root = loop {
            if records.is_empty() {
                break 0;
            }
            let mut parents = vec![];
            for chunk in records.chunks(3500 / width) {
                let mut bytes = vec![];
                for (k, v) in chunk {
                    bytes.extend_from_slice(k);
                    bytes.extend_from_slice(v);
                }
                let hid = self.alloc(&bytes)?;
                parents.push((chunk[0].0.clone(), hid.to_le_bytes().to_vec()));
            }
            if parents.len() == 1 {
                break u32at(&parents[0].1, 0)?;
            }
            records = parents;
            width = key + 4;
            level += 1;
        };
        let mut h = vec![0xb5, key as u8, value as u8, level, 0, 0, 0, 0];
        p32(&mut h, 4, root);
        self.alloc(&h)
    }
    pub fn finish(mut self, root: u32) -> Vec<Vec<u8>> {
        for (i, page) in self.pages.iter_mut().enumerate() {
            let end = page.len();
            if end % 2 != 0 {
                page.push(0);
            }
            let map = page.len();
            let count = self.offsets[i].len();
            self.offsets[i].push(end as u16);
            page.extend_from_slice(&(count as u16).to_le_bytes());
            page.extend_from_slice(&[0, 0]);
            for offset in &self.offsets[i] {
                page.extend_from_slice(&offset.to_le_bytes());
            }
            p16(page, 0, map as u16);
        }
        // Fill-level arrays cover the first eight pages, then groups of 128.
        let free: Vec<_> = self.pages.iter().map(|p| MAX - p.len()).collect();
        for (i, &bytes) in free.iter().enumerate() {
            let level = match bytes {
                3584.. => 0,
                2560.. => 1,
                2048.. => 2,
                1792.. => 3,
                1536.. => 4,
                1280.. => 5,
                1024.. => 6,
                768.. => 7,
                512.. => 8,
                256.. => 9,
                128.. => 10,
                64.. => 11,
                32.. => 12,
                16.. => 13,
                8.. => 14,
                _ => 15,
            };
            let (owner, index, offset) = if i < 8 {
                (0, i, 8)
            } else {
                (8 + (i - 8) / 128 * 128, (i - 8) % 128, 2)
            };
            self.pages[owner][offset + index / 2] |= level << ((index % 2) * 4);
        }
        let count = self.pages.len();
        for page in &mut self.pages[..count - 1] {
            page.resize(MAX, 0);
        }
        self.pages[0][2] = 0xec;
        self.pages[0][3] = self.kind;
        p32(&mut self.pages[0], 4, root);
        self.pages
    }
}
pub(super) fn pc(
    cells: &Properties,
    heap: &mut HeapBuilder,
    mut payload: impl FnMut(&[u8]) -> Result<u32>,
) -> Result<u32> {
    let mut records = vec![];
    for (id, cell) in cells {
        let value = if inline(cell.kind) {
            u32at(&cell.bytes, 0)?
        } else if let Some(nid) = cell.subnode {
            nid
        } else if cell.bytes.len() > 3580 {
            payload(&cell.bytes)?
        } else {
            heap.alloc(&cell.bytes)?
        };
        let mut bytes = cell.kind.to_le_bytes().to_vec();
        bytes.extend_from_slice(&value.to_le_bytes());
        records.push((id.to_le_bytes().to_vec(), bytes));
    }
    heap.bth(&records, 2, 6)
}
