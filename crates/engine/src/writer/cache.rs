//! Disk-backed policy planning. Only fixed operands survive an occurrence.
use super::{
    context::{self, Properties},
    spool::{Index, Spool},
    table,
    table_storage::{Blocks, Heap},
    Result,
};
use crate::{Difference, FolderRef, ItemRef};
use pstoff_format::ndb::{Node, Pst};

const IDS: [u16; 3] = [0x3008, 0x65e2, 0x65e3];
const CHUNK: usize = 8192;
#[derive(Clone, Copy, Default)]
pub(super) struct Value {
    pub kind: u16,
    pub at: u64,
    pub len: u64,
}
impl Value {
    pub fn encode(self) -> [u8; 18] {
        let mut b = [0; 18];
        b[..2].copy_from_slice(&self.kind.to_le_bytes());
        b[2..10].copy_from_slice(&self.at.to_le_bytes());
        b[10..].copy_from_slice(&self.len.to_le_bytes());
        b
    }
    fn decode(b: &[u8]) -> Result<Self> {
        Ok(Self {
            kind: u16::from_le_bytes(b.get(..2).ok_or("Short policy descriptor")?.try_into()?),
            at: u64::from_le_bytes(b.get(2..10).ok_or("Short policy descriptor")?.try_into()?),
            len: u64::from_le_bytes(b.get(10..18).ok_or("Short policy descriptor")?.try_into()?),
        })
    }
}
pub(super) struct Blob(Spool);
impl Blob {
    fn new() -> Result<Self> {
        Ok(Self(Spool::new()?))
    }
    pub fn len(&self) -> u64 {
        self.0.len
    }
    pub fn append(&mut self, b: &[u8]) -> Result<()> {
        self.0.append(b)?;
        Ok(())
    }
    pub fn put(&mut self, kind: u16, b: &[u8]) -> Result<Value> {
        Ok(Value {
            kind,
            at: self.0.append(b)?,
            len: b.len() as u64,
        })
    }
    fn read(&mut self, v: Value, offset: u64, b: &mut [u8]) -> Result<()> {
        if offset
            .checked_add(b.len() as u64)
            .is_none_or(|end| end > v.len)
        {
            return Err("Policy range overflow".into());
        }
        self.0
            .read(v.at.checked_add(offset).ok_or("Policy offset overflow")?, b)
    }
    fn equal(&mut self, a: Value, b: Value, cancel: &crate::CancelToken) -> Result<bool> {
        if a.kind != b.kind || a.len != b.len {
            return Ok(false);
        }
        let (mut left, mut right) = ([0; CHUNK], [0; CHUNK]);
        let mut at = 0;
        while at < a.len {
            if cancel.is_cancelled() {
                return Err("Cancelled policy comparison".into());
            }
            let n = (a.len - at).min(CHUNK as u64) as usize;
            self.read(a, at, &mut left[..n])?;
            self.read(b, at, &mut right[..n])?;
            if left[..n] != right[..n] {
                return Ok(false);
            }
            at += n as u64;
        }
        Ok(true)
    }
    fn valid_pair(&mut self, key: Value, pcl: Value, cancel: &crate::CancelToken) -> Result<bool> {
        if key.kind != 0x102 || pcl.kind != 0x102 || !(17..=24).contains(&key.len) || pcl.len == 0 {
            return Ok(false);
        }
        let mut k = [0; 24];
        self.read(key, 0, &mut k[..key.len as usize])?;
        let mut previous: Option<[u8; 16]> = None;
        let mut covered = false;
        // A single fixed read buffer; SizedXids may cross any buffer boundary.
        let mut buffer = [0; CHUNK];
        let mut cursor = 0;
        let mut available = 0;
        let mut offset = 0;
        let mut next = |blob: &mut Self| -> Result<Option<u8>> {
            if cursor == available {
                if cancel.is_cancelled() {
                    return Err("Cancelled PCL validation".into());
                }
                if offset == pcl.len {
                    return Ok(None);
                }
                available = (pcl.len - offset).min(CHUNK as u64) as usize;
                blob.read(pcl, offset, &mut buffer[..available])?;
                offset += available as u64;
                cursor = 0;
            }
            let byte = buffer[cursor];
            cursor += 1;
            Ok(Some(byte))
        };
        while let Some(size) = next(self)? {
            if !(17..=24).contains(&size) {
                return Ok(false);
            }
            let mut xid = [0; 24];
            for b in &mut xid[..size as usize] {
                let Some(v) = next(self)? else {
                    return Ok(false);
                };
                *b = v;
            }
            let ns: [u8; 16] = xid[..16].try_into()?;
            if previous.is_some_and(|p| p >= ns) {
                return Ok(false);
            }
            if ns == k[..16] {
                if u64::from(size) != key.len {
                    return Ok(false);
                }
                covered = xid[16..size as usize] >= k[16..key.len as usize];
            }
            previous = Some(ns);
        }
        Ok(covered)
    }
}

