mod support;
use pstoff_engine::{
    convert, inspect, CancelToken, ConvertOptions, ErrorKind, Support, UnsupportedReason,
};

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
#[test]
fn unsupported_matrix_never_modifies_source_or_creates_output() {
    for (v, e, reason) in [
        (14, 0, UnsupportedReason::Ansi),
        (15, 0, UnsupportedReason::Ansi),
        (21, 0, UnsupportedReason::OlderUnicode(21)),
        (23, 0, UnsupportedReason::OlderUnicode(23)),
        (37, 0, UnsupportedReason::UnknownVersion(37)),
        (99, 0, UnsupportedReason::UnknownVersion(99)),
        (36, 1, UnsupportedReason::Encoding(1)),
        (36, 2, UnsupportedReason::Encoding(2)),
        (36, 255, UnsupportedReason::Encoding(255)),
    ] {
        let f = support::Fixture::invalid();
        let bytes = header(v, e);
        std::fs::write(&f.source, &bytes).unwrap();
        let inspection = inspect(&f.source).unwrap();
        assert_eq!(inspection.support, Support::Unsupported(reason.clone()));
        assert!(inspection.folders.is_empty());
        let options = ConvertOptions::new(f.root.join("absent"), "fixture".into());
        assert!(
            matches!(convert(&f.source,&options,&mut |_| panic!("unsupported input made progress"),&CancelToken::default()).unwrap_err().kind,ErrorKind::Unsupported(r) if r == reason)
        );
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&f.root).unwrap().count(), 1);
    }
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
fn generated_store(nodes: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let length = (12288 + nodes.len() * 512).max(16384);
    let mut b = vec![0; length];
    b[..564].copy_from_slice(&header(36, 0));
    put64(&mut b, 184, length as u64);
    put64(&mut b, 216, 4);
    put64(&mut b, 224, 4096);
    put64(&mut b, 232, 8);
    put64(&mut b, 240, 8192);
    let mut n = vec![0; nodes.len() * 32];
    let mut entries = vec![0; nodes.len() * 24];
    for (i, (nid, parent, pc)) in nodes.iter().enumerate() {
        let id = 12 + i as u64 * 4;
        let at = 12288 + i * 512;
        put64(&mut n, i * 32, *nid as u64);
        put64(&mut n, i * 32 + 8, id);
        put32(&mut n, i * 32 + 24, *parent);
        b[at..at + pc.len()].copy_from_slice(pc);
        let t = at + 488;
        put16(&mut b, t, pc.len() as u16);
        put32(&mut b, t + 4, crc(pc));
        put64(&mut b, t + 8, id);
        put16(&mut b, t + 18, pc.len() as u16);
        put64(&mut entries, i * 24, id);
        put64(&mut entries, i * 24 + 8, at as u64);
        put16(&mut entries, i * 24 + 16, pc.len() as u16);
        put16(&mut entries, i * 24 + 18, 1);
    }
    b[4096..8192].copy_from_slice(&page(&n, nodes.len() as u16, 32, 0x81, 4));
    b[8192..12288].copy_from_slice(&page(&entries, nodes.len() as u16, 24, 0x80, 8));
    seal_header(&mut b);
    b
}
fn store() -> Vec<u8> {
    generated_store(&[(0x122, 0, size_pc(0)), (0x124, 0x122, size_pc(1234))])
}
// Property contexts with arbitrary fixed and variable cells, all generated locally.
fn pc(cells: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut allocations = vec![vec![0xb5, 2, 6, 0, 64, 0, 0, 0], Vec::new()];
    let mut records = Vec::new();
    for (id, kind, value) in cells {
        records.extend_from_slice(&id.to_le_bytes());
        records.extend_from_slice(&kind.to_le_bytes());
        if *kind == 3 {
            records.extend_from_slice(value);
        } else {
            records.extend_from_slice(&(((allocations.len() + 1) as u32) * 32).to_le_bytes());
            allocations.push(value.clone());
        }
    }
    allocations[1] = records;
    heap(0xbc, &allocations)
}
fn heap(kind: u8, allocations: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0; 12];
    bytes[2] = 0xec;
    bytes[3] = kind;
    put32(&mut bytes, 4, 32);
    let mut offsets = Vec::new();
    for a in allocations {
        offsets.push(bytes.len() as u16);
        bytes.extend_from_slice(a);
    }
    offsets.push(bytes.len() as u16);
    let at = bytes.len();
    bytes.extend_from_slice(&(allocations.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&[0, 0]);
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    put16(&mut bytes, 0, at as u16);
    bytes
}
fn unicode(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn generated_v36_inspection_counts_items_and_sizes_read_only() {
    let f = support::Fixture::invalid();
    let bytes = store();
    std::fs::write(&f.source, &bytes).unwrap();
    let result = inspect(&f.source).unwrap();
    assert_eq!(result.support, Support::Supported);
    assert_eq!(result.folders.len(), 1);
    assert_eq!(result.folders[0].id.0, 0x122);
    assert_eq!(result.folders[0].item_count, 1);
    assert_eq!(result.folders[0].bytes, 1234);
    assert!(result.folders[0].unreadable.is_none());
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}
#[test]
fn damaged_index_is_not_a_successful_empty_inspection() {
    let f = support::Fixture::invalid();
    let mut bytes = store();
    bytes[4100] ^= 1;
    std::fs::write(&f.source, &bytes).unwrap();
    assert!(matches!(
        inspect(&f.source).unwrap_err().kind,
        ErrorKind::DamagedSource
    ));
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}
#[test]
fn unreadable_folder_is_visible() {
    let f = support::Fixture::invalid();
    let mut bytes = store();
    bytes[12290] = 0;
    let sum = crc(&bytes[12288..12328]);
    put32(&mut bytes, 12288 + 492, sum);
    std::fs::write(&f.source, &bytes).unwrap();
    let result = inspect(&f.source).unwrap();
    assert!(result.folders[0].unreadable.is_some());
}
#[test]
fn missing_file_is_io_failure() {
    let f = support::Fixture::invalid();
    assert!(matches!(
        inspect(f.root.join("missing")).unwrap_err().kind,
        ErrorKind::Io
    ));
}
#[test]
fn supported_but_truncated_header_is_damage() {
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, &header(36, 0)[..512]).unwrap();
    assert!(matches!(
        inspect(&f.source).unwrap_err().kind,
        ErrorKind::DamagedSource
    ));
}

