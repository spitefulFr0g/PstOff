//! Raw property values are never decoded and re-encoded during heap relocation.
use super::table_storage::{Blocks, Heap};
use super::{p16, p32, Result, MAX};
use pstoff_format::ndb::Pst;
use std::collections::BTreeMap;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Cell {
    pub kind: u16,
    // Relocation/selective reads may omit bytes when subnode is Some. Only
    // explicitly selected exact tags may be used as table/policy operands;
    // pc() reproduces the local reference without inspecting these bytes.
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
    read_properties(source, bid, sub, |_, _| true, None)
}
// Relocation preserves local subnode references in this context's namespace.
// Payload bytes are validated, but never retained merely to copy that reference.
pub(super) fn relocation_properties(
    source: &mut Pst,
    bid: u64,
    sub: u64,
    cancel: &crate::CancelToken,
) -> Result<Properties> {
    read_properties(source, bid, sub, |_, _| false, Some(cancel))
}
// Store policy needs only bounded comparison operands. All unrelated subnode
// payloads remain namespace-local references, including arbitrary custom values.
pub(super) fn store_properties(
    source: &mut Pst,
    bid: u64,
    sub: u64,
    display_bytes: usize,
    cancel: &crate::CancelToken,
) -> Result<Properties> {
    let mut cells = Properties::new();
    visit_properties(
        source,
        bid,
        sub,
        |id, _| match id {
            0x0ff9 => 17,
            0x3001 => display_bytes.saturating_add(1),
            0x67ff | 0x35df => 5,
            0x35e0 | 0x35e3 | 0x35e7 => 25,
            _ => 0,
        },
        Some(cancel),
        |id, cell| {
            cells.insert(id, cell);
            Ok(())
        },
    )?;
    Ok(cells)
}
pub(super) fn selected_properties(
    source: &mut Pst,
    bid: u64,
    sub: u64,
    tags: &std::collections::BTreeSet<u32>,
    cancel: &crate::CancelToken,
) -> Result<Properties> {
    read_properties(
        source,
        bid,
        sub,
        |id, kind| tags.contains(&((id as u32) << 16 | kind as u32)),
        Some(cancel),
    )
}
fn read_properties(
    source: &mut Pst,
    bid: u64,
    sub: u64,
    materialize: impl Fn(u16, u16) -> bool,
    cancel: Option<&crate::CancelToken>,
) -> Result<Properties> {
    let mut cells = Properties::new();
    let result = visit_properties(
        source,
        bid,
        sub,
        |id, kind| if materialize(id, kind) { usize::MAX } else { 0 },
        cancel,
        |id, cell| {
            cells.insert(id, cell);
            Ok(())
        },
    );
    if !cancel.is_some_and(crate::CancelToken::is_cancelled) {
        result?;
    }
    Ok(cells)
}