// Fixed 80-byte record: next+1, table, schema flags, three 18-byte values,
// and presence bits. Index keys are global Item NIDs, never attachment-local IDs.
const RECORD: usize = 80;
#[derive(Default, Clone, Copy)]
struct Observation {
    table: u32,
    time: bool,
    pair: bool,
    values: [Option<Value>; 3],
    // Zero means absent column; otherwise retain its exact column type even
    // when the occurrence has no cell. Presence bits are encoded separately.
    schema: [u16; 3],
}
struct Observations {
    index: Index,
    records: Spool,
}
impl Observations {
    fn new() -> Result<Self> {
        Ok(Self {
            index: Index::new()?,
            records: Spool::new()?,
        })
    }
    fn push(&mut self, item: u32, o: Observation) -> Result<()> {
        let mut b = [0; RECORD];
        let next = self
            .index
            .get(item)?
            .map(|at| at.checked_add(1).ok_or("Observation offset overflow"))
            .transpose()?
            .unwrap_or(0);
        b[..8].copy_from_slice(&next.to_le_bytes());
        b[8..12].copy_from_slice(&o.table.to_le_bytes());
        b[12] = u8::from(o.time) | u8::from(o.pair) << 1;
        for (i, v) in o.values.iter().enumerate() {
            if let Some(v) = v {
                b[13] |= 1 << i;
                b[16 + i * 18..34 + i * 18].copy_from_slice(&v.encode());
            }
        }
        for (i, kind) in o.schema.into_iter().enumerate() {
            b[70 + i * 2..72 + i * 2].copy_from_slice(&kind.to_le_bytes());
        }
        let at = self.records.append(&b)?;
        self.index.insert(item, at)?;
        Ok(())
    }
    fn get(&mut self, at: u64) -> Result<(Option<u64>, Observation)> {
        let mut b = [0; RECORD];
        self.records.read(at, &mut b)?;
        let mut o = Observation {
            table: u32::from_le_bytes(b[8..12].try_into()?),
            time: b[12] & 1 != 0,
            pair: b[12] & 2 != 0,
            ..Default::default()
        };
        for i in 0..3 {
            if b[13] & (1 << i) != 0 {
                o.values[i] = Some(Value::decode(&b[16 + i * 18..34 + i * 18])?);
            }
        }
        for i in 0..3 {
            o.schema[i] = u16::from_le_bytes(b[70 + i * 2..72 + i * 2].try_into()?);
        }
        Ok((u64::from_le_bytes(b[..8].try_into()?).checked_sub(1), o))
    }
}

pub(super) struct Plan {
    decisions: Observations,
    blob: Blob,
}
impl Plan {
    pub fn new() -> Result<Self> {
        Ok(Self {
            decisions: Observations::new()?,
            blob: Blob::new()?,
        })
    }
    // Materialization belongs to the existing writer, one current Item/row only.
    pub fn apply(&mut self, item: u32, cells: &mut Properties) -> Result<()> {
        if let Some(at) = self.decisions.index.get(item)? {
            let (_, o) = self.decisions.get(at)?;
            for (id, value) in IDS.into_iter().zip(o.values) {
                if let Some(v) = value {
                    let mut bytes = vec![0; usize::try_from(v.len)?];
                    self.blob.read(v, 0, &mut bytes)?;
                    cells.insert(
                        id,
                        context::Cell {
                            kind: v.kind,
                            bytes,
                            subnode: None,
                        },
                    );
                }
            }
        }
        Ok(())
    }
}

