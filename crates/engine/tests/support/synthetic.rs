//! Test-only, deterministic MS-PST v36 writer. No source file is ever read.
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Store {
    pub root: Folder,
}
#[derive(Default)]
pub struct Folder {
    pub name: String,
    pub items: Vec<Item>,
    pub children: Vec<Folder>,
    pub unreadable: bool,
}
#[derive(Default)]
pub struct Item {
    pub class: String,
    pub size: u32,
    pub properties: Vec<Property>,
    pub recipients: Vec<Recipient>,
    pub attachments: Vec<Attachment>,
    pub headers_only: bool,
    pub cached: Vec<Property>,
    pub unreadable: bool,
}
impl Item {
    pub fn new(class: &str, size: u32) -> Self {
        Self {
            class: class.into(),
            size,
            ..Default::default()
        }
    }
}
#[derive(Clone)]
pub struct Property {
    pub id: u16,
    pub kind: u16,
    pub bytes: Vec<u8>,
}
impl Property {
    pub fn text(id: u16, text: &str) -> Self {
        Self {
            id,
            kind: 0x1f,
            bytes: unicode(text),
        }
    }
    pub fn binary(id: u16, bytes: Vec<u8>) -> Self {
        Self {
            id,
            kind: 0x102,
            bytes,
        }
    }
    pub fn time(id: u16, ticks: u64) -> Self {
        Self {
            id,
            kind: 0x40,
            bytes: ticks.to_le_bytes().to_vec(),
        }
    }
    pub fn int(id: u16, value: u32) -> Self {
        Self {
            id,
            kind: 3,
            bytes: value.to_le_bytes().to_vec(),
        }
    }
}
pub struct Recipient {
    pub name: String,
    pub address: String,
    pub kind: u32,
}
pub enum Attachment {
    Bytes(Vec<u8>),
    Embedded(Box<Item>),
    External(String),
}