// Retain one heap allocation per depth, never the complete PC heap or a
// fan-out-sized pending vector. An ordered key cursor detects duplicates even
// when the caller discards every descriptor.
fn visit_properties(
    source: &mut Pst,
    bid: u64,
    sub: u64,
    materialize: impl Fn(u16, u16) -> usize,
    cancel: Option<&crate::CancelToken>,
    mut visit: impl FnMut(u16, Cell) -> Result<()>,
) -> Result<()> {
    let mut blocks = Blocks::new()?;
    for chunk in source.node_chunks(bid)? {
        if cancel.is_some_and(crate::CancelToken::is_cancelled) {
            return Err("Cancelled property read".into());
        }
        blocks.push(&chunk?)?;
    }
    let mut heap = Heap::new(blocks, source.ver)?;
    if heap.client_sig != 0xbc {
        return Err("Invalid property context signature".into());
    }
    let header = heap.item(heap.user_root)?;
    if header.len() != 8 || header[..3] != [0xb5, 2, 6] {
        return Err("Invalid property BTH".into());
    }
    let subs = source.subnodes(sub)?;
    let mut previous = None;
    fn walk(
        heap: &mut Heap,
        hid: u32,
        level: u8,
        previous: &mut Option<u16>,
        leaf: &mut impl FnMut(u16, u16, u32, &mut Heap) -> Result<()>,
    ) -> Result<()> {
        if level > 32 {
            return Err("Excessive property BTH depth".into());
        }
        let data = heap.item(hid)?;
        let width = if level == 0 { 8 } else { 6 };
        if !data.len().is_multiple_of(width) {
            return Err("Partial BTH entry".into());
        }
        for r in data.chunks_exact(width) {
            if level != 0 {
                walk(heap, u32at(r, 2)?, level - 1, previous, leaf)?;
            } else {
                let id = u16::from_le_bytes(r[..2].try_into()?);
                if previous.is_some_and(|last| id <= last) {
                    return Err("Duplicate or out-of-order property".into());
                }
                *previous = Some(id);
                leaf(
                    id,
                    u16::from_le_bytes(r[2..4].try_into()?),
                    u32at(r, 4)?,
                    heap,
                )?;
            }
        }
        Ok(())
    }
    walk(
        &mut heap,
        u32at(&header, 4)?,
        header[3],
        &mut previous,
        &mut |id, kind, value, heap| {
            if cancel.is_some_and(crate::CancelToken::is_cancelled) {
                return Err("Cancelled property read".into());
            }
            let wanted = materialize(id, kind);
            let bytes = if inline(kind) {
                value.to_le_bytes().to_vec()
            } else if value & 31 == 0 {
                heap.item(value)?
            } else {
                let node = subs.get(&value).ok_or("Missing property payload subnode")?;
                let mut bytes = Vec::new();
                for chunk in source.node_chunks(node.data)? {
                    if cancel.is_some_and(crate::CancelToken::is_cancelled) {
                        return Err("Cancelled property read".into());
                    }
                    let chunk = chunk?;
                    let n = wanted.saturating_sub(bytes.len()).min(chunk.len());
                    bytes.extend_from_slice(&chunk[..n]);
                }
                bytes
            };
            visit(
                id,
                Cell {
                    kind,
                    bytes,
                    subnode: (!inline(kind) && value & 31 != 0).then_some(value),
                },
            )
        },
    )
}

// Evidence and inspection read only their exact operands. Every other payload
// is still traversed for integrity, without collecting its bytes or descriptor.
pub(crate) fn evidence_properties(
    source: &mut Pst,
    bid: u64,
    sub: u64,
    ids: &[u16],
    cancel: &crate::CancelToken,
) -> Result<pstoff_format::ltp::Pc> {
    use pstoff_format::ltp::{Pc, Value, PID_MESSAGE_CODEPAGE};
    let mut selected = Properties::new();
    visit_properties(
        source,
        bid,
        sub,
        |id, kind| {
            if id == PID_MESSAGE_CODEPAGE {
                return if matches!(kind, 2 | 3 | 0xa | 6 | 0x14) {
                    8
                } else {
                    0
                };
            }
            if !ids.contains(&id) || id == 0x3701 {
                return 0;
            }
            match id {
                0x3001 | 0x3707 | 0x370d | 0x3708 if matches!(kind, 0x1e | 0x1f) => usize::MAX,
                0x0e08 if matches!(kind, 2 | 3 | 6 | 0x14) => 8,
                _ if kind == 3 => 4,
                _ => 0,
            }
        },
        Some(cancel),
        |id, cell| {
            if ids.contains(&id) || id == PID_MESSAGE_CODEPAGE {
                selected.insert(id, cell);
            }
            Ok(())
        },
    )?;
    let cp = selected.get(&PID_MESSAGE_CODEPAGE).and_then(|c| {
        let value = match c.kind {
            2 => i64::from(i16::from_le_bytes(c.bytes.get(..2)?.try_into().ok()?)),
            3 | 0xa => i64::from(i32::from_le_bytes(c.bytes.get(..4)?.try_into().ok()?)),
            6 | 0x14 => i64::from_le_bytes(c.bytes.get(..8)?.try_into().ok()?),
            _ => return None,
        };
        u32::try_from(value).ok()
    });
    let mut pc = Pc {
        props: BTreeMap::new(),
        types: BTreeMap::new(),
        from_subnode: 0,
    };
    for (id, cell) in selected {
        let b = cell.bytes;
        let value = match cell.kind {
            2 => Value::Int(i16::from_le_bytes(b[..2].try_into()?) as i64),
            3 | 0xa => Value::Int(i32::from_le_bytes(b[..4].try_into()?) as i64),
            6 | 0x14 if b.len() >= 8 => Value::Int(i64::from_le_bytes(b[..8].try_into()?)),
            0x1f => Value::Str(String::from_utf16_lossy(
                &b.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|r| u16::from_le_bytes([r[0], r[1]]))
                    .collect::<Vec<_>>(),
            )),
            0x1e => Value::Str(pstoff_format::html::decode(&b, cp)),
            _ => Value::Raw {
                ptype: cell.kind,
                bytes: b,
            },
        };
        pc.types.insert(id, cell.kind);
        pc.props.insert(id, value);
    }
    Ok(pc)
}