// Source namespace is resolved while the immutable source is held. Spool ranges
// subsequently identify bytes, so equal local NIDs in different PCs cannot alias.
fn own_policy(
    source: &mut Pst,
    node: &Node,
    blob: &mut Blob,
    cancel: &crate::CancelToken,
) -> Result<[Option<Value>; 3]> {
    let mut blocks = Blocks::new()?;
    for chunk in source.node_chunks(node.bid_data)? {
        if cancel.is_cancelled() {
            return Err("Cancelled Item policy read".into());
        }
        blocks.push(&chunk?)?;
    }
    let mut heap = Heap::new(blocks, source.ver)?;
    let header = heap.item(heap.user_root)?;
    if header.len() != 8 || header[..3] != [0xb5, 2, 6] {
        return Err("Invalid property BTH".into());
    }
    let subs = crate::nodes::Namespace::new(source, node.bid_sub, cancel)?;
    let mut values = [None; 3];
    struct Walk<'a> {
        source: &'a mut Pst,
        blob: &'a mut Blob,
        cancel: &'a crate::CancelToken,
    }
    fn walk(
        w: &mut Walk<'_>,
        heap: &mut Heap,
        hid: u32,
        level: u8,
        values: &mut [Option<Value>; 3],
        subs: &crate::nodes::Namespace,
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
            if w.cancel.is_cancelled() {
                return Err("Cancelled Item policy read".into());
            }
            if level != 0 {
                walk(
                    w,
                    heap,
                    u32::from_le_bytes(r[2..6].try_into()?),
                    level - 1,
                    values,
                    subs,
                )?;
                continue;
            }
            let id = u16::from_le_bytes(r[..2].try_into()?);
            let Some(i) = IDS.iter().position(|v| *v == id) else {
                continue;
            };
            if values[i].is_some() {
                return Err("Duplicate policy property".into());
            }
            let kind = u16::from_le_bytes(r[2..4].try_into()?);
            let hnid = u32::from_le_bytes(r[4..8].try_into()?);
            let v = if matches!(kind, 2 | 3 | 4 | 0xa | 0xb) {
                w.blob.put(kind, &r[4..8])?
            } else if hnid & 31 == 0 {
                w.blob.put(kind, &heap.item(hnid)?)?
            } else {
                let at = w.blob.len();
                let bid = subs.get(&hnid)?.ok_or("Missing policy subnode")?.data;
                for chunk in w.source.node_chunks(bid)? {
                    if w.cancel.is_cancelled() {
                        return Err("Cancelled Item policy read".into());
                    }
                    w.blob.append(&chunk?)?;
                }
                Value {
                    kind,
                    at,
                    len: w.blob.len() - at,
                }
            };
            values[i] = Some(v);
        }
        Ok(())
    }
    walk(
        &mut Walk {
            source,
            blob,
            cancel,
        },
        &mut heap,
        u32::from_le_bytes(header[4..].try_into()?),
        header[3],
        &mut values,
        &subs,
    )?;
    Ok(values)
}

