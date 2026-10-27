//! Logical streams borrow the original source handle; no NID is resolved here.
use super::{u16le, u32le, Pst};
#[cfg(test)]
mod tests;

struct Frame {
    bytes: Vec<u8>,
    next: usize,
    count: usize,
    total: u64,
    observed: u64,
    level: u8,
}

/// A strict logical stream cursor. At most two decoded index blocks and one
/// decoded data block are live. Consume through `None` to validate every cbTotal;
/// chunks delivered before an error must not be published as an accepted stream.
pub struct NodeChunks<'a> {
    source: &'a mut Pst,
    frames: Vec<Frame>,
    pending: Option<(u64, Option<u8>)>,
    failed: bool,
}

impl Pst {
    /// Open a bounded cursor on this handle's BID (including attachment-local
    /// streams resolved by the caller in their original subnode namespace).
    pub fn node_chunks(&mut self, bid: u64) -> Result<NodeChunks<'_>, String> {
        if !self.strict {
            return Err("Logical stream chunks require a strict source handle".into());
        }
        Ok(NodeChunks {
            source: self,
            frames: Vec::with_capacity(2),
            pending: (bid != 0).then_some((bid, None)),
            failed: false,
        })
    }
}

impl NodeChunks<'_> {
    fn read_next(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            if let Some((bid, expected)) = self.pending.take() {
                if bid == 0 || expected.is_some_and(|level| (bid & 2 != 0) != (level != 0)) {
                    return Err("Invalid logical stream child kind".into());
                }
                let bytes = self.source.block(bid)?;
                if bid & 2 == 0 {
                    for frame in &mut self.frames {
                        frame.observed = frame
                            .observed
                            .checked_add(bytes.len() as u64)
                            .ok_or("Logical stream length overflow")?;
                        if frame.observed > frame.total {
                            return Err("Logical stream exceeds cbTotal".into());
                        }
                    }
                    return Ok(Some(bytes));
                }
                if bytes.len() < 8
                    || bytes[0] != 1
                    || !matches!(bytes[1], 1 | 2)
                    || expected.is_some_and(|level| bytes[1] != level)
                    || self.frames.len() >= 2
                {
                    return Err("Invalid XBLOCK type, level or depth".into());
                }
                let count = u16le(&bytes, 2) as usize;
                if count == 0 || 8 + count * self.source.page.w() != bytes.len() {
                    return Err("Invalid XBLOCK entry count".into());
                }
                self.frames.push(Frame {
                    count,
                    next: 0,
                    total: u32le(&bytes, 4) as u64,
                    observed: 0,
                    level: bytes[1],
                    bytes,
                });
            }
            let Some(frame) = self.frames.last_mut() else {
                return Ok(None);
            };
            if frame.next == frame.count {
                if frame.observed != frame.total {
                    return Err("Logical stream length differs from cbTotal".into());
                }
                self.frames.pop();
                continue;
            }
            let child = self
                .source
                .page
                .wide(&frame.bytes, 8 + frame.next * self.source.page.w());
            frame.next += 1;
            self.pending = Some((child, Some(frame.level - 1)));
        }
    }
}

impl Iterator for NodeChunks<'_> {
    type Item = Result<Vec<u8>, String>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        match self.read_next() {
            Ok(Some(bytes)) => Some(Ok(bytes)),
            Ok(None) => None,
            Err(error) => {
                self.failed = true;
                Some(Err(error))
            }
        }
    }
}