#[test]
fn compressed_item_block_has_same_observable_result() {
    let f = support::Fixture::invalid();
    let mut bytes = store();
    let compressed: &[u8] = &[
        120, 156, 147, 97, 120, 179, 71, 129, 1, 2, 182, 50, 177, 49, 56, 0, 105, 14, 62, 102, 134,
        75, 44, 12, 12, 76, 64, 54, 15, 131, 8, 131, 12, 195, 240, 5, 0, 223, 75, 4, 15,
    ];
    let at = 12800;
    bytes[at..at + 488].fill(0);
    bytes[at..at + compressed.len()].copy_from_slice(compressed);
    put16(&mut bytes, at + 488, compressed.len() as u16);
    put16(&mut bytes, at + 506, 256);
    put32(&mut bytes, at + 492, crc(compressed));
    put16(&mut bytes, 8192 + 40, compressed.len() as u16);
    let sum = crc(&bytes[8192..8192 + 4072]);
    put32(&mut bytes, 8192 + 4076, sum);
    std::fs::write(&f.source, &bytes).unwrap();
    let result = inspect(&f.source).unwrap();
    assert_eq!(result.folders[0].bytes, 1234);
    assert!(result.folders[0].unreadable.is_none());
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}
#[test]
fn corrupt_item_checksum_marks_folder_unreadable() {
    let f = support::Fixture::invalid();
    let mut bytes = store();
    bytes[12824] ^= 1;
    std::fs::write(&f.source, &bytes).unwrap();
    let result = inspect(&f.source).unwrap();
    assert_eq!(result.folders[0].item_count, 1);
    assert!(result.folders[0].unreadable.is_some());
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}

