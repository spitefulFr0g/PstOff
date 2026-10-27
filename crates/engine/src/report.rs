//! Count-independent record ownership. Debug never formats record contents.
use crate::writer::spool::Spool;
use crate::*;
use serde::{de::DeserializeOwned, ser::SerializeSeq};
use std::{
    io,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

type Result<T> = io::Result<T>;
fn storage_error() -> io::Error {
    io::Error::other("Report record storage failed; check destination/temp free space and retry with a new output base name")
}
pub struct Records<T> {
    spool: Option<Arc<Mutex<Spool>>>,
    count: usize,
    marker: PhantomData<T>,
}
impl<T> Default for Records<T> {
    fn default() -> Self {
        Self {
            spool: None,
            count: 0,
            marker: PhantomData,
        }
    }
}
impl<T> Clone for Records<T> {
    fn clone(&self) -> Self {
        Self {
            spool: self.spool.clone(),
            count: self.count,
            marker: PhantomData,
        }
    }
}
impl<T> std::fmt::Debug for Records<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Records")
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}
impl<T: Serialize + DeserializeOwned> Records<T> {
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn push(&mut self, value: T) -> Result<()> {
        // A clone is a read snapshot. Never mutate a shared stream.
        if self
            .spool
            .as_ref()
            .is_some_and(|s| Arc::strong_count(s) > 1)
        {
            let mut copy = Self::default();
            for value in self.iter() {
                copy.push(value?)?;
            }
            *self = copy;
        }
        if self.spool.is_none() {
            self.spool = Some(Arc::new(Mutex::new(
                Spool::new().map_err(|_| storage_error())?,
            )));
        }
        let mut spool = self
            .spool
            .as_ref()
            .unwrap()
            .lock()
            .map_err(|_| storage_error())?;
        let at = spool.append(&[0; 8]).map_err(|_| storage_error())?;
        {
            use io::Write;
            let mut writer = io::BufWriter::with_capacity(8192, &mut *spool);
            serde_json::to_writer(&mut writer, &value).map_err(|_| storage_error())?;
            writer.flush()?;
        }
        let length = spool.len.checked_sub(at + 8).ok_or_else(storage_error)?;
        spool
            .write(at, &length.to_le_bytes())
            .map_err(|_| storage_error())?;
        self.count = self.count.checked_add(1).ok_or_else(storage_error)?;
        Ok(())
    }
    pub fn iter(&self) -> RecordIter<'_, T> {
        RecordIter {
            records: self,
            at: 0,
            remaining: self.count,
        }
    }
    pub fn extend(&mut self, values: impl IntoIterator<Item = T>) -> Result<()> {
        for value in values {
            self.push(value)?;
        }
        Ok(())
    }
    pub fn append(&mut self, other: &Self) -> Result<()> {
        self.append_cancellable(other, &CancelToken::default())
    }
    pub(crate) fn append_cancellable(&mut self, other: &Self, cancel: &CancelToken) -> Result<()> {
        for value in other.iter() {
            if cancel.is_cancelled() {
                return Err(io::Error::other("Report processing cancelled"));
            }
            self.push(value?)?;
        }
        Ok(())
    }
    pub fn retain(&mut self, keep: impl FnMut(&T) -> bool) -> Result<()> {
        self.retain_cancellable(keep, &CancelToken::default())
    }
    pub(crate) fn retain_cancellable(
        &mut self,
        mut keep: impl FnMut(&T) -> bool,
        cancel: &CancelToken,
    ) -> Result<()> {
        let mut next = Self::default();
        for value in self.iter() {
            if cancel.is_cancelled() {
                return Err(io::Error::other("Report processing cancelled"));
            }
            let value = value?;
            if keep(&value) {
                next.push(value)?;
            }
        }
        *self = next;
        Ok(())
    }
    /// Explicitly opt into count-dependent memory; frontends must stream instead.
    pub fn collect_records(&self) -> Result<Vec<T>> {
        self.iter().collect()
    }
}
pub struct RecordIter<'a, T> {
    records: &'a Records<T>,
    at: u64,
    remaining: usize,
}
impl<T: DeserializeOwned> Iterator for RecordIter<'_, T> {
    type Item = Result<T>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let result = (|| {
            let mut spool = self
                .records
                .spool
                .as_ref()
                .ok_or_else(storage_error)?
                .lock()
                .map_err(|_| storage_error())?;
            let mut length = [0; 8];
            spool
                .read(self.at, &mut length)
                .map_err(|_| storage_error())?;
            self.at = self.at.checked_add(8).ok_or_else(storage_error)?;
            let end = self
                .at
                .checked_add(u64::from_le_bytes(length))
                .ok_or_else(storage_error)?;
            if end > spool.len {
                return Err(storage_error());
            }
            struct Range<'a> {
                spool: &'a mut Spool,
                at: u64,
                end: u64,
            }
            impl io::Read for Range<'_> {
                fn read(&mut self, b: &mut [u8]) -> Result<usize> {
                    let n = (self.end - self.at).min(b.len() as u64) as usize;
                    if n == 0 {
                        return Ok(0);
                    }
                    self.spool
                        .read(self.at, &mut b[..n])
                        .map_err(|_| storage_error())?;
                    self.at += n as u64;
                    Ok(n)
                }
            }
            let value = serde_json::from_reader(io::BufReader::with_capacity(
                8192,
                Range {
                    spool: &mut spool,
                    at: self.at,
                    end,
                },
            ))
            .map_err(|_| storage_error())?;
            self.at = end;
            Ok(value)
        })();
        if result.is_err() {
            self.remaining = 0;
        }
        Some(result)
    }
}
impl<T: Serialize + DeserializeOwned> Serialize for Records<T> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.count))?;
        for value in self.iter() {
            seq.serialize_element(&value.map_err(serde::ser::Error::custom)?)?;
        }
        seq.end()
    }
}
impl<'de, T: Serialize + DeserializeOwned> Deserialize<'de> for Records<T> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor<T>(PhantomData<T>);
        impl<'de, T: Serialize + DeserializeOwned> serde::de::Visitor<'de> for Visitor<T> {
            type Value = Records<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("report records")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut records = Records::default();
                while let Some(value) = seq.next_element()? {
                    records.push(value).map_err(serde::de::Error::custom)?;
                }
                Ok(records)
            }
        }
        deserializer.deserialize_seq(Visitor(PhantomData))
    }
}
pub type StreamedReport = ConversionReport<
    Records<OutputFile>,
    Records<FolderPart>,
    Records<Skipped>,
    Records<Difference>,
    Records<ItemRef>,
