fn put16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn put64(b: &mut [u8], at: usize, n: u64) {
    b[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
// Independent fixture checksum: deliberately does not call the format implementation.
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
fn seal_header(b: &mut [u8]) {
    put32(b, 4, crc(&b[8..479]));
    put32(b, 524, crc(&b[8..524]));
}
fn header(version: u16, encoding: u8) -> Vec<u8> {
    let mut b = vec![0; 564];
    b[..4].copy_from_slice(b"!BDN");
    b[8..10].copy_from_slice(b"SO");
    put16(&mut b, 10, version);
    b[512] = 0x80;
    b[513] = encoding;
    b[248] = 1;
    seal_header(&mut b);
    b
}

fn page(entries: &[u8], count: u16, width: u8, kind: u8, id: u64) -> Vec<u8> {
    let mut b = vec![0; 4096];
    b[..entries.len()].copy_from_slice(entries);
    put16(&mut b, 4056, count);
    put16(&mut b, 4058, 100);
    b[4060] = width;
    b[4072] = kind;
    b[4073] = kind;
    put64(&mut b, 4080, id);
    let sum = crc(&b[..4072]);
    put32(&mut b, 4076, sum);
    b
}
// A generated PC with two allocations: BTH header and its inline size property.
fn size_pc(size: u32) -> Vec<u8> {
    let mut b = vec![0; 40];
    put16(&mut b, 0, 28);
    b[2] = 0xec;
    b[3] = 0xbc;
    put32(&mut b, 4, 32);
    b[12..16].copy_from_slice(&[0xb5, 2, 6, 0]);
    put32(&mut b, 16, 64);
    put16(&mut b, 20, 0x0e08);
    put16(&mut b, 22, 3);
    put32(&mut b, 24, size);
    put16(&mut b, 28, 2);
    put16(&mut b, 32, 12);
    put16(&mut b, 34, 20);
    put16(&mut b, 36, 28);
    b
}
pub fn generated_store(nodes: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let length = (12288
        + nodes
            .iter()
            .map(|n| (n.2.len() + 24).div_ceil(512) * 512)
            .sum::<usize>())
    .max(16384);
    let mut b = vec![0; length];
    b[..564].copy_from_slice(&header(36, 0));
    put64(&mut b, 184, length as u64);
    put64(&mut b, 216, 4);
    put64(&mut b, 224, 4096);
    put64(&mut b, 232, 8);
    put64(&mut b, 240, 8192);
    let mut n = vec![0; nodes.len() * 32];
    let mut entries = vec![0; nodes.len() * 24];
    let mut at = 12288;
    for (i, (nid, parent, pc)) in nodes.iter().enumerate() {
        let id = 12 + i as u64 * 4;
        let span = (pc.len() + 24).div_ceil(512) * 512;
        put64(&mut n, i * 32, *nid as u64);
        put64(&mut n, i * 32 + 8, id);
        put32(&mut n, i * 32 + 24, *parent);
        b[at..at + pc.len()].copy_from_slice(pc);
        let t = at + span - 24;
        put16(&mut b, t, pc.len() as u16);
        put32(&mut b, t + 4, crc(pc));
        put64(&mut b, t + 8, id);
        put16(&mut b, t + 18, pc.len() as u16);
        put64(&mut entries, i * 24, id);
        put64(&mut entries, i * 24 + 8, at as u64);
        put16(&mut entries, i * 24 + 16, pc.len() as u16);
        put16(&mut entries, i * 24 + 18, 1);
        at += span;
    }
    b[4096..8192].copy_from_slice(&page(&n, nodes.len() as u16, 32, 0x81, 4));
    b[8192..12288].copy_from_slice(&page(&entries, nodes.len() as u16, 24, 0x80, 8));
    seal_header(&mut b);
    b
}
pub fn pc(cells: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    pc_with_external(cells, &[])
}
pub fn pc_with_external(cells: &[(u16, u16, Vec<u8>)], external: &[(u16, u32)]) -> Vec<u8> {
    let mut allocations = vec![vec![0xb5, 2, 6, 0, 64, 0, 0, 0], Vec::new()];
    for (id, kind, bytes) in cells {
        allocations[1].extend_from_slice(&id.to_le_bytes());
        allocations[1].extend_from_slice(&kind.to_le_bytes());
        if let Some((_, nid)) = external.iter().find(|(prop, _)| prop == id) {
            allocations[1].extend_from_slice(&nid.to_le_bytes());
        } else if *kind == 3 {
            allocations[1].extend_from_slice(bytes)
        } else {
            let hid = (allocations.len() as u32 + 1) * 32;
            allocations[1].extend_from_slice(&hid.to_le_bytes());
            allocations.push(bytes.clone());
        }
    }
    let mut b = vec![0; 12];
    b[2] = 0xec;
    b[3] = 0xbc;
    put32(&mut b, 4, 32);
    let mut offsets = vec![];
    for a in allocations {
        offsets.push(b.len() as u16);
        b.extend(a);
    }
    offsets.push(b.len() as u16);
    let map = b.len();
    b.extend_from_slice(&((offsets.len() - 1) as u16).to_le_bytes());
    b.extend_from_slice(&[0, 0]);
    for n in offsets {
        b.extend_from_slice(&n.to_le_bytes());
    }
    put16(&mut b, 0, map as u16);
    b
}
pub fn skeleton_nodes(store: Vec<u8>) -> Vec<(u32, u32, Vec<u8>)> {
    let name = |s: &str| {
        pc(&[(
            0x3001,
            0x1f,
            s.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        )])
    };
    vec![
        (0x21, 0, store),
        (0x122, 0, name("Root")),
        (0x2022, 0x122, name("Root - Mailbox")),
        (0x2042, 0x2022, name("Finder")),
        (0x2062, 0x2022, name("IPM_SUBTREE")),
        (0x2082, 0x2062, name("Deleted Items")),
        (0x124, 0x2062, size_pc(1234)),
    ]
}
pub fn store() -> Vec<u8> {
    skeleton(pc(&[(0x0ff9, 0x102, vec![1; 16])]))
}

pub fn oversized_pc() -> Vec<u8> {
    let mut cells = vec![(0x0ff9, 0x102, vec![1; 16])];
    for i in 0..24u16 {
        cells.push((0x4000 + i, 0x102, vec![i as u8; 1000]));
    }
    pc(&cells)
}

pub fn skeleton(store: Vec<u8>) -> Vec<u8> {
    generated_store(&skeleton_nodes(store))
}
pub fn cached_table() -> Vec<u8> {
    let mut info = vec![0; 54];
    info[0] = 0x7c;
    info[1] = 4;
    for at in [2, 4, 6] {
        put16(&mut info, at, 20);
    }
    put16(&mut info, 8, 21);
    put32(&mut info, 10, 64);
    put32(&mut info, 14, 128);
    for (i, (tag, at)) in [
        (0x67f20003u32, 0),
        (0x67f30003, 4),
        (0x0e080003, 8),
        (0x66280014, 12),
    ]
    .iter()
    .enumerate()
    {
        put32(&mut info, 22 + i * 8, *tag);
        put16(&mut info, 26 + i * 8, *at);
        info[28 + i * 8] = if i == 3 { 8 } else { 4 };
        info[29 + i * 8] = i as u8;
    }
    let mut index = vec![0; 8];
    put32(&mut index, 0, 0x124);
    let mut row = vec![0; 21];
    put32(&mut row, 0, 0x124);
    put32(&mut row, 4, 1);
    put32(&mut row, 8, 999);
    row[20] = 0xf0;
    let allocations = [info, vec![0xb5, 4, 4, 0, 96, 0, 0, 0], index, row];
    let mut b = vec![0; 12];
    b[2] = 0xec;
    b[3] = 0x7c;
    put32(&mut b, 4, 32);
    let mut offsets = vec![];
    for a in allocations {
        offsets.push(b.len() as u16);
        b.extend(a);
    }
    offsets.push(b.len() as u16);
    let map = b.len();
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&[0, 0]);
    for n in offsets {
        b.extend_from_slice(&n.to_le_bytes());
    }
    put16(&mut b, 0, map as u16);
    b
}
pub fn attachment_store() -> Vec<u8> {
    let mut nodes = skeleton_nodes(pc(&[(0x0ff9, 0x102, vec![1; 16])]));
    nodes.push((
        0x5000,
        0,
        pc(&[
            (0x3701, 0x102, vec![0xa5; 40000]),
            (0x3705, 3, 1u32.to_le_bytes().to_vec()),
        ]),
    ));
    let mut sub = vec![0; 32];
    sub[0] = 2;
    put16(&mut sub, 2, 1);
    put64(&mut sub, 8, 0x685);
    put64(&mut sub, 16, 40);
    nodes.push((0x5020, 0, sub));
    let mut b = generated_store(&nodes);
    put64(&mut b, 4096 + 6 * 32 + 16, 46);
    put64(&mut b, 4096 + 8 * 32 + 8, 46);
    put64(&mut b, 8192 + 8 * 24, 46);
    let at = 12288
        + nodes[..8]
            .iter()
            .map(|n| (n.2.len() + 24).div_ceil(512) * 512)
            .sum::<usize>();
    put64(&mut b, at + 496, 46);
    let c = crc(&b[4096..4096 + 4072]);
    put32(&mut b, 4096 + 4076, c);
    let c = crc(&b[8192..8192 + 4072]);
    put32(&mut b, 8192 + 4076, c);
    b
}

// A one-row, indexed cache with independent synthetic values.
pub fn policy_table(cells: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut columns = vec![
        (0x67f2, 3, 0x124u32.to_le_bytes().to_vec()),
        (0x67f3, 3, 1u32.to_le_bytes().to_vec()),
    ];
    columns.extend_from_slice(cells);
    let mut info = vec![0; 22 + columns.len() * 8];
    info[0] = 0x7c;
    info[1] = columns.len() as u8;
    put32(&mut info, 10, 64);
    put32(&mut info, 14, 128);
    let mut row = vec![];
    let mut values = vec![];
    for (i, (id, kind, bytes)) in columns.iter().enumerate() {
        put32(&mut info, 22 + i * 8, (*id as u32) << 16 | *kind as u32);
        put16(&mut info, 26 + i * 8, row.len() as u16);
        let size = if *kind == 0x40 { 8 } else { 4 };
        info[28 + i * 8] = size;
        info[29 + i * 8] = i as u8;
        if matches!(*kind, 0x102 | 0x1f) {
            row.extend_from_slice(&((5 + values.len() as u32) * 32).to_le_bytes());
            values.push(bytes.clone());
        } else {
            row.extend_from_slice(bytes);
        }
    }
    for at in [2, 4, 6] {
        put16(&mut info, at, row.len() as u16);
    }
    let bitmap = columns.len().div_ceil(8);
    row.extend(std::iter::repeat_n(0xff, bitmap));
    put16(&mut info, 8, row.len() as u16);
    let mut index = vec![0; 8];
    put32(&mut index, 0, 0x124);
    let mut allocations = vec![info, vec![0xb5, 4, 4, 0, 96, 0, 0, 0], index, row];
    allocations.extend(values);
    let mut b = vec![0; 12];
    b[2] = 0xec;
    b[3] = 0x7c;
    put32(&mut b, 4, 32);
    let mut offsets = vec![];
    for a in allocations {
        offsets.push(b.len() as u16);
        b.extend(a);
    }
    offsets.push(b.len() as u16);
    let map = b.len();
    b.extend_from_slice(&((offsets.len() - 1) as u16).to_le_bytes());
    b.extend([0, 0]);
    for n in offsets {
        b.extend_from_slice(&n.to_le_bytes());
    }
    put16(&mut b, 0, map as u16);
    b
}

pub fn embedded_store() -> Vec<u8> {
    embedded_store_with_external_payload(false)
}
pub fn embedded_store_external_payload() -> Vec<u8> {
    embedded_store_with_external_payload(true)
}
fn embedded_store_with_external_payload(external: bool) -> Vec<u8> {
    let mut nodes = skeleton_nodes(pc(&[(0x0ff9, 0x102, vec![1; 16])]));
    nodes[6].2 = pc(&[
        (0x0e07, 3, 0x10u32.to_le_bytes().to_vec()),
        (0x0e08, 3, 1234u32.to_le_bytes().to_vec()),
    ]);
    let mut embedded_pc = pc_with_external(
        &[
            (0x0e08, 3, 4321u32.to_le_bytes().to_vec()),
            (0x4000, 0x102, vec![0x5a; 64]),
        ],
        if external { &[(0x4000, 0x7ff)] } else { &[] },
    );
    let embedded_size = if external {
        (embedded_pc.len() + 16) as u32
    } else {
        4321
    };
    put32(&mut embedded_pc, 24, embedded_size);
    let mut object = 0x704u32.to_le_bytes().to_vec();
    object.extend(embedded_size.to_le_bytes());
    nodes.push((
        0x5000,
        0,
        pc(&[
            (0x0e20, 3, 4321u32.to_le_bytes().to_vec()),
            (0x3701, 0x0d, object),
            (
                0x3704,
                0x1f,
                "synthetic.msg"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            ),
            (0x3705, 3, 5u32.to_le_bytes().to_vec()),
            (0x370b, 3, u32::MAX.to_le_bytes().to_vec()),
        ]),
    ));
    nodes.push((0x5020, 0, embedded_pc));
    nodes.push((0x5040, 0, vec![0x7b; 16]));
    let mut inner = vec![0; 32];
    inner[0] = 2;
    put16(&mut inner, 2, 1);
    put64(&mut inner, 8, 0x7ff);
    put64(&mut inner, 16, 48);
    nodes.push((0x5060, 0, inner));
    // One attachment row, indexed by the actual attachment subnode NID.
    let mut table = policy_table(&[
        (0x0e20, 3, 4321u32.to_le_bytes().to_vec()),
        (
            0x3704,
            0x1f,
            "synthetic.msg"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ),
        (0x3705, 3, 5u32.to_le_bytes().to_vec()),
        (0x370b, 3, u32::MAX.to_le_bytes().to_vec()),
    ]);
    let map = u16::from_le_bytes(table[..2].try_into().unwrap()) as usize;
    for offset in [8, 10] {
        let at =
            u16::from_le_bytes(table[map + offset..map + offset + 2].try_into().unwrap()) as usize;
        put32(&mut table, at, 0x685);
    }
    // Added after the outer tree below, so its data BID is 60.
    let mut outer = vec![0; 56];
    outer[0] = 2;
    put16(&mut outer, 2, 2);
    put64(&mut outer, 8, 0x671);
    put64(&mut outer, 16, 60);
    put64(&mut outer, 32, 0x685);
    put64(&mut outer, 40, 40);
    put64(&mut outer, 48, 66);
    nodes.push((0x5080, 0, outer));
    nodes.push((0x50a0, 0, table));
    // Object payload belongs to the attachment's local subnode tree.
    let mut attachment_subs = vec![0; 32];
    attachment_subs[0] = 2;
    put16(&mut attachment_subs, 2, 1);
    put64(&mut attachment_subs, 8, 0x704);
    put64(&mut attachment_subs, 16, 44);
    put64(&mut attachment_subs, 24, 54);
    nodes.push((0x50c0, 0, attachment_subs));
    let mut b = generated_store(&nodes);
    put64(&mut b, 4096 + 6 * 32 + 16, 58);
    put64(&mut b, 4096 + 7 * 32 + 16, 66);
    put64(&mut b, 4096 + 8 * 32 + 16, 54);
    for i in [10, 11, 13] {
        let bid = 12 + i as u64 * 4 + 2;
        put64(&mut b, 4096 + i * 32 + 8, bid);
        put64(&mut b, 8192 + i * 24, bid);
        let at = 12288
            + nodes[..i]
                .iter()
                .map(|n| (n.2.len() + 24).div_ceil(512) * 512)
                .sum::<usize>();
        let span = (nodes[i].2.len() + 24).div_ceil(512) * 512;
        put64(&mut b, at + span - 16, bid);
    }
    let c = crc(&b[4096..4096 + 4072]);
    put32(&mut b, 4096 + 4076, c);
    let c = crc(&b[8192..8192 + 4072]);
    put32(&mut b, 8192 + 4076, c);
    b
}