#[test]
fn all_six_item_classes_and_nested_folders_are_inspected() {
    let mut nodes = vec![
        (0x122, 0, pc(&[(0x3001, 0x1f, unicode("Generated root"))])),
        (
            0x142,
            0x122,
            pc(&[(0x3001, 0x1f, unicode("Generated child"))]),
        ),
    ];
    for (i, class) in [
        "IPM.Note",
        "IPM.Appointment",
        "IPM.Contact",
        "IPM.Task",
        "IPM.StickyNote",
        "IPM.Activity",
    ]
    .iter()
    .enumerate()
    {
        nodes.push((
            0x164 + i as u32 * 32,
            0x142,
            pc(&[
                (0x001a, 0x1f, unicode(class)),
                (0x0e08, 3, ((i + 1) as u32).to_le_bytes().to_vec()),
            ]),
        ));
    }
    let f = support::Fixture::invalid();
    let bytes = generated_store(&nodes);
    std::fs::write(&f.source, &bytes).unwrap();
    let result = inspect(&f.source).unwrap();
    let root = &result.folders[0];
    assert_eq!(root.name, "Generated root");
    assert_eq!(root.item_count, 0);
    assert_eq!(root.children.len(), 1);
    let child = &root.children[0];
    assert_eq!(child.name, "Generated child");
    assert_eq!(child.item_count, 6);
    assert_eq!(child.bytes, 21);
    assert!(child.unreadable.is_none());
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}
#[test]
fn cyclic_folder_hierarchy_is_damage() {
    let f = support::Fixture::invalid();
    let bytes = generated_store(&[(0x122, 0x142, size_pc(0)), (0x142, 0x122, size_pc(0))]);
    std::fs::write(&f.source, &bytes).unwrap();
    assert!(matches!(
        inspect(&f.source).unwrap_err().kind,
        ErrorKind::DamagedSource
    ));
}

fn table() -> Vec<u8> {
    let mut info = vec![0; 30];
    info[0] = 0x7c;
    info[1] = 1;
    put16(&mut info, 2, 8);
    put16(&mut info, 4, 8);
    put16(&mut info, 6, 8);
    put16(&mut info, 8, 9);
    put32(&mut info, 10, 64);
    put32(&mut info, 14, 128);
    put32(&mut info, 22, 0x0e080003);
    put16(&mut info, 26, 4);
    info[28] = 4;
    let mut index = vec![0; 8];
    put32(&mut index, 0, 0x124);
    let mut row = vec![0; 9];
    put32(&mut row, 0, 0x124);
    put32(&mut row, 4, 1234);
    row[8] = 0x80;
    heap(0x7c, &[info, vec![0xb5, 4, 4, 0, 96, 0, 0, 0], index, row])
}
#[test]
fn source_contents_table_is_read_and_damage_is_visible() {
    for bad in [false, true] {
        let mut context = table();
        if bad {
            context[12 + 28] = 1;
        }
        let f = support::Fixture::invalid();
        let bytes = generated_store(&[
            (0x122, 0, size_pc(0)),
            (0x124, 0x122, size_pc(1234)),
            (0x12e, 0, context),
        ]);
        std::fs::write(&f.source, &bytes).unwrap();
        let result = inspect(&f.source).unwrap();
        assert_eq!(result.folders[0].unreadable.is_some(), bad);
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}
#[test]
fn named_property_map_checks_streams_and_preserves_item_readability() {
    for bad in [false, true] {
        let mut entry = vec![0; 8];
        put16(&mut entry, 4, 5); // PS_PUBLIC_STRINGS, string name, property 0x8000.
        let mut strings = vec![0; 4];
        let name = unicode("HeaderItem");
        put32(&mut strings, 0, name.len() as u32);
        strings.extend_from_slice(&name);
        if bad {
            strings.pop();
        }
        let map = pc(&[(2, 0x102, vec![]), (3, 0x102, entry), (4, 0x102, strings)]);
        let item = pc(&[
            (0x0e08, 3, 1234u32.to_le_bytes().to_vec()),
            (0x8000, 3, 1u32.to_le_bytes().to_vec()),
        ]);
        let f = support::Fixture::invalid();
        let bytes =
            generated_store(&[(0x122, 0, size_pc(0)), (0x124, 0x122, item), (0x61, 0, map)]);
        std::fs::write(&f.source, &bytes).unwrap();
        if bad {
            assert!(matches!(
                inspect(&f.source).unwrap_err().kind,
                ErrorKind::DamagedSource
            ));
        } else {
            assert!(inspect(&f.source).unwrap().folders[0].unreadable.is_none());
        }
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}
#[test]
fn recipient_and_attachment_contexts_are_read_and_damage_is_visible() {
    for bad in [false, true] {
        let mut attachment = pc(&[
            (0x3705, 3, 1u32.to_le_bytes().to_vec()),
            (0x3701, 0x102, vec![0; 32]),
        ]);
        if bad {
            attachment[2] = 0;
        }
        let mut subs = vec![0; 56];
        subs[0] = 2;
        put16(&mut subs, 2, 2);
        put64(&mut subs, 8, 0x692);
        put64(&mut subs, 16, 20);
        put64(&mut subs, 32, 0x685);
        put64(&mut subs, 40, 24);
        let f = support::Fixture::invalid();
        let mut bytes = generated_store(&[
            (0x122, 0, size_pc(0)),
            (0x124, 0x122, size_pc(1234)),
            (0x200, 0, table()),
            (0x220, 0, attachment),
            (0x240, 0, subs),
        ]);
        // Set the Item's subnode BID and the subnode block's internal flag everywhere.
        put64(&mut bytes, 4096 + 32 + 16, 30);
        put64(&mut bytes, 4096 + 4 * 32 + 8, 30);
        put64(&mut bytes, 8192 + 4 * 24, 30);
        put64(&mut bytes, 12288 + 4 * 512 + 496, 30);
        let sum = crc(&bytes[4096..4096 + 4072]);
        put32(&mut bytes, 4096 + 4076, sum);
        let sum = crc(&bytes[8192..8192 + 4072]);
        put32(&mut bytes, 8192 + 4076, sum);
        std::fs::write(&f.source, &bytes).unwrap();
        let result = inspect(&f.source).unwrap();
        assert_eq!(result.folders[0].unreadable.is_some(), bad);
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}
#[cfg(windows)]
#[test]
fn windows_writer_sharing_violation_is_in_use() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, store()).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(1)
        .open(&f.source)
        .unwrap();
    assert_eq!(inspect(&f.source).unwrap().support, Support::InUse);
    let options = ConvertOptions::new(f.root.join("absent"), "fixture".into());
    assert!(matches!(
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default())
            .unwrap_err()
            .kind,
        ErrorKind::InUse
    ));
    drop(held);
    assert_eq!(inspect(&f.source).unwrap().support, Support::Supported);
}

