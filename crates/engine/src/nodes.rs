//! Fixed-width node inventory. Iteration owns one decoded node; I/O is fallible.
use crate::writer::spool::{Index, Spool};
use pstoff_format::ndb::Node;
use std::cell::RefCell;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(crate) struct Nodes {
    file: RefCell<Spool>,
    index: RefCell<Index>,
    len: usize,
}
impl Nodes {
    pub fn new() -> Result<Self> {
        Ok(Self {
            file: RefCell::new(Spool::new()?),
            index: RefCell::new(Index::new()?),
            len: 0,
        })
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn push(&mut self, n: Node) -> Result<()> {
        let len = self.len.checked_add(1).ok_or("Node count overflow")?;
        let mut b = [0; 24];
        b[..4].copy_from_slice(&n.nid.to_le_bytes());
        b[4..8].copy_from_slice(&n.nid_parent.to_le_bytes());
        b[8..16].copy_from_slice(&n.bid_data.to_le_bytes());
        b[16..].copy_from_slice(&n.bid_sub.to_le_bytes());
        if self.index.get_mut().get(n.nid)?.is_some() {
            return Err("Duplicate node ID".into());
        }
        let at = u64::try_from(self.len)?
            .checked_mul(24)
            .ok_or("Node offset overflow")?;
        self.file.get_mut().write(at, &b)?;
        self.index.get_mut().insert(n.nid, self.len as u64)?;
        self.len = len;
        Ok(())
    }
    pub fn get(&self, i: usize) -> Result<Option<Node>> {
        if i >= self.len {
            return Ok(None);
        }
        let at = u64::try_from(i)?
            .checked_mul(24)
            .ok_or("Node offset overflow")?;
        let mut b = [0; 24];
        self.file.borrow_mut().read(at, &mut b)?;
        Ok(Some(Node {
            nid: u32::from_le_bytes(b[..4].try_into()?),
            nid_parent: u32::from_le_bytes(b[4..8].try_into()?),
            bid_data: u64::from_le_bytes(b[8..16].try_into()?),
            bid_sub: u64::from_le_bytes(b[16..].try_into()?),
        }))
    }
    pub fn by_id(&self, id: u32) -> Result<Option<Node>> {
        match self.index.borrow_mut().get(id)? {
            Some(i) => self.get(usize::try_from(i)?),
            None => Ok(None),
        }
    }
    pub fn namespace(
        source: &mut pstoff_format::ndb::Pst,
        bid: u64,
        cancel: &crate::CancelToken,
    ) -> Result<Self> {
        let mut nodes = Self::new()?;
        source.visit_subnodes::<Box<dyn std::error::Error>>(bid, |nid, sub| {
            if cancel.is_cancelled() {
                return Err("Namespace staging cancelled".into());
            }
            nodes.push(Node {
                nid,
                nid_parent: 0,
                bid_data: sub.data,
                bid_sub: sub.sub,
            })
        })?;
        Ok(nodes)
    }
    pub fn iter(&self) -> NodeIter<'_> {
        NodeIter {
            nodes: self,
            at: 0,
            keep: Box::new(|_| true),
        }
    }
    pub fn retain(
        &mut self,
        mut keep: impl FnMut(&Node) -> Result<bool>,
        cancel: &crate::CancelToken,
    ) -> Result<()> {
        let mut next = Self::new()?;
        for n in self.iter() {
            if cancel.is_cancelled() {
                return Err("Node processing cancelled".into());
            }
            let n = n?;
            if keep(&n)? {
                next.push(n)?;
            }
        }
        if cancel.is_cancelled() {
            return Err("Node processing cancelled".into());
        }
        *self = next;
        Ok(())
    }
    pub fn sorted(&self, cancel: &crate::CancelToken) -> Result<Self> {
        let mut next = Self::new()?;
        self.index.borrow_mut().visit(cancel, |_, i| {
            next.push(self.get(usize::try_from(i)?)?.ok_or("Missing node")?)
        })?;
        if cancel.is_cancelled() {
            return Err("Node processing cancelled".into());
        }
        Ok(next)
    }
}
pub(crate) struct NodeIter<'a> {
    nodes: &'a Nodes,
    at: usize,
    keep: Box<dyn Fn(&Node) -> bool + 'a>,
}
impl<'a> NodeIter<'a> {
    pub fn filter(self, keep: impl Fn(&Node) -> bool + 'a) -> Self {
        let previous = self.keep;
        Self {
            keep: Box::new(move |n| previous(n) && keep(n)),
            ..self
        }
    }
    pub fn find(mut self, keep: impl Fn(&Node) -> bool) -> Result<Option<Node>> {
        for n in &mut self {
            let n = n?;
            if keep(&n) {
                return Ok(Some(n));
            }
        }
        Ok(None)
    }
    pub fn any(self, keep: impl Fn(&Node) -> bool) -> Result<bool> {
        Ok(self.find(keep)?.is_some())
    }
    pub fn max_key(mut self, key: impl Fn(Node) -> u32) -> Result<Option<u32>> {
        self.try_fold(None, |m: Option<u32>, n| {
            let k = key(n?);
            Ok(Some(m.map_or(k, |m| m.max(k))))
        })
    }
    pub fn count(mut self) -> Result<usize> {
        self.try_fold(0usize, |n, value| {
            value?;
            Ok(n + 1)
        })
    }
}
impl Iterator for NodeIter<'_> {
    type Item = Result<Node>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.at < self.nodes.len {
            let i = self.at;
            self.at += 1;
            match self.nodes.get(i) {
                Ok(Some(n)) if (self.keep)(&n) => return Some(Ok(n)),
                Ok(_) => (),
                Err(e) => {
                    self.at = self.nodes.len;
                    return Some(Err(e));
                }
            }
        }
        None
    }
}
impl<'a> IntoIterator for &'a Nodes {
    type Item = Result<Node>;
    type IntoIter = NodeIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub(crate) struct Namespace(Nodes);
impl Namespace {
    pub fn new(
        source: &mut pstoff_format::ndb::Pst,
        bid: u64,
        cancel: &crate::CancelToken,
    ) -> Result<Self> {
        Ok(Self(Nodes::namespace(source, bid, cancel)?))
    }
    pub fn get(&self, id: &u32) -> Result<Option<pstoff_format::ndb::Sub>> {
        Ok(self.0.by_id(*id)?.map(|n| pstoff_format::ndb::Sub {
            data: n.bid_data,
            sub: n.bid_sub,
        }))
    }
}

/// Disk stack for fan-out dependent traversal state, independent of namespace width.
pub(crate) struct Pending {
    file: Spool,
    count: u64,
}
impl Pending {
    pub fn new(bid: u64) -> Result<Self> {
        let mut s = Self {
            file: Spool::new()?,
            count: 0,
        };
        s.push(bid, 0)?;
        Ok(s)
    }
    pub fn push(&mut self, bid: u64, depth: u32) -> Result<()> {
        let mut b = [0; 12];
        b[..8].copy_from_slice(&bid.to_le_bytes());
        b[8..].copy_from_slice(&depth.to_le_bytes());
        self.file.write(
            self.count
                .checked_mul(12)
                .ok_or("Traversal offset overflow")?,
            &b,
        )?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or("Traversal count overflow")?;
        Ok(())
    }
    pub fn pop(&mut self) -> Result<Option<(u64, u32)>> {
        if self.count == 0 {
            return Ok(None);
        }
        self.count -= 1;
        let mut b = [0; 12];
        self.file.read(self.count * 12, &mut b)?;
        Ok(Some((
            u64::from_le_bytes(b[..8].try_into()?),
            u32::from_le_bytes(b[8..].try_into()?),
        )))
    }
}

/// Count-independent membership, including empty sets without a fallible constructor.
#[derive(Default)]
pub(crate) struct IdSet {
    index: RefCell<Option<Index>>,
    count: usize,
}
impl IdSet {
    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn contains(&self, id: &u32) -> std::io::Result<bool> {
        match self.index.borrow_mut().as_mut() {
            Some(i) => i.get(*id).map(|v| v.is_some()).map_err(storage_error),
            None => Ok(false),
        }
    }
    pub fn insert(&mut self, id: u32) -> std::io::Result<bool> {
        let slot = self.index.get_mut();
        if slot.is_none() {
            *slot = Some(Index::new().map_err(storage_error)?);
        }
        let added = slot
            .as_mut()
            .unwrap()
            .insert(id, 0)
            .map_err(storage_error)?
            .is_none();
        if added {
            self.count = self
                .count
                .checked_add(1)
                .ok_or_else(|| std::io::Error::other("Membership count overflow"))?;
        }
        Ok(added)
    }
    pub fn copy(&self, cancel: &crate::CancelToken) -> std::io::Result<Self> {
        let mut out = Self::default();
        if let Some(index) = self.index.borrow_mut().as_mut() {
            index
                .visit(cancel, |id, _| {
                    out.insert(id)?;
                    Ok(())
                })
                .map_err(storage_error)?;
        }
        if cancel.is_cancelled() {
            return Err(std::io::Error::other("Membership copying cancelled"));
        }
        Ok(out)
    }
    pub fn contains_group(&self, id: u32) -> std::io::Result<bool> {
        for kind in 0..32 {
            if self.contains(&(id & !31 | kind))? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
fn storage_error(_: Box<dyn std::error::Error>) -> std::io::Error {
    std::io::Error::other("Membership staging failed; check temporary volume free space")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_filter_is_transactional_on_cancellation_and_truncation_is_fallible() {
        let mut nodes = Nodes::new().unwrap();
        nodes
            .push(Node {
                nid: 4,
                nid_parent: 2,
                bid_data: 8,
                bid_sub: 12,
            })
            .unwrap();
        let cancel = crate::CancelToken::default();
        cancel.cancel();
        assert!(nodes.retain(|_| Ok(false), &cancel).is_err());
        assert_eq!(nodes.len(), 1);
        let late = crate::CancelToken::default();
        assert!(nodes
            .retain(
                |_| {
                    late.cancel();
                    Ok(false)
                },
                &late
            )
            .is_err());
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes.by_id(4).unwrap().unwrap().bid_sub, 12);
        assert!(nodes
            .push(Node {
                nid: 4,
                nid_parent: 0,
                bid_data: 99,
                bid_sub: 0
            })
            .is_err());
        assert_eq!(nodes.by_id(4).unwrap().unwrap().bid_data, 8);
        let path = nodes.file.borrow().path().to_owned();
        nodes.file.borrow_mut().len = 1;
        assert!(nodes.iter().next().unwrap().is_err());
        drop(nodes);
        assert!(!path.exists());
    }
}