>;
impl StreamedReport {
    pub fn collect_report(&self) -> Result<ConversionReport> {
        fn evidence(e: &Evidence<Records<ItemRef>>) -> Result<Evidence> {
            Ok(match e {
                Evidence::NotChecked => Evidence::NotChecked,
                Evidence::Checked { count, references } => Evidence::Checked {
                    count: *count,
                    references: references.collect_records()?,
                },
                Evidence::Incomplete {
                    count,
                    references,
                    unchecked,
                } => Evidence::Incomplete {
                    count: *count,
                    references: references.collect_records()?,
                    unchecked: unchecked.collect_records()?,
                },
            })
        }
        Ok(ConversionReport {
            schema_version: self.schema_version,
            mailbox_completeness: self.mailbox_completeness.clone(),
            mailbox_completeness_statement: self.mailbox_completeness_statement.clone(),
            source: self.source.clone(),
            source_sha256: self.source_sha256.clone(),
            files: self.files.collect_records()?,
            folders: self.folders.collect_records()?,
            skipped: self.skipped.collect_records()?,
            differences: self.differences.collect_records()?,
            evidence: CompletenessEvidence {
                headers_only: evidence(&self.evidence.headers_only)?,
                unreadable: evidence(&self.evidence.unreadable)?,
                external_references: evidence(&self.evidence.external_references)?,
            },
        })
    }
}
impl ConversionReport {
    pub fn into_streamed(self) -> Result<StreamedReport> {
        fn records<T: Serialize + DeserializeOwned>(v: Vec<T>) -> Result<Records<T>> {
            let mut r = Records::default();
            r.extend(v)?;
            Ok(r)
        }
        fn evidence(e: Evidence) -> Result<Evidence<Records<ItemRef>>> {
            Ok(match e {
                Evidence::NotChecked => Evidence::NotChecked,
                Evidence::Checked { count, references } => Evidence::Checked {
                    count,
                    references: records(references)?,
                },
                Evidence::Incomplete {
                    count,
                    references,
                    unchecked,
                } => Evidence::Incomplete {
                    count,
                    references: records(references)?,
                    unchecked: records(unchecked)?,
                },
            })
        }
        Ok(StreamedReport {
            schema_version: self.schema_version,
            mailbox_completeness: self.mailbox_completeness,
            mailbox_completeness_statement: self.mailbox_completeness_statement,
            source: self.source,
            source_sha256: self.source_sha256,
            files: records(self.files)?,
            folders: records(self.folders)?,
            skipped: records(self.skipped)?,
            differences: records(self.differences)?,
            evidence: CompletenessEvidence {
                headers_only: evidence(self.evidence.headers_only)?,
                unreadable: evidence(self.evidence.unreadable)?,
                external_references: evidence(self.evidence.external_references)?,
            },
        })
    }
}

