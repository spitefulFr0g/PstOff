//! Private, seekable staging files. No mail-derived names and no persistent handles.
use super::Result;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) struct Spool {
    file: Option<File>,
    path: PathBuf,
    pub len: u64,
}
impl Spool {
    pub fn new() -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "pstoff-table-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(0).custom_flags(0x04000000); // FILE_FLAG_DELETE_ON_CLOSE
            }
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        file: Some(file),
                        path,
                        len: 0,
                    })
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
    pub fn write(&mut self, at: u64, bytes: &[u8]) -> Result<()> {
        let end = at
            .checked_add(bytes.len() as u64)
            .ok_or("Table spool offset overflow")?;
        let file = self.file.as_mut().unwrap();
        file.seek(SeekFrom::Start(at))?;
        file.write_all(bytes)?;
        self.len = self.len.max(end);
        Ok(())
    }
    pub fn append(&mut self, bytes: &[u8]) -> Result<u64> {
        let at = self.len;
        self.write(at, bytes)?;
        Ok(at)
    }
    pub fn read(&mut self, at: u64, bytes: &mut [u8]) -> Result<()> {
        if at
            .checked_add(bytes.len() as u64)
            .is_none_or(|end| end > self.len)
        {
            return Err("Truncated table spool".into());
        }
        let file = self.file.as_mut().unwrap();
        file.seek(SeekFrom::Start(at))?;
        file.read_exact(bytes)?;
        Ok(())
    }
}
impl Drop for Spool {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

// Four byte-radix levels; only one eight-byte slot is read at a time. Values
// are encoded plus one, so zero-filled slots mean absent. Nodes are allocated
// densely on disk, never with an attacker-controlled sparse-file offset.
pub(super) struct Index(Spool);
impl Index {
    pub fn visit(
        &mut self,
        cancel: &crate::CancelToken,
        mut visit: impl FnMut(u32, u64) -> Result<()>,
    ) -> Result<()> {
        fn walk(
            file: &mut Spool,
            node: u64,
            level: u32,
            key: u32,
            cancel: &crate::CancelToken,
            visit: &mut dyn FnMut(u32, u64) -> Result<()>,
        ) -> Result<()> {
            let mut page = [0; 2048];
            file.read(node, &mut page)?;
            for (i, slot) in page.as_chunks::<8>().0.iter().enumerate() {
                if cancel.is_cancelled() {
                    return Ok(());
                }
                let value = u64::from_le_bytes(*slot);
                if value == 0 {
                    continue;
                }
                let key = (key << 8) | i as u32;
                if level == 3 {
                    visit(key, value - 1)?;
                } else {
                    walk(file, value, level + 1, key, cancel, visit)?;
                }
            }
            Ok(())
        }
        walk(&mut self.0, 0, 0, 0, cancel, &mut visit)
    }
    pub fn new() -> Result<Self> {
        let mut file = Spool::new()?;
        file.append(&[0; 2048])?;
        Ok(Self(file))
    }
    pub fn get(&mut self, key: u32) -> Result<Option<u64>> {
        self.access(key, None)
    }
    pub fn insert(&mut self, key: u32, value: u64) -> Result<Option<u64>> {
        self.access(key, Some(value))
    }
    fn access(&mut self, key: u32, value: Option<u64>) -> Result<Option<u64>> {
        let mut node = 0;
        for (level, byte) in key.to_be_bytes().iter().enumerate() {
            let at = node + u64::from(*byte) * 8;
            let mut bytes = [0; 8];
            self.0.read(at, &mut bytes)?;
            let old = u64::from_le_bytes(bytes);
            if level == 3 {
                if let Some(value) = value {
                    self.0.write(
                        at,
                        &value
                            .checked_add(1)
                            .ok_or("Index value overflow")?
                            .to_le_bytes(),
                    )?;
                }
                return Ok(old.checked_sub(1));
            }
            if old == 0 {
                if value.is_none() {
                    return Ok(None);
                }
                node = self.0.append(&[0; 2048])?;
                self.0.write(at, &node.to_le_bytes())?;
            } else {
                node = old;
            }
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staging_files_are_removed_on_success_and_error() {
        let mut spool = Spool::new().unwrap();
        let path = spool.path.clone();
        spool.append(b"generated").unwrap();
        assert!(path.exists());
        assert!(spool.read(0, &mut [0; 10]).is_err());
        drop(spool);
        assert!(!path.exists());
        let mut index = Index::new().unwrap();
        let path = index.0.path.clone();
        for key in [0, u32::MAX, 0x12345678, 7] {
            assert_eq!(index.insert(key, u64::from(key)).unwrap(), None);
            assert_eq!(index.get(key).unwrap(), Some(u64::from(key)));
        }
        assert_eq!(index.get(8).unwrap(), None);
        assert_eq!(index.insert(7, 99).unwrap(), Some(7));
        let cancel = crate::CancelToken::default();
        let mut keys = vec![];
        index
            .visit(&cancel, |key, _| {
                keys.push(key);
                Ok(())
            })
            .unwrap();
        assert_eq!(keys, [0, 7, 0x12345678, u32::MAX]);
        cancel.cancel();
        index
            .visit(&cancel, |_, _| panic!("cancelled index visited a record"))
            .unwrap();
        drop(index);
        assert!(!path.exists());
    }
}
