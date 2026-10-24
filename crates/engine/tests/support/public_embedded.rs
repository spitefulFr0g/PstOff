//! Test-only augmentation of the hash-pinned public OST. Keep its complete store,
//! folders, templates, tables and original message/recipient properties. Append
//! synthetic attachment data and replacement indexes; never edit the input file.
use pstoff_format::ndb::{Block, Pst};
use std::{collections::BTreeMap, path::Path};

fn p16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn p32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn p64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn u32at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn crc(b: &[u8]) -> u32 {
    let mut c = 0;
    for v in b {
        c ^= *v as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 == 0 { 0 } else { 0xedb88320 };
        }
    }
    c
}
struct Append {
    bytes: Vec<u8>,
    blocks: Vec<Block>,
    next: u64,
}
impl Append {
    fn id(&mut self, internal: bool) -> u64 {
        self.next += 4;
        self.next | if internal { 2 } else { 0 }
    }
    fn align(&mut self, n: usize) {
        self.bytes.resize(self.bytes.len().div_ceil(n) * n, 0);
    }
    fn block(&mut self, data: &[u8], internal: bool) -> u64 {
        assert!(data.len() <= 65512);
        self.align(512);
        let ib = self.bytes.len();
        let bid = self.id(internal);
        let span = (data.len() + 24).div_ceil(512) * 512;
        self.bytes.extend_from_slice(data);
        self.bytes.resize(ib + span, 0);
        let t = ib + span - 24;
        p16(&mut self.bytes, t, data.len() as u16);
        let sig = ib as u64 ^ bid;
        p16(&mut self.bytes, t + 2, (sig ^ (sig >> 16)) as u16);
        p32(&mut self.bytes, t + 4, crc(data));
        p64(&mut self.bytes, t + 8, bid);
        p16(&mut self.bytes, t + 18, data.len() as u16);
        self.blocks.push(Block {
            bid,
            ib: ib as u64,
            cb: data.len() as u16,
            cref: 2,
        });
        bid
    }
    fn stream(&mut self, blocks: &[Vec<u8>]) -> u64 {
        let parts: Vec<_> = blocks.iter().map(|b| self.block(b, false)).collect();
        if parts.len() == 1 {
            return parts[0];
        }
        let mut x = vec![0; 8 + parts.len() * 8];
        x[0] = 1;
        x[1] = 1;
        p16(&mut x, 2, parts.len() as u16);
        p32(&mut x, 4, blocks.iter().map(Vec::len).sum::<usize>() as u32);
        for (i, bid) in parts.iter().enumerate() {
            p64(&mut x, 8 + i * 8, *bid);
        }
        self.block(&x, true)
    }
    fn subs(&mut self, entries: &BTreeMap<u32, (u64, u64)>) -> u64 {
        let mut b = vec![0; 8 + entries.len() * 24];
        b[0] = 2;
        p16(&mut b, 2, entries.len() as u16);
        for (i, (nid, (data, sub))) in entries.iter().enumerate() {
            p64(&mut b, 8 + i * 24, *nid as u64);
            p64(&mut b, 16 + i * 24, *data);
            p64(&mut b, 24 + i * 24, *sub);
        }
        self.block(&b, true)
    }
    fn tree(&mut self, mut records: Vec<(u64, Vec<u8>)>, kind: u8, mut width: usize) -> (u64, u64) {
        let mut level = 0;
        loop {
            let mut parents = vec![];
            for chunk in records.chunks(4056 / width) {
                self.align(4096);
                let at = self.bytes.len();
                let bid = self.id(false);
                let mut b = vec![0; 4096];
                for (i, (_, entry)) in chunk.iter().enumerate() {
                    b[i * width..(i + 1) * width].copy_from_slice(entry);
                }
                p16(&mut b, 4056, chunk.len() as u16);
                p16(&mut b, 4058, (4056 / width) as u16);
                b[4060] = width as u8;
                b[4061] = level;
                b[4072] = kind;
                b[4073] = kind;
                let sig = at as u64 ^ bid;
                p16(&mut b, 4074, (sig ^ (sig >> 16)) as u16);
                let sum = crc(&b[..4072]);
                p32(&mut b, 4076, sum);
                p64(&mut b, 4080, bid);
                self.bytes.extend(b);
                let mut entry = chunk[0].0.to_le_bytes().to_vec();
                entry.extend(bid.to_le_bytes());
                entry.extend((at as u64).to_le_bytes());
                parents.push((chunk[0].0, entry));
            }
            if parents.len() == 1 {
                let e = &parents[0].1;
                return (
                    u64::from_le_bytes(e[8..16].try_into().unwrap()),
                    u64::from_le_bytes(e[16..24].try_into().unwrap()),
                );
            }
            records = parents;
            width = 24;
            level += 1;
        }
    }
}