impl StreamedReport {
    pub fn summary_counts(&self) -> io::Result<(u64, u64)> {
        let mut index = crate::writer::spool::Index::new().map_err(|_| storage_error())?;
        let (mut items, mut folders) = (0u64, 0u64);
        for folder in self.folders.iter() {
            let folder = folder?;
            items = items.checked_add(folder.items).ok_or_else(storage_error)?;
            if index
                .insert(folder.folder.0, 0)
                .map_err(|_| storage_error())?
                .is_none()
            {
                folders += 1;
            }
        }
        Ok((items, folders))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_cleanup_and_debug_do_not_expose_details() {
        let mut records = Records::default();
        records
            .push(Difference {
                property: 0x3001001f,
                item: None,
                source: "synthetic-private-detail".into(),
                reason: "synthetic-private-detail".into(),
            })
            .unwrap();
        let path = records
            .spool
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .path()
            .to_owned();
        let snapshot = records.clone();
        records
            .push(Difference {
                property: 2,
                item: None,
                source: String::new(),
                reason: String::new(),
            })
            .unwrap();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(records.len(), 2);
        assert!(!format!("{snapshot:?}").contains("synthetic-private-detail"));
        assert!(!format!("{:?}", snapshot.iter().next().unwrap().unwrap())
            .contains("synthetic-private-detail"));
        drop(snapshot);
        assert!(!path.exists());
        let path = records
            .spool
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .path()
            .to_owned();
        drop(records);
        assert!(!path.exists());
    }
    #[test]
    fn truncated_records_fail_without_payload_diagnostics() {
        let mut records = Records::default();
        records
            .push(ItemRef {
                folder: FolderRef(2),
                node: 4,
            })
            .unwrap();
        records
            .spool
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .write(0, &u64::MAX.to_le_bytes())
            .unwrap();
        assert!(records.iter().next().unwrap().is_err());
        assert!(serde_json::to_writer(io::sink(), &records).is_err());
    }
    #[test]
    fn cancelling_record_filter_preserves_input_and_discards_rewrite() {
        let mut records = Records::default();
        records
            .extend((0..1000).map(|node| ItemRef {
                folder: FolderRef(2),
                node,
            }))
            .unwrap();
        let token = CancelToken::default();
        assert!(records
            .retain_cancellable(
                |_| {
                    token.cancel();
                    true
                },
                &token
            )
            .is_err());
        assert_eq!(records.len(), 1000);
        assert_eq!(records.iter().count(), 1000);
        assert_eq!(records.iter().last().unwrap().unwrap().node, 999);
        let mut copy = Records::default();
        assert!(copy.append_cancellable(&records, &token).is_err());
        assert!(copy.is_empty());
    }
}