pub(super) fn prepare(
    source: &mut Pst,
    nodes: &crate::nodes::Nodes,
    differences: &mut crate::Records<Difference>,
    cancel: &crate::CancelToken,
    omitted: &crate::nodes::IdSet,
) -> Result<Option<Plan>> {
    let result = prepare_inner(source, nodes, differences, cancel, omitted);
    if cancel.is_cancelled() {
        return Ok(None);
    }
    result.map(Some)
}
fn prepare_inner(
    source: &mut Pst,
    nodes: &crate::nodes::Nodes,
    differences: &mut crate::Records<Difference>,
    cancel: &crate::CancelToken,
    omitted: &crate::nodes::IdSet,
) -> Result<Plan> {
    let mut plan = Plan::new()?;
    let mut blob = Blob::new()?;
    let mut observations = Observations::new()?;
    for node in nodes
        .iter()
        .filter(|n| n.bid_data != 0 && matches!(n.nid & 31, 0xe..=0x10))
    {
        let node = node?;
        if cancel.is_cancelled() {
            return Err("Cancelled cache planning".into());
        }
        let mut table = table::policy_read(source, node.bid_data, node.bid_sub, cancel, &mut blob)?;
        table.rows.visit(|(nid, _, cells)| {
            if omitted.contains(&nid)? {
                return Ok(());
            }
            let item = nodes
                .by_id(nid)?
                .filter(|n| matches!(n.nid & 31, 4 | 8))
                .ok_or("Cache row has no indexed Item")?;
            if node.nid & 31 != 0x10 && node.nid & !31 != item.nid_parent & !31 {
                return Err("Cache row belongs to another folder".into());
            }
            let mut o = Observation {
                table: node.nid,
                time: table.time_cache,
                pair: table.pair_cache,
                ..Default::default()
            };
            for (i, id) in IDS.into_iter().enumerate() {
                if let Some((tag, _)) = table
                    .columns
                    .iter()
                    .find(|(tag, _)| *tag >> 16 == u32::from(id))
                {
                    o.schema[i] = *tag as u16;
                }
                if let Some((_, cell)) = cells.iter().find(|(tag, _)| **tag >> 16 == u32::from(id))
                {
                    o.values[i] = Some(Value::decode(&cell.bytes)?);
                }
            }
            observations.push(nid, o)
        })?;
    }
    for node in nodes.iter().filter(|n| matches!(n.nid & 31, 4 | 8)) {
        let node = node?;
        if cancel.is_cancelled() {
            return Err("Cancelled cache planning".into());
        }
        let own = own_policy(source, &node, &mut blob, cancel)?;
        let mut first_time = None;
        let mut first_pair: Option<[Value; 2]> = None;
        let mut count = [0u64; 2];
        let mut tables = [0u32; 2];
        let mut at = observations.index.get(node.nid)?;
        while let Some(offset) = at {
            if cancel.is_cancelled() {
                return Err("Cancelled cache planning".into());
            }
            let (next, o) = observations.get(offset)?;
            at = next;
            let diagnostic = |problem: &str, id: u16| {
                format!("Q15 {problem} for Folder 0x{:x}, Item 0x{:x}, property 0x{id:04x}0102 in contents table 0x{:x}; no cache value chosen. Review this Item's cache occurrences and property context before retrying.", node.nid_parent, node.nid, o.table)
            };
            if o.time && own[0].is_none() {
                let value = o.values[0].ok_or_else(|| {
                    format!(
                        "Q12 invalid or missing cache for Item 0x{:x}, table 0x{:x}",
                        node.nid, o.table
                    )
                })?;
                let mut bytes = [0; 8];
                if value.kind != 0x40 || value.len != 8 {
                    return Err(format!(
                        "Q12 invalid cache for Item 0x{:x}, table 0x{:x}",
                        node.nid, o.table
                    )
                    .into());
                }
                blob.read(value, 0, &mut bytes)?;
                let time = u64::from_le_bytes(bytes);
                if time == 0 || time > i64::MAX as u64 {
                    return Err(format!("Q12 invalid cache for Item 0x{:x}", node.nid).into());
                }
                if let Some(first) = first_time {
                    if !blob.equal(first, value, cancel)? {
                        return Err(format!(
                            "Q12 conflict for Item 0x{:x}, table 0x{:x}",
                            node.nid, o.table
                        )
                        .into());
                    }
                } else {
                    first_time = Some(value);
                }
                count[0] += 1;
                tables[0] = o.table;
            }
            if o.pair {
                let key = o.values[1].ok_or_else(|| diagnostic("missing ChangeKey", 0x65e2))?;
                let pcl = o.values[2].ok_or_else(|| diagnostic("missing PCL", 0x65e3))?;
                if first_pair.is_none() {
                    if !blob.valid_pair(key, pcl, cancel)? {
                        return Err(diagnostic("invalid XID/PCL pair", 0x65e2).into());
                    }
                    first_pair = Some([key, pcl]);
                }
                for (i, value) in [key, pcl].into_iter().enumerate() {
                    if !blob.equal(first_pair.unwrap()[i], value, cancel)?
                        || if let Some(v) = own[i + 1] {
                            !blob.equal(v, value, cancel)?
                        } else {
                            false
                        }
                    {
                        return Err(diagnostic(
                            "conflict with another occurrence or Item property",
                            IDS[i + 1],
                        )
                        .into());
                    }
                }
                count[1] += 1;
                tables[1] = o.table;
            }
        }
        let mut recovery = [
            first_time,
            first_pair.map(|p| p[0]).filter(|_| own[1].is_none()),
            first_pair.map(|p| p[1]).filter(|_| own[2].is_none()),
        ];
        if recovery.iter().any(Option::is_some) {
            for value in recovery.iter_mut().flatten() {
                let start = plan.blob.len();
                let mut offset = 0;
                let mut buffer = [0; CHUNK];
                while offset < value.len {
                    if cancel.is_cancelled() {
                        return Err("Cancelled recovery spool copy".into());
                    }
                    let count = (value.len - offset).min(CHUNK as u64) as usize;
                    blob.read(*value, offset, &mut buffer[..count])?;
                    plan.blob.append(&buffer[..count])?;
                    offset += count as u64;
                }
                value.at = start;
            }
            plan.decisions.push(
                node.nid,
                Observation {
                    values: recovery,
                    ..Default::default()
                },
            )?;
            for (i, v) in recovery.into_iter().enumerate() {
                if v.is_some() {
                    let group = usize::from(i != 0);
                    differences.push(Difference {
                        property: u32::from(IDS[i]) << 16 | if i == 0 { 0x40 } else { 0x102 },
                        item: Some(ItemRef {
                            folder: FolderRef(node.nid_parent),
                            node: node.nid,
                        }),
                        source: format!(
                            "Same Item cache: {} agreeing occurrences; evidence table 0x{:x}",
                            count[group], tables[group]
                        ),
                        reason: if i == 0 {
                            "Q12: validated, agreeing source FILETIME; no server-history claim."
                        } else {
                            "Q15: validated agreeing XID/PCL pair; no server-lineage claim."
                        }
                        .into(),
                    })?;
                }
            }
        }
    }
    Ok(plan)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn growing_item_and_occurrence_counts_keep_fixed_planner_operands() {
        // Increasing disk population with exactly the same in-memory owners.
        for items in [16, 128, 1024] {
            let (_, peak) = super::super::tests::allocations::measure(|| {
                let mut observations = Observations::new().unwrap();
                let mut blob = Blob::new().unwrap();
                let cancel = crate::CancelToken::default();
                let time = blob.put(0x40, &42u64.to_le_bytes()).unwrap();
                for item in 0..items {
                    for table in 0..32 {
                        observations
                            .push(
                                item,
                                Observation {
                                    table,
                                    time: true,
                                    schema: [0x40, 0x102, 0],
                                    values: [Some(time), None, None],
                                    ..Default::default()
                                },
                            )
                            .unwrap();
                    }
                }
                assert_eq!(
                    observations.records.len,
                    u64::from(items) * 32 * RECORD as u64
                );
                for item in 0..items {
                    let mut at = observations.index.get(item).unwrap();
                    let mut count = 0;
                    while let Some(offset) = at {
                        let (next, o) = observations.get(offset).unwrap();
                        assert!(blob.equal(time, o.values[0].unwrap(), &cancel).unwrap());
                        assert_eq!(o.table, 31 - count);
                        assert_eq!(o.schema, [0x40, 0x102, 0]);
                        assert!(o.values[1].is_none());
                        count += 1;
                        at = next;
                    }
                    assert_eq!(count, 32);
                }
                // The record chain has no per-Item or per-occurrence Vec/Map.
                assert!(std::mem::size_of::<Observation>() < 128);
                assert_eq!(blob.len(), 8);
            });
            assert!(
                peak < 64 * 1024,
                "{items} Items, 32 occurrences: planner live allocation {peak}"
            );
        }
    }
    fn pcl(blob: &mut Blob, entries: u64) -> Value {
        let at = blob.len();
        for i in 0..entries {
            let mut xid = [0; 25];
            xid[0] = 24;
            xid[1..9].copy_from_slice(&i.to_be_bytes());
            xid[24] = 1;
            blob.append(&xid).unwrap();
        }
        Value {
            kind: 0x102,
            at,
            len: blob.len() - at,
        }
    }
    #[test]
    fn complete_multi_megabyte_pcls_validate_and_compare_past_last_chunk() {
        let mut blob = Blob::new().unwrap();
        let cancel = crate::CancelToken::default();
        let mut key = [0; 24];
        key[23] = 1;
        let key = blob.put(0x102, &key).unwrap();
        let left = pcl(&mut blob, 90_000);
        let right = pcl(&mut blob, 90_000);
        assert!(left.len > 2 * 1024 * 1024);
        let (_, peak) = super::super::tests::allocations::measure(|| {
            assert!(blob.valid_pair(key, left, &cancel).unwrap());
            assert!(blob.equal(left, right, &cancel).unwrap());
        });
        assert!(peak < 4096, "PCL parser/comparison heap allocation {peak}");
        // Conflict in the final byte must not disappear through truncation.
        blob.0.write(right.at + right.len - 1, &[2]).unwrap();
        assert!(!blob.equal(left, right, &cancel).unwrap());
        assert!(blob.valid_pair(key, right, &cancel).unwrap());
        // A final, incomplete SizedXid is rejected even beyond two MiB.
        assert!(!blob
            .valid_pair(
                key,
                Value {
                    len: left.len - 1,
                    ..left
                },
                &cancel
            )
            .unwrap());
        // Cross-buffer SizedXid namespace ordering error.
        blob.0.write(left.at + 25 * 327 + 1, &[0; 16]).unwrap();
        assert!(!blob.valid_pair(key, left, &cancel).unwrap());
        cancel.cancel();
        assert!(blob.equal(left, right, &cancel).is_err());
        assert!(blob.valid_pair(key, left, &cancel).is_err());
    }
    #[test]
    fn recovered_values_are_keyed_by_item_and_exact_kind() {
        let mut plan = Plan::new().unwrap();
        let value = plan.blob.put(0x102, b"synthetic policy").unwrap();
        plan.decisions
            .push(
                0x104,
                Observation {
                    values: [None, Some(value), None],
                    ..Default::default()
                },
            )
            .unwrap();
        let mut own = Properties::new();
        plan.apply(0x124, &mut own).unwrap();
        assert!(own.is_empty());
        plan.apply(0x104, &mut own).unwrap();
        assert_eq!(own[&0x65e2].kind, 0x102);
        assert_eq!(own[&0x65e2].bytes, b"synthetic policy");
        assert!(own[&0x65e2].subnode.is_none());
    }
}