#[test]
fn xblock_items_are_read_and_cycles_are_bounded() {
    for cycle in [false, true] {
        let mut xblock = vec![0; 16];
        xblock[0] = 1;
        xblock[1] = if cycle { 2 } else { 1 };
        put16(&mut xblock, 2, 1);
        put32(&mut xblock, 4, 40);
        put64(&mut xblock, 8, if cycle { 22 } else { 16 });
        let f = support::Fixture::invalid();
        let mut bytes = generated_store(&[
            (0x122, 0, size_pc(0)),
            (0x124, 0x122, size_pc(1234)),
            (0x200, 0, xblock),
        ]);
        put64(&mut bytes, 4096 + 32 + 8, 22);
        put64(&mut bytes, 4096 + 2 * 32 + 8, 22);
        put64(&mut bytes, 8192 + 2 * 24, 22);
        put64(&mut bytes, 12288 + 2 * 512 + 496, 22);
        let sum = crc(&bytes[4096..4096 + 4072]);
        put32(&mut bytes, 4096 + 4076, sum);
        let sum = crc(&bytes[8192..8192 + 4072]);
        put32(&mut bytes, 8192 + 4076, sum);
        std::fs::write(&f.source, &bytes).unwrap();
        let result = inspect(&f.source).unwrap();
        assert_eq!(result.folders[0].unreadable.is_some(), cycle);
        if !cycle {
            assert_eq!(result.folders[0].bytes, 1234);
        }
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}
#[test]
fn damaged_header_never_publishes_output() {
    let f = support::Fixture::invalid();
    let mut bytes = store();
    bytes[524] ^= 1;
    std::fs::write(&f.source, &bytes).unwrap();
    assert!(matches!(
        inspect(&f.source).unwrap_err().kind,
        ErrorKind::DamagedSource
    ));
    let options = ConvertOptions::new(f.root.join("absent"), "fixture".into());
    assert!(matches!(
        convert(
            &f.source,
            &options,
            &mut |_| panic!("damaged input made progress"),
            &CancelToken::default()
        )
        .unwrap_err()
        .kind,
        ErrorKind::DamagedSource
    ));
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&f.root).unwrap().count(), 1);
}
#[cfg(unix)]
#[test]
fn non_utf8_source_paths_are_supported() {
    use std::os::unix::ffi::OsStringExt;
    let f = support::Fixture::invalid();
    let path = f.root.join(std::ffi::OsString::from_vec(vec![
        0xff, b'.', b'o', b's', b't',
    ]));
    std::fs::write(&path, store()).unwrap();
    assert_eq!(inspect(&path).unwrap().support, Support::Supported);
}
