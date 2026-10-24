//! Rebuild table storage and contents caches without interpreting property payloads.
use super::{
    context::{self, Cell, HeapBuilder, Properties},
    p16, p32, Result, MAX,
};
use crate::{Difference, FolderRef, ItemRef};
use pstoff_format::{
    ltp::Heap,
    ndb::{Node, Pst},
};
use std::collections::BTreeMap;
fn u16at(b: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2).ok_or("Truncated table")?.try_into()?,
    ))
}
fn u32at(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4).ok_or("Truncated table")?.try_into()?,
    ))
}
fn variable(kind: u16) -> bool {
    matches!(kind, 0x1e | 0x1f | 0x102 | 0x0d) || kind & 0x1000 != 0
}
pub(super) struct Table {
    columns: Vec<(u32, usize)>,
    rows: Vec<(u32, u32, Properties)>,
}
pub(super) fn read(source: &mut Pst, bid: u64, sub: u64) -> Result<Table> {
    let heap = Heap::new(source.node_blocks(bid)?)?;
    let info = heap.item(heap.user_root)?;
    if info.len() < 22 || info[0] != 0x7c {
        return Err("Invalid table context".into());
    }
    let width = u16at(info, 8)? as usize;
    let ceb = u16at(info, 6)? as usize;
    if width < 8 || ceb > width {
        return Err("Invalid table width".into());
    }
    let columns = (0..info[1] as usize)
        .map(|i| {
            let at = 22 + i * 8;
            Ok((
                u32at(info, at)?,
                u16at(info, at + 4)? as usize,
                *info.get(at + 6).ok_or("Truncated column")? as usize,
                *info.get(at + 7).ok_or("Truncated column")? as usize,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let subs = source.subnodes(sub)?;
    let resolve = |source: &mut Pst, hnid: u32| -> Result<Vec<Vec<u8>>> {
        if hnid == 0 {
            Ok(vec![])
        } else if hnid & 31 == 0 {
            Ok(vec![heap.item(hnid)?.to_vec()])
        } else {
            Ok(source.node_blocks(subs.get(&hnid).ok_or("Missing table payload")?.data)?)
        }
    };
    let mut rows = vec![];
    for block in resolve(source, u32at(info, 14)?)? {
        for row in block.chunks_exact(width) {
            let mut cells = Properties::new();
            for (tag, at, size, bit) in &columns {
                if at + size > ceb || bit / 8 >= width - ceb {
                    return Err("Invalid table cell".into());
                }
                if row[ceb + bit / 8] & (0x80 >> (bit % 8)) == 0 {
                    continue;
                }
                let kind = *tag as u16;
                let bytes = if variable(kind) {
                    resolve(source, u32at(row, *at)?)?.concat()
                } else {
                    row[*at..at + size].to_vec()
                };
                cells.insert((tag >> 16) as u16, Cell { kind, bytes });
            }
            rows.push((u32at(row, 0)?, u32at(row, 4)?, cells));
        }
    }
    Ok(Table {
        columns: columns.into_iter().map(|(t, _, s, _)| (t, s)).collect(),
        rows,
    })
}
impl Table {
    pub fn rebuild_contents(
        &mut self,
        source: &mut Pst,
        nodes: &[Node],
        nid: u32,
        differences: &mut Vec<Difference>,
    ) -> Result<()> {
        for (row, _, cells) in &mut self.rows {
            let node = nodes
                .iter()
                .find(|n| n.nid == *row)
                .ok_or("Contents row has no Item")?;
            let own = context::properties(source, node.bid_data, node.bid_sub)?;
            for (tag, _) in &self.columns {
                let id = (tag >> 16) as u16;
                let kind = *tag as u16;
                if matches!(id, 0x67f2 | 0x67f3) {
                    continue;
                }
                let previous = cells.get(&id);
                let counterpart = own.get(&id).filter(|c| c.kind == kind);
                let reason = if let Some(value) = counterpart {
                    let changed =
                        previous.is_none_or(|c| c.kind != value.kind || c.bytes != value.bytes);
                    cells.insert(id, value.clone());
                    if !changed {
                        continue;
                    }
                    "Q16: contents cell rebuilt from the Item's own exact property tag."
                } else if *tag == 0x66280014 && previous.is_some() {
                    cells.remove(&id);
                    "Q17: omit undocumented table-only cell without an exact Item property."
                } else {
                    continue;
                };
                differences.push(Difference {
                    property: *tag,
                    item: Some(ItemRef {
                        folder: FolderRef(node.nid_parent),
                        node: *row,
                    }),
                    source: format!("Contents table 0x{nid:x}, same Item property context"),
                    reason: reason.into(),
                });
            }
        }
        // Required contents schema columns have absent cells, never invented defaults.
        for tag in [
            0x0e300102, 0x0e330014, 0x0e340102, 0x0e3d0102, 0x0e3c0102, 0x0e380003,
        ] {
            if !self.columns.iter().any(|c| c.0 == tag) {
                self.columns
                    .push((tag, if tag as u16 == 0x14 { 8 } else { 4 }));
            }
        }
        Ok(())
    }
    pub fn build(
        &self,
        heap: &mut HeapBuilder,
        mut payload: impl FnMut(&[Vec<u8>]) -> Result<u32>,
    ) -> Result<u32> {
        let mut columns = self.columns.clone();
        columns.sort_by_key(|(tag, size)| {
            if *tag == 0x67f20003 {
                (0, 0)
            } else if *tag == 0x67f30003 {
                (0, 1)
            } else {
                (
                    if *size >= 4 {
                        1
                    } else if *size == 2 {
                        2
                    } else {
                        3
                    },
                    *tag,
                )
            }
        });
        if columns.len() > 255 {
            return Err("Too many table columns".into());
        }
        let bits: BTreeMap<_, _> = columns
            .iter()
            .enumerate()
            .map(|(i, (tag, _))| (*tag, i))
            .collect();
        let mut offsets = BTreeMap::new();
        let mut width = 0;
        let mut ends = [0usize; 3];
        for (tag, size) in &columns {
            offsets.insert(*tag, width);
            width += size;
        }
        for (i, end) in ends.iter_mut().enumerate() {
            *end = columns
                .iter()
                .filter(|(_, s)| {
                    if i == 0 {
                        *s >= 4
                    } else if i == 1 {
                        *s >= 2
                    } else {
                        true
                    }
                })
                .map(|(_, s)| *s)
                .sum();
        }
        columns.sort_by_key(|c| c.0);
        let ceb = width;
        width += columns.len().div_ceil(8);
        if width > MAX {
            return Err("Table row exceeds a PST block".into());
        }
        let mut matrix = vec![];
        let mut rows = vec![];
        let mut index = vec![];
        for (i, (id, unique, cells)) in self.rows.iter().enumerate() {
            let mut row = vec![0; width];
            p32(&mut row, 0, *id);
            p32(&mut row, 4, *unique);
            for (tag, size) in &columns {
                let bit = bits[tag];
                let Some(cell) = cells
                    .get(&((tag >> 16) as u16))
                    .filter(|c| c.kind == *tag as u16)
                else {
                    continue;
                };
                let at = offsets[tag];
                if variable(cell.kind) {
                    let hnid = if cell.bytes.len() > 3580 {
                        payload(std::slice::from_ref(&cell.bytes))?
                    } else {
                        heap.alloc(&cell.bytes)?
                    };
                    p32(&mut row, at, hnid);
                } else {
                    row[at..at + size].copy_from_slice(
                        cell.bytes.get(..*size).ok_or("Invalid fixed cell width")?,
                    );
                }
                row[ceb + bit / 8] |= 0x80 >> (bit % 8);
            }
            rows.extend_from_slice(&row);
            index.push((id.to_le_bytes().to_vec(), (i as u32).to_le_bytes().to_vec()));
            if rows.len() + width > MAX {
                if i + 1 < self.rows.len() {
                    rows.resize(MAX, 0);
                }
                matrix.push(std::mem::take(&mut rows));
            }
        }
        if !rows.is_empty() {
            matrix.push(rows)
        }
        let matrix_hnid = if matrix.len() == 1 && matrix[0].len() <= 3580 {
            heap.alloc(&matrix[0])?
        } else if matrix.is_empty() {
            0
        } else {
            payload(&matrix)?
        };
        index.sort_by_key(|e| u32::from_le_bytes(e.0[..4].try_into().unwrap()));
        let row_index = heap.bth(&index, 4, 4)?;
        let mut info = vec![0; 22 + columns.len() * 8];
        info[0] = 0x7c;
        info[1] = columns.len() as u8;
        for (i, end) in ends.iter().enumerate() {
            p16(&mut info, 2 + i * 2, *end as u16);
        }
        p16(&mut info, 8, width as u16);
        p32(&mut info, 10, row_index);
        p32(&mut info, 14, matrix_hnid);
        for (i, (tag, size)) in columns.iter().enumerate() {
            p32(&mut info, 22 + i * 8, *tag);
            p16(&mut info, 26 + i * 8, offsets[tag] as u16);
            info[28 + i * 8] = *size as u8;
            info[29 + i * 8] = bits[tag] as u8;
        }
        heap.alloc(&info)
    }
}