pub(super) struct HeapBuilder {
    pages: super::table_storage::Blocks,
    page: Vec<u8>,
    offsets: Vec<u16>,
    kind: u8,
}
impl HeapBuilder {
    pub fn new(kind: u8) -> Result<Self> {
        Ok(Self {
            pages: super::table_storage::Blocks::new()?,
            page: vec![0; 12],
            offsets: vec![],
            kind,
        })
    }
    fn flush(&mut self) -> Result<()> {
        let end = self.page.len();
        if !end.is_multiple_of(2) {
            self.page.push(0);
        }
        let map = self.page.len();
        self.page
            .extend_from_slice(&(self.offsets.len() as u16).to_le_bytes());
        self.page.extend_from_slice(&[0, 0]);
        for offset in &self.offsets {
            self.page.extend_from_slice(&offset.to_le_bytes());
        }
        self.page.extend_from_slice(&(end as u16).to_le_bytes());
        p16(&mut self.page, 0, map as u16);
        self.pages.push(&self.page)?;
        self.page.clear();
        self.offsets.clear();
        Ok(())
    }
    pub fn alloc(&mut self, value: &[u8]) -> Result<u32> {
        if value.is_empty() {
            return Ok(0);
        }
        if value.len() > 3580 {
            return Err("Heap allocation requires a payload subnode".into());
        }
        if self.page.len() + value.len() + 6 + (self.offsets.len() + 1) * 2 > MAX
            || self.offsets.len() >= 2047
        {
            self.flush()?;
            if self.pages.count > 65535 {
                return Err("Heap page index overflow".into());
            }
            let header = if self.pages.count % 128 == 8 { 66 } else { 2 };
            self.page.resize(header, 0);
        }
        self.offsets.push(self.page.len() as u16);
        self.page.extend_from_slice(value);
        Ok(self.pages.count << 16 | (self.offsets.len() as u32) << 5)
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
    pub fn finish(
        mut self,
        root: u32,
        cancel: &crate::CancelToken,
    ) -> Result<super::table_storage::Blocks> {
        self.flush()?;
        for i in 0..self.pages.count {
            if cancel.is_cancelled() {
                return Ok(self.pages);
            }
            let bytes = MAX - self.pages.get(i)?.len();
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
            let at = offset + index / 2;
            let page = self.pages.get(owner)?;
            self.pages.patch(
                owner,
                u64::from(at),
                &[page[at as usize] | level << ((index % 2) * 4)],
            )?;
        }
        self.pages.patch(0, 2, &[0xec, self.kind])?;
        self.pages.patch(0, 4, &root.to_le_bytes())?;
        Ok(self.pages)
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

pub(super) fn folder(name: &str, ordinary: u32, associated: u32, children: bool) -> Properties {
    let mut cells = Properties::new();
    cells.insert(
        0x3001,
        Cell {
            kind: 0x1f,
            bytes: name.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            subnode: None,
        },
    );
    for (id, value) in [
        (0x3602, ordinary),
        (0x3617, associated),
        (0x3603, 0),
        (0x360a, u32::from(children)),
    ] {
        cells.insert(
            id,
            Cell {
                kind: 3,
                bytes: value.to_le_bytes().to_vec(),
                subnode: None,
            },
        );
    }
    cells
}
