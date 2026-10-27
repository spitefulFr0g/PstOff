use super::{context::Cell, spool::Spool, Result};
use std::collections::BTreeMap;

pub(super) type Row = (u32, u32, BTreeMap<u32, Cell>);
pub(super) struct Rows {
    file: Spool,
    pub count: u32,
    pub cancel: crate::CancelToken,
}
impl Rows {
    pub fn new() -> Result<Self> {
        Ok(Self {
            file: Spool::new()?,
            count: 0,
            cancel: Default::default(),
        })
    }
    pub fn push(&mut self, (id, version, cells): &Row) -> Result<()> {
        let count = self.count.checked_add(1).ok_or("Too many table rows")?;
        self.file.append(&id.to_le_bytes())?;
        self.file.append(&version.to_le_bytes())?;
        self.file
            .append(&u32::try_from(cells.len())?.to_le_bytes())?;
        for (tag, cell) in cells {
            self.file.append(&tag.to_le_bytes())?;
            self.file.append(&(cell.bytes.len() as u64).to_le_bytes())?;
            self.file.append(&cell.bytes)?;
        }
        self.count = count;
        Ok(())
    }
    pub fn visit(&mut self, mut visit: impl FnMut(Row) -> Result<()>) -> Result<()> {
        let mut at = 0;
        for _ in 0..self.count {
            if self.cancel.is_cancelled() {
                break;
            }
            let mut header = [0; 12];
            self.file.read(at, &mut header)?;
            at += 12;
            let id = u32::from_le_bytes(header[..4].try_into()?);
            let version = u32::from_le_bytes(header[4..8].try_into()?);
            let count = u32::from_le_bytes(header[8..].try_into()?);
            let mut cells = BTreeMap::new();
            for _ in 0..count {
                self.file.read(at, &mut header)?;
                at += 12;
                let tag = u32::from_le_bytes(header[..4].try_into()?);
                let len = u64::from_le_bytes(header[4..].try_into()?);
                let mut bytes = vec![0; usize::try_from(len)?];
                self.file.read(at, &mut bytes)?;
                at = at.checked_add(len).ok_or("Row spool offset overflow")?;
                cells.insert(
                    tag,
                    Cell {
                        kind: tag as u16,
                        bytes,
                        subnode: None,
                    },
                );
            }
            visit((id, version, cells))?;
        }
        Ok(())
    }
    pub fn rewrite(&mut self, mut change: impl FnMut(&mut Row) -> Result<bool>) -> Result<()> {
        let mut next = Self::new()?;
        next.cancel = self.cancel.clone();
        self.visit(|mut row| {
            if change(&mut row)? {
                next.push(&row)?;
            }
            Ok(())
        })?;
        *self = next;
        Ok(())
    }
}

// A block spool preserves short source heap pages and row-block boundaries.
// The directory is itself a file, rather than an in-memory offset per page.
pub(super) struct Blocks {
    data: Spool,
    offsets: Spool,
    pub count: u32,
}
impl Blocks {
    pub fn new() -> Result<Self> {
        Ok(Self {
            data: Spool::new()?,
            offsets: Spool::new()?,
            count: 0,
        })
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<()> {
        let next = self.count.checked_add(1).ok_or("Too many table blocks")?;
        self.offsets.append(&self.data.len.to_le_bytes())?;
        self.data.append(&(bytes.len() as u64).to_le_bytes())?;
        self.data.append(bytes)?;
        self.count = next;
        Ok(())
    }
    pub fn get(&mut self, page: u32) -> Result<Vec<u8>> {
        if page >= self.count {
            return Err("Invalid table heap page".into());
        }
        let mut bytes = [0; 8];
        self.offsets.read(u64::from(page) * 8, &mut bytes)?;
        let at = u64::from_le_bytes(bytes);
        self.data.read(at, &mut bytes)?;
        let mut value = vec![0; usize::try_from(u64::from_le_bytes(bytes))?];
        self.data.read(at + 8, &mut value)?;
        Ok(value)
    }
    pub fn iter(&mut self) -> impl Iterator<Item = Result<Vec<u8>>> + '_ {
        (0..self.count).map(|i| self.get(i))
    }
    pub fn patch(&mut self, page: u32, offset: u64, bytes: &[u8]) -> Result<()> {
        if page >= self.count {
            return Err("Invalid table spool page".into());
        }
        let mut entry = [0; 8];
        self.offsets.read(u64::from(page) * 8, &mut entry)?;
        let at = u64::from_le_bytes(entry);
        self.data.read(at, &mut entry)?;
        if offset
            .checked_add(bytes.len() as u64)
            .is_none_or(|end| end > u64::from_le_bytes(entry))
        {
            return Err("Invalid table spool patch".into());
        }
        self.data.write(at + 8 + offset, bytes)
    }
}

pub(super) struct Heap {
    pub blocks: Blocks,
    pub user_root: u32,
    pub client_sig: u8,
    shift: u32,
}
impl Heap {
    pub fn new(mut blocks: Blocks, version: u16) -> Result<Self> {
        let first = blocks.get(0)?;
        if first.len() < 12 || first[2] != 0xec {
            return Err("Invalid table heap".into());
        }
        Ok(Self {
            user_root: u32::from_le_bytes(first[4..8].try_into()?),
            client_sig: first[3],
            shift: if version >= 36 { 19 } else { 16 },
            blocks,
        })
    }
    pub fn item(&mut self, hid: u32) -> Result<Vec<u8>> {
        if hid == 0 {
            return Ok(vec![]);
        }
        if hid & 31 != 0 || hid & ((1 << self.shift) - 1) == 0 {
            return Err("Invalid table HID".into());
        }
        let page = self.blocks.get(hid >> self.shift)?;
        let u16at = |at: usize| -> Result<usize> {
            Ok(u16::from_le_bytes(
                page.get(at..at + 2)
                    .ok_or("Truncated table heap")?
                    .try_into()?,
            ) as usize)
        };
        let map = u16at(0)?;
        let index = (hid as usize & ((1 << self.shift) - 1)) >> 5;
        if index > u16at(map)? {
            return Err("Invalid table allocation index".into());
        }
        let start = u16at(map + 4 + (index - 1) * 2)?;
        let end = u16at(map + 4 + index * 2)?;
        Ok(page
            .get(start..end)
            .ok_or("Invalid table allocation")?
            .to_vec())
    }
}