#[cfg(test)]
mod namespace_test {
    use super::super::tests::{support, synthetic};
    use super::*;
    #[test]
    fn item_policy_resolves_reused_local_nids_in_their_original_namespace() {
        let fixture = support::Fixture::invalid();
        let items = (1..=2)
            .map(|n| {
                let mut item = synthetic::Item::new("IPM.Note", 1);
                item.properties
                    .push(synthetic::Property::binary(0x65e3, vec![n; 8000]));
                item
            })
            .collect();
        let bytes = synthetic::Store {
            root: synthetic::Folder {
                name: "Generated".into(),
                items,
                ..Default::default()
            },
        }
        .build();
        std::fs::write(&fixture.source, &bytes).unwrap();
        let mut source = Pst::from_file(std::fs::File::open(&fixture.source).unwrap()).unwrap();
        let nodes: Vec<_> = source
            .nodes()
            .into_iter()
            .filter(|n| n.nid_type() == 4)
            .collect();
        let first_subs = source.subnodes(nodes[0].bid_sub).unwrap();
        let second_subs = source.subnodes(nodes[1].bid_sub).unwrap();
        assert!(first_subs.keys().any(|nid| second_subs.contains_key(nid)));
        let mut blob = Blob::new().unwrap();
        let cancel = crate::CancelToken::default();
        let a = own_policy(&mut source, &nodes[0], &mut blob, &cancel).unwrap()[2].unwrap();
        let b = own_policy(&mut source, &nodes[1], &mut blob, &cancel).unwrap()[2].unwrap();
        assert_eq!(a.len, 8000);
        assert_eq!(b.len, 8000);
        assert!(!blob.equal(a, b, &cancel).unwrap());
        let mut first = [0; 1];
        let mut second = [0; 1];
        blob.read(a, 0, &mut first).unwrap();
        blob.read(b, 0, &mut second).unwrap();
        assert_eq!((first[0], second[0]), (1, 2));
        drop(source);
        assert_eq!(std::fs::read(&fixture.source).unwrap(), bytes);
    }
}