// Patch one inline property in the copied original heap, retaining all other
// original allocations, subnode references and source cache identities.
fn add_attachment_flag(blocks: &mut [Vec<u8>]) {
    let heap = pstoff_format::ltp::Heap::new(blocks.to_vec()).unwrap();
    let h = heap.item(heap.user_root).unwrap();
    let mut pending = vec![(u32at(h, 4), h[3])];
    while let Some((hid, level)) = pending.pop() {
        let rows = heap.item(hid).unwrap();
        if level != 0 {
            pending.extend(rows.chunks_exact(6).map(|r| (u32at(r, 2), level - 1)));
            continue;
        }
        for (i, row) in rows.chunks_exact(8).enumerate() {
            if row[..4] == [7, 14, 3, 0] {
                let b = &mut blocks[(hid >> 16) as usize];
                let map = u16::from_le_bytes(b[..2].try_into().unwrap()) as usize;
                let index = ((hid >> 5) & 0x7ff) as usize;
                let at = u16::from_le_bytes(
                    b[map + 2 + index * 2..map + 4 + index * 2]
                        .try_into()
                        .unwrap(),
                ) as usize
                    + i * 8
                    + 4;
                p32(b, at, u32at(row, 4) | 0x10);
                return;
            }
        }
    }
    panic!("public message has no flags");
}

