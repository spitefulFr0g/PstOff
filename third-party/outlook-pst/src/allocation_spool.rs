//! PstOff adaptation: fixed-width private disk records instead of all AMap pages.
use super::*;
use std::{
    marker::PhantomData,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) struct AllocationSpool<'a, Pst: PstFile> {
    cancelled: &'a dyn Fn() -> bool,
    file: Option<File>,
    path: PathBuf,
    count: u64,
    marker: PhantomData<Pst>,
}
impl<'a, Pst> AllocationSpool<'a, Pst>
where
    Pst: PstFile,
    Pst::AllocationMapPage: AllocationMapPageReadWrite<Pst>,
    <Pst::ByteIndex as ByteIndex>::Index: TryFrom<u64>,
    Pst::PageId: From<<Pst::ByteIndex as ByteIndex>::Index>,
{
    pub fn new(cancelled: &'a dyn Fn() -> bool) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "pstoff-amap-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(0).custom_flags(0x04000000);
            }
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        cancelled,
                        file: Some(file),
                        path,
                        count: 0,
                        marker: PhantomData,
                    })
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
    }
    pub fn check_cancelled(&self) -> io::Result<()> {
        if (self.cancelled)() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "PST finalization cancelled",
            ));
        }
        Ok(())
    }
    pub fn push(&mut self, page: &AllocationMapPageInfo<Pst>) -> io::Result<()> {
        let index = self.count;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(PstError::IntegerConversion)?;
        self.set(
            usize::try_from(index).map_err(|_| PstError::IntegerConversion)?,
            page,
        )
    }
    pub fn set(&mut self, index: usize, page: &AllocationMapPageInfo<Pst>) -> io::Result<()> {
        let at = self.offset(index)?;
        let file = self.file.as_mut().unwrap();
        file.seek(SeekFrom::Start(at))?;
        file.write_all(&page.free_space.to_le_bytes())?;
        file.write_all(page.amap_page.map_bits())
    }
    fn offset(&self, index: usize) -> io::Result<u64> {
        self.check_cancelled()?;
        let index = u64::try_from(index).map_err(|_| PstError::IntegerConversion)?;
        if index >= self.count {
            return Err(PstError::AllocationMapPageNotFound(index as usize).into());
        }
        index
            .checked_mul(504)
            .ok_or_else(|| PstError::IntegerConversion.into())
    }
    pub fn get(&mut self, index: usize) -> io::Result<AllocationMapPageInfo<Pst>> {
        let at = self.offset(index)?;
        let file = self.file.as_mut().unwrap();
        file.seek(SeekFrom::Start(at))?;
        let mut free = [0; 8];
        let mut bits = [0; 496];
        file.read_exact(&mut free)?;
        file.read_exact(&mut bits)?;
        let at = (index as u64)
            .checked_mul(AMAP_DATA_SIZE)
            .and_then(|n| n.checked_add(AMAP_FIRST_OFFSET))
            .ok_or(PstError::IntegerConversion)?;
        let at = <<Pst::ByteIndex as ByteIndex>::Index as TryFrom<u64>>::try_from(at)
            .map_err(|_| PstError::IntegerConversion)?;
        let trailer = Pst::PageTrailer::new(PageType::AllocationMap, 0, Pst::PageId::from(at), 0);
        Ok(AllocationMapPageInfo {
            amap_page: <Pst::AllocationMapPage as AllocationMapPageReadWrite<Pst>>::new(
                bits, trailer,
            )?,
            free_space: u64::from_le_bytes(free),
        })
    }
}
impl<Pst: PstFile> Drop for AllocationSpool<'_, Pst> {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spool_is_removed_after_io_error_and_cancellation() {
        let cancelled = std::cell::Cell::new(false);
        let check = || cancelled.get();
        let mut spool = AllocationSpool::<UnicodePstFile>::new(&check).unwrap();
        let path = spool.path.clone();
        let trailer = UnicodePageTrailer::new(PageType::AllocationMap, 0, 0x4400.into(), 0);
        let page = AllocationMapPageInfo {
            amap_page:
                <<UnicodePstFile as PstFile>::AllocationMapPage as AllocationMapPageReadWrite<
                    UnicodePstFile,
                >>::new([0; 496], trailer)
                .unwrap(),
            free_space: AMAP_DATA_SIZE,
        };
        spool.push(&page).unwrap();
        spool.file.as_mut().unwrap().set_len(1).unwrap();
        assert_eq!(
            spool.get(0).err().unwrap().kind(),
            io::ErrorKind::UnexpectedEof
        );
        drop(spool);
        assert!(!path.exists());
        let mut spool = AllocationSpool::<UnicodePstFile>::new(&check).unwrap();
        let path = spool.path.clone();
        spool.push(&page).unwrap();
        cancelled.set(true);
        assert_eq!(
            spool.get(0).err().unwrap().kind(),
            io::ErrorKind::Interrupted
        );
        drop(spool);
        assert!(!path.exists());
    }
}