type Subnodes = BTreeMap<u32, (u64, u64)>;
impl Store {
    pub fn build(&self) -> Vec<u8> {
        let mut w = Writer::default();
        w.node(
            0x21,
            0,
            &[
                Property::binary(0x0ff9, vec![0x23; 16]),
                Property::text(0x3001, "Synthetic store"),
            ],
        );
        // PidLidHeaderItem: PSETID_Common, LID 0x8578, mapped to 0x8000.
        let mut entry = vec![0; 8];
        p32(&mut entry, 0, 0x8578);
        p16(&mut entry, 4, 6); // GUID stream index 3, MNID_ID.
        w.node(
            0x61,
            0,
            &[
                Property::binary(
                    2,
                    vec![8, 0x20, 6, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0x46],
                ),
                Property::binary(3, entry),
                Property::binary(4, vec![0; 4]),
            ],
        );
        w.folder(&self.root, 0, &mut 0x120);
        w.finish()
    }
}
fn p16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn p32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn p64(b: &mut [u8], at: usize, n: u64) {
    b[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn unicode(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
// Independent bitwise implementation; never call the reader's checksum routines.
fn crc(b: &[u8]) -> u32 {
    let mut c = 0u32;
    for &v in b {
        c ^= v as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    c
}
fn signature(id: u64, offset: usize) -> u16 {
    let v = id ^ offset as u64;
    (v ^ (v >> 16)) as u16
}

// Each allocation fits on one HN page. HIDs address both page and allocation.
struct Heap {
    pages: Vec<Vec<Vec<u8>>>,
}
impl Heap {
    fn new() -> Self {
        Self {
            pages: vec![vec![]],
        }
    }
    fn alloc(&mut self, bytes: Vec<u8>) -> u32 {
        assert!(
            bytes.len() <= 3500,
            "large values belong in subnode streams"
        );
        let last = self.pages.last().unwrap();
        if 12 + last.iter().map(Vec::len).sum::<usize>() + bytes.len() + 6 + 2 * (last.len() + 1)
            > 8000
        {
            self.pages.push(vec![]);
        }
        let page = self.pages.len() - 1;
        let index = self.pages[page].len();
        self.pages[page].push(bytes);
        ((page as u32) << 19) | ((index as u32 + 1) << 5)
    }
    fn set(&mut self, hid: u32, bytes: Vec<u8>) {
        let target = &mut self.pages[(hid >> 19) as usize][((hid >> 5) & 0x3fff) as usize - 1];
        assert_eq!(target.len(), bytes.len());
        *target = bytes;
    }
    fn finish(self, kind: u8) -> Vec<Vec<u8>> {
        self.pages
            .into_iter()
            .enumerate()
            .map(|(i, allocs)| {
                let mut b = vec![0; if i == 0 { 12 } else { 2 }];
                if i == 0 {
                    b[2] = 0xec;
                    b[3] = kind;
                    p32(&mut b, 4, 32);
                }
                let mut offsets = Vec::new();
                for a in &allocs {
                    offsets.push(b.len() as u16);
                    b.extend(a);
                }
                offsets.push(b.len() as u16);
                let at = b.len();
                b.extend((allocs.len() as u16).to_le_bytes());
                b.extend([0, 0]);
                for n in offsets {
                    b.extend(n.to_le_bytes());
                }
                p16(&mut b, 0, at as u16);
                b
            })
            .collect()
    }
}
#[derive(Default)]
struct Writer {
    nodes: BTreeMap<u32, (u32, u64, u64)>,
    blocks: Vec<(u64, Vec<u8>)>,
}
impl Writer {
    fn block(&mut self, bytes: Vec<u8>, internal: bool) -> u64 {
        assert!(bytes.len() <= 8000);
        let bid = 12 + self.blocks.len() as u64 * 4 + if internal { 2 } else { 0 };
        self.blocks.push((bid, bytes));
        bid
    }
    fn stream(&mut self, pages: Vec<Vec<u8>>) -> u64 {
        let total: usize = pages.iter().map(Vec::len).sum();
        let bids: Vec<_> = pages.into_iter().map(|p| self.block(p, false)).collect();
        if bids.len() == 1 {
            return bids[0];
        }
        assert!(bids.len() <= 999, "test stream exceeds one XBLOCK");
        let mut x = vec![1, 1, 0, 0, 0, 0, 0, 0];
        p16(&mut x, 2, bids.len() as u16);
        p32(&mut x, 4, total as u32);
        for bid in bids {
            x.extend(bid.to_le_bytes());
        }
        self.block(x, true)
    }
    fn raw(&mut self, bytes: &[u8]) -> u64 {
        self.stream(bytes.chunks(8000).map(<[u8]>::to_vec).collect())
    }
    fn subs(&mut self, subs: Subnodes) -> u64 {
        if subs.is_empty() {
            return 0;
        }
        let mut b = vec![2, 0, 0, 0, 0, 0, 0, 0];
        p16(&mut b, 2, subs.len() as u16);
        for (nid, (data, sub)) in subs {
            b.extend((nid as u64).to_le_bytes());
            b.extend(data.to_le_bytes());
            b.extend(sub.to_le_bytes());
        }
        self.block(b, true)
    }
    fn context(&mut self, properties: &[Property], mut subs: Subnodes) -> (u64, u64) {
        let mut props = properties.to_vec();
        props.sort_by_key(|p| p.id);
        assert!(
            props.windows(2).all(|p| p[0].id != p[1].id),
            "duplicate property"
        );
        let mut h = Heap::new();
        let head = h.alloc(vec![0; 8]);
        let rec = h.alloc(vec![0; props.len() * 8]);
        let mut bth = vec![0xb5, 2, 6, 0, 0, 0, 0, 0];
        p32(&mut bth, 4, if props.is_empty() { 0 } else { rec });
        h.set(head, bth);
        let mut records = Vec::new();
        for p in props {
            records.extend(p.id.to_le_bytes());
            records.extend(p.kind.to_le_bytes());
            let cell = if matches!(p.kind, 2 | 3 | 4 | 0xa | 0xb) {
                let mut b = [0; 4];
                b[..p.bytes.len()].copy_from_slice(&p.bytes);
                u32::from_le_bytes(b)
            } else if p.bytes.len() > 3500 {
                let nid = 0x1001f + subs.len() as u32 * 32;
                let data = self.raw(&p.bytes);
                subs.insert(nid, (data, 0));
                nid
            } else {
                h.alloc(p.bytes)
            };
            records.extend(cell.to_le_bytes());
        }
        h.set(rec, records);
        (self.stream(h.finish(0xbc)), self.subs(subs))
    }
    fn node(&mut self, nid: u32, parent: u32, props: &[Property]) {
        let (data, sub) = self.context(props, BTreeMap::new());
        self.nodes.insert(nid, (parent, data, sub));
    }
    // Damage only the target heap; all indexes and checksums stay valid.
    fn unreadable(&mut self, data: u64) {
        let (_, bytes) = self.blocks.iter().find(|(id, _)| *id == data).unwrap();
        let leaf = if data & 2 != 0 {
            u64::from_le_bytes(bytes[8..16].try_into().unwrap())
        } else {
            data
        };
        self.blocks
            .iter_mut()
            .find(|(id, _)| *id == leaf)
            .unwrap()
            .1[2] = 0;
    }
    fn item(&mut self, item: &Item) -> (u64, u64) {
        let mut props = vec![
            Property::text(0x001a, &item.class),
            Property::int(0x0e08, item.size),
        ];
        props.extend(item.properties.clone());
        if item.headers_only {
            props.push(Property::int(0x8000, 1));
        }
        let mut subs = BTreeMap::new();
        if !item.recipients.is_empty() {
            let rows: Vec<_> = item
                .recipients
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    (
                        i as u32,
                        vec![
                            Property::int(0x0c15, r.kind),
                            Property::text(0x3001, &r.name),
                            Property::text(0x3002, "SMTP"),
                            Property::text(0x3003, &r.address),
                        ],
                    )
                })
                .collect();
            let data = self.table(&rows);
            subs.insert(0x692, (data, 0));
        }
        let mut attach_rows = Vec::new();
        for (i, a) in item.attachments.iter().enumerate() {
            let nid = 0x805 + i as u32 * 32;
            let mut nested = BTreeMap::new();
            let mut ap = vec![Property::text(0x3707, "synthetic.bin")];
            match a {
                Attachment::Bytes(bytes) => {
                    ap.push(Property::int(0x3705, 1));
                    ap.push(Property::binary(0x3701, bytes.clone()));
                }
                Attachment::External(url) => {
                    ap.push(Property::int(0x3705, 2));
                    ap.push(Property::text(0x370d, url));
                }
                Attachment::Embedded(inner) => {
                    ap.push(Property::int(0x3705, 5));
                    let embedded = self.item(inner);
                    nested.insert(0x804, embedded);
                    let mut object = 0x804u32.to_le_bytes().to_vec();
                    object.extend(0u32.to_le_bytes());
                    ap.push(Property {
                        id: 0x3701,
                        kind: 0xd,
                        bytes: object,
                    });
                }
            }
            subs.insert(nid, self.context(&ap, nested));
            attach_rows.push((nid, vec![Property::int(0x0e21, i as u32)]));
        }
        if !attach_rows.is_empty() {
            let data = self.table(&attach_rows);
            subs.insert(0x671, (data, 0));
        }
        let result = self.context(&props, subs);
        if item.unreadable {
            self.unreadable(result.0);
        }
        result
    }
    fn folder(&mut self, folder: &Folder, parent: u32, next: &mut u32) -> u32 {
        let nid = *next | 2;
        *next += 32;
        self.node(
            nid,
            if parent == 0 { nid } else { parent },
            &[
                Property::text(0x3001, &folder.name),
                Property::int(0x3602, folder.items.len() as u32),
                Property::int(0x3603, 0),
                Property::int(0x360a, u32::from(!folder.children.is_empty())),
                Property::int(0x3617, 0),
            ],
        );
        if folder.unreadable {
            self.unreadable(self.nodes[&nid].1);
        }
        let mut items = Vec::new();
        for item in &folder.items {
            let id = *next | 4;
            *next += 32;
            items.push((id, item.cached.clone()));
            let (data, sub) = self.item(item);
            self.nodes.insert(id, (nid, data, sub));
        }
        let children: Vec<_> = folder
            .children
            .iter()
            .map(|child| (self.folder(child, nid, next), vec![]))
            .collect();
        for (kind, rows) in [(0xd, &children), (0xe, &items), (0xf, &vec![])] {
            let data = self.table(rows);
            self.nodes.insert((nid & !31) | kind, (0, data, 0));
        }
        nid
    }
    fn table(&mut self, rows: &[(u32, Vec<Property>)]) -> u64 {
        // Required row ID/version plus the union of described cached cells.
        let mut columns = BTreeMap::from([(0x67f2, 3), (0x67f3, 3)]);
        for (_, cells) in rows {
            for p in cells {
                assert!(!matches!(p.id, 0x67f2 | 0x67f3));
                if let Some(kind) = columns.insert(p.id, p.kind) {
                    assert_eq!(kind, p.kind);
                }
            }
        }
        assert!(columns.len() <= 255);
        let width = |kind| match kind {
            0x40 | 0x14 | 5 | 6 | 7 => 8,
            2 => 2,
            0xb => 1,
            _ => 4,
        };
        let mut offsets = BTreeMap::from([(0x67f2, 0), (0x67f3, 4)]);
        let mut end = 8;
        let mut groups = [0; 4];
        for (g, sizes) in [&[8, 4][..], &[2][..], &[1][..]].into_iter().enumerate() {
            for (&id, &kind) in &columns {
                if !matches!(id, 0x67f2 | 0x67f3) && sizes.contains(&width(kind)) {
                    offsets.insert(id, end);
                    end += width(kind);
                }
            }
            groups[g] = end;
        }
        groups[3] = end + columns.len().div_ceil(8);
        let mut columns: Vec<_> = columns.into_iter().collect();
        columns.sort_by_key(|(id, _)| {
            if *id == 0x67f2 {
                0
            } else if *id == 0x67f3 {
                1
            } else {
                *id as u32 + 2
            }
        });
        let mut h = Heap::new();
        let info_id = h.alloc(vec![0; 22 + columns.len() * 8]);
        let bth_id = h.alloc(vec![0; 8]);
        let index_id = h.alloc(vec![0; rows.len() * 8]);
        let rows_id = h.alloc(vec![0; rows.len() * groups[3]]);
        let mut info = vec![0; 22 + columns.len() * 8];
        info[0] = 0x7c;
        info[1] = columns.len() as u8;
        for (i, n) in groups.iter().enumerate() {
            p16(&mut info, 2 + 2 * i, *n as u16);
        }
        p32(&mut info, 10, bth_id);
        p32(&mut info, 14, if rows.is_empty() { 0 } else { rows_id });
        for (i, (id, kind)) in columns.iter().copied().enumerate() {
            p32(&mut info, 22 + 8 * i, ((id as u32) << 16) | kind as u32);
            p16(&mut info, 26 + 8 * i, offsets[&id] as u16);
            info[28 + 8 * i] = width(kind) as u8;
            info[29 + 8 * i] = i as u8;
        }
        h.set(info_id, info);
        let mut bth = vec![0xb5, 4, 4, 0, 0, 0, 0, 0];
        p32(&mut bth, 4, if rows.is_empty() { 0 } else { index_id });
        h.set(bth_id, bth);
        let mut index = vec![0; rows.len() * 8];
        let mut matrix = vec![0; rows.len() * groups[3]];
        for (i, (nid, cells)) in rows.iter().enumerate() {
            p32(&mut index, i * 8, *nid);
            p32(&mut index, i * 8 + 4, i as u32);
            let mut values: BTreeMap<_, _> =
                cells.iter().map(|p| (p.id, p.bytes.clone())).collect();
            values.insert(0x67f2, nid.to_le_bytes().to_vec());
            values.insert(0x67f3, 1u32.to_le_bytes().to_vec());
            for (bit, (id, kind)) in columns.iter().copied().enumerate() {
                if let Some(bytes) = values.get(&id) {
                    let cell = match kind {
                        0x1e | 0x1f | 0x102 | 0xd => h.alloc(bytes.clone()).to_le_bytes().to_vec(),
                        _ => bytes.clone(),
                    };
                    assert_eq!(cell.len(), width(kind));
                    let start = i * groups[3] + offsets[&id];
                    matrix[start..start + cell.len()].copy_from_slice(&cell);
                    matrix[i * groups[3] + end + bit / 8] |= 0x80 >> (bit % 8);
                }
            }
        }
        h.set(index_id, index);
        h.set(rows_id, matrix);
        self.stream(h.finish(0x7c))
    }
    fn finish(self) -> Vec<u8> {
        assert!(
            self.nodes.len() <= 126 && self.blocks.len() <= 169,
            "synthetic index capacity exceeded"
        );
        let mut b = vec![0; 12288];
        let mut nbt = Vec::new();
        for (nid, (parent, data, sub)) in self.nodes {
            let mut entry = vec![0; 32];
            p64(&mut entry, 0, nid as u64);
            p64(&mut entry, 8, data);
            p64(&mut entry, 16, sub);
            p32(&mut entry, 24, parent);
            p32(&mut entry, 28, 2);
            nbt.extend(entry);
        }
        let mut bbt = Vec::new();
        for (id, bytes) in self.blocks {
            let at = b.len();
            let padded = (bytes.len() + 24).div_ceil(512) * 512;
            b.resize(at + padded, 0);
            b[at..at + bytes.len()].copy_from_slice(&bytes);
            let t = at + padded - 24;
            p16(&mut b, t, bytes.len() as u16);
            p16(&mut b, t + 2, signature(id, at));
            p32(&mut b, t + 4, crc(&bytes));
            p64(&mut b, t + 8, id);
            p16(&mut b, t + 18, bytes.len() as u16);
            let mut entry = vec![0; 24];
            p64(&mut entry, 0, id);
            p64(&mut entry, 8, at as u64);
            p16(&mut entry, 16, bytes.len() as u16);
            p16(&mut entry, 18, 1);
            bbt.extend(entry);
        }
        b[4096..8192].copy_from_slice(&page(&nbt, 32, 0x81, 4, 4096));
        b[8192..12288].copy_from_slice(&page(&bbt, 24, 0x80, 8, 8192));
        b[..4].copy_from_slice(b"!BDN");
        b[8..10].copy_from_slice(b"SO");
        p16(&mut b, 10, 36);
        p16(&mut b, 12, 19);
        b[14] = 1;
        b[15] = 1;
        let len = b.len() as u64;
        p64(&mut b, 184, len);
        p64(&mut b, 216, 4);
        p64(&mut b, 224, 4096);
        p64(&mut b, 232, 8);
        p64(&mut b, 240, 8192);
        b[248] = 1;
        b[512] = 0x80;
        let partial = crc(&b[8..479]);
        p32(&mut b, 4, partial);
        let full = crc(&b[8..524]);
        p32(&mut b, 524, full);
        b
    }
}
fn page(entries: &[u8], width: u8, kind: u8, id: u64, at: usize) -> Vec<u8> {
    let mut b = vec![0; 4096];
    b[..entries.len()].copy_from_slice(entries);
    p16(&mut b, 4056, (entries.len() / width as usize) as u16);
    p16(&mut b, 4058, 4056 / width as u16);
    b[4060] = width;
    b[4072] = kind;
    b[4073] = kind;
    p16(&mut b, 4074, signature(id, at));
    p64(&mut b, 4080, id);
    let sum = crc(&b[..4072]);
    p32(&mut b, 4076, sum);
    b
}