pub fn store(path: &Path) -> (Vec<u8>, u32, u32) {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d"
    );
    let mut source = Pst::open(path.to_str().unwrap()).unwrap();
    let mut nodes = source.nodes();
    nodes.sort_by_key(|n| n.nid);
    let blocks = source.blocks();
    let next = blocks.iter().map(|b| b.bid & !3).max().unwrap();
    let mut out = Append {
        bytes,
        blocks,
        next,
    };
    let message = nodes
        .iter_mut()
        .find(|n| n.nid_type() == 4 && n.bid_sub != 0)
        .unwrap();
    let mut message_blocks = source.node_blocks(message.bid_data).unwrap();
    add_attachment_flag(&mut message_blocks);
    let mut subs: BTreeMap<_, _> = source
        .subnodes(message.bid_sub)
        .unwrap()
        .into_iter()
        .map(|(nid, s)| (nid, (s.data, s.sub)))
        .collect();
    assert!(!subs.contains_key(&0x671) && !subs.contains_key(&0x685));
    let recipient = nodes_template(&mut source, 0x692);
    let time = 132000000000000000u64.to_le_bytes().to_vec();
    let cells = vec![
        (
            0x001a,
            0x1f,
            "IPM.Note"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ),
        (0x0e07, 3, 1u32.to_le_bytes().to_vec()),
        (0x0e08, 3, 0u32.to_le_bytes().to_vec()),
        (0x3007, 0x40, time.clone()),
        (0x3008, 0x40, time),
        (0x300b, 0x102, vec![0x33; 16]),
        (0x4000, 0x102, vec![0x5a; 64]),
        (0x4001, 0x102, vec![]),
    ];
    let mut embedded = super::generated::pc_with_external(&cells, &[(0x4001, 0x7ff)]);
    // Composite size counts data streams recursively, excluding SL/SI indexes.
    let object_size = (embedded.len() + recipient.iter().map(Vec::len).sum::<usize>() + 16) as u32;
    // Same allocation layout as the fixture PC above: third record is size.
    p32(&mut embedded, 20 + 2 * 8 + 4, object_size);
    let raw = out.block(&[0x7b; 16], false);
    let rec = out.stream(&recipient);
    let below = out.subs(&BTreeMap::from([(0x692, (rec, 0)), (0x7ff, (raw, 0))]));
    let embedded = out.block(&embedded, false);
    let attachment_subs = out.subs(&BTreeMap::from([(0x704, (embedded, below))]));
    let mut object = 0x704u32.to_le_bytes().to_vec();
    object.extend(object_size.to_le_bytes());
    let filename: Vec<_> = "synthetic.msg"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let attachment_size = object_size + 12 + filename.len() as u32;
    let attachment_cells = vec![
        (0x0e20, 3, attachment_size.to_le_bytes().to_vec()),
        (0x3701, 0xd, object),
        (0x3704, 0x1f, filename.clone()),
        (0x3705, 3, 5u32.to_le_bytes().to_vec()),
        (0x370b, 3, u32::MAX.to_le_bytes().to_vec()),
    ];
    let attachment = out.block(&super::generated::pc(&attachment_cells), false);
    let mut table = super::generated::policy_table(&[
        (0x0e20, 3, attachment_size.to_le_bytes().to_vec()),
        (0x3704, 0x1f, filename),
        (0x3705, 3, 5u32.to_le_bytes().to_vec()),
        (0x370b, 3, u32::MAX.to_le_bytes().to_vec()),
    ]);
    let map = u16::from_le_bytes(table[..2].try_into().unwrap()) as usize;
    for offset in [8, 10] {
        let at =
            u16::from_le_bytes(table[map + offset..map + offset + 2].try_into().unwrap()) as usize;
        p32(&mut table, at, 0x685);
    }
    let table = out.block(&table, false);
    subs.insert(0x671, (table, 0));
    subs.insert(0x685, (attachment, attachment_subs));
    message.bid_data = out.stream(&message_blocks);
    message.bid_sub = out.subs(&subs);
    let message_id = message.nid;
    let mut nbt = vec![];
    for n in nodes {
        let mut e = vec![0; 32];
        p64(&mut e, 0, n.nid as u64);
        p64(&mut e, 8, n.bid_data);
        p64(&mut e, 16, n.bid_sub);
        p32(&mut e, 24, n.nid_parent);
        nbt.push((n.nid as u64, e));
    }
    let nbt = out.tree(nbt, 0x81, 32);
    out.blocks.sort_by_key(|b| b.bid);
    let bbt = out
        .blocks
        .iter()
        .map(|b| {
            let mut e = vec![0; 24];
            p64(&mut e, 0, b.bid);
            p64(&mut e, 8, b.ib);
            p16(&mut e, 16, b.cb);
            p16(&mut e, 18, b.cref);
            (b.bid, e)
        })
        .collect();
    let bbt = out.tree(bbt, 0x80, 24);
    let len = out.bytes.len() as u64;
    p64(&mut out.bytes, 184, len);
    for (at, (bid, ib)) in [(216, nbt), (232, bbt)] {
        p64(&mut out.bytes, at, bid);
        p64(&mut out.bytes, at + 8, ib);
    }
    p64(&mut out.bytes, 32, out.next + 4);
    // The engine reads indexed data only; output allocation maps are regenerated.
    // Full source allocation-map validation remains a separate fixture gate.
    let partial = crc(&out.bytes[8..479]);
    p32(&mut out.bytes, 4, partial);
    let full = crc(&out.bytes[8..524]);
    p32(&mut out.bytes, 524, full);
    (out.bytes, message_id, object_size)
}

fn nodes_template(source: &mut Pst, nid: u32) -> Vec<Vec<u8>> {
    let n = source.nodes().into_iter().find(|n| n.nid == nid).unwrap();
    assert_eq!(n.bid_sub, 0);
    source.node_blocks(n.bid_data).unwrap()
}
