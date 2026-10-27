use super::*;
use std::collections::BTreeMap;

pub(super) fn output(path: &Path) -> Output<'static> {
    Output {
        cancel: Default::default(),
        file: File::create(path).unwrap(),
        pos: AMAP + 2048,
        next: 1,
        blocks: spool::Spool::new().unwrap(),
        nodes: &[],
        differences: vec![],
        highwater: 1024,
        planned: cache::Plan::new().unwrap(),
        omitted: Default::default(),
        created: Default::default(),
    }
}

// File-backed generated table fixture: one NBT entry, arbitrarily many rows and
// multilevel BBT/BTH indexes. No whole-fixture byte vector or source mail.
fn emit(path: &Path, table: &mut table::Table) {
    let mut out = output(path);
    let mut heap = context::HeapBuilder::new(0x7c).unwrap();
    let mut subs = vec![];
    let root = table
        .build(&mut heap, |chunks| {
            let nid = ((subs.len() as u32 + 1) << 5) | 0x1f;
            let bid = out.stream_chunks(chunks, true)?;
            subs.push((nid, bid, 0));
            Ok(nid)
        })
        .unwrap();
    let sub = out.subentries(&subs).unwrap();
    let data = out
        .stream_chunks(
            &mut heap.finish(root, &Default::default()).unwrap().iter(),
            true,
        )
        .unwrap();
    let mut entry = vec![0; 32];
    p64(&mut entry, 0, 0x12e);
    p64(&mut entry, 8, data);
    p64(&mut entry, 16, sub);
    finish(out, vec![(0x12e, entry)]);
}
fn finish(out: Output, entries: Vec<(u64, Vec<u8>)>) {
    let mut records = spool::Spool::new().unwrap();
    for (_, entry) in entries {
        records.append(&entry).unwrap();
    }
    finish_records(out, records);
}
pub(super) fn finish_records(mut out: Output, records: spool::Spool) {
    let nbt = out.tree(records, 0x81, 32).unwrap();
    let blocks = std::mem::replace(&mut out.blocks, spool::Spool::new().unwrap());
    let bbt = out.tree(blocks, 0x80, 24).unwrap();
    let eof = out.pos.div_ceil(512) * 512;
    out.file.set_len(eof).unwrap();
    let mut header = vec![0; 564];
    header[..4].copy_from_slice(b"!BDN");
    header[8..10].copy_from_slice(b"SM");
    p16(&mut header, 10, 23);
    p16(&mut header, 12, 19);
    header[14] = 1;
    header[15] = 1;
    p64(&mut header, 32, out.next * 4);
    p64(&mut header, 516, out.next * 4);
    p64(&mut header, 184, eof);
    p64(
        &mut header,
        192,
        AMAP + (eof - AMAP - 1) / INTERVAL * INTERVAL,
    );
    p64(&mut header, 216, nbt.0);
    p64(&mut header, 224, nbt.1);
    p64(&mut header, 232, bbt.0);
    p64(&mut header, 240, bbt.1);
    header[248] = 1;
    header[512] = 0x80;
    header[513] = 1;
    let crc = crc32(&header[8..479]);
    p32(&mut header, 4, crc);
    let crc = crc32(&header[8..524]);
    p32(&mut header, 524, crc);
    out.write(0, &header).unwrap();
    out.file.sync_all().unwrap();
}
fn read(path: &Path, cancel: &crate::CancelToken) -> table::Table {
    let mut pst = Pst::from_file(File::open(path).unwrap()).unwrap();
    let node = pst.nodes()[0];
    table::read(&mut pst, node.bid_data, node.bid_sub, cancel).unwrap()
}

#[test]
fn generated_large_table_has_bounded_traversal_and_emission_and_exact_roundtrip() {
    const COUNT: u32 = 4096;
    let fixture = support::Fixture::invalid();
    let mut table = table::empty(vec![]).unwrap();
    for id in 0x7000..0x7050 {
        table.columns.push((id << 16 | 0x14, 8));
    }
    table.columns.push((0x71000102, 4));
    // The row matrix alone exceeds the local 2 MiB allocation allowance. The
    // former Vec<Row> and whole-matrix builder exceed it by much more.
    for ordinal in 0..COUNT {
        let id = (COUNT - ordinal) * 32 + 4;
        table.add_identity_row(id).unwrap();
    }
    let mut ordinal = 0u32;
    table
        .rows
        .rewrite(|(_, version, cells)| {
            *version = ordinal ^ 0x5a5a0000;
            cells.get_mut(&0x67f30003).unwrap().bytes = version.to_le_bytes().to_vec();
            for id in 0x7000..0x7050 {
                if (id + ordinal).is_multiple_of(3) {
                    continue;
                }
                cells.insert(
                    id << 16 | 0x14,
                    context::Cell {
                        kind: 0x14,
                        bytes: (u64::from(id) << 32 | u64::from(ordinal))
                            .to_le_bytes()
                            .to_vec(),
                        subnode: None,
                    },
                );
            }
            cells.insert(
                0x71000102,
                context::Cell {
                    kind: 0x102,
                    bytes: vec![(ordinal % 251) as u8; 300],
                    subnode: None,
                },
            );
            ordinal += 1;
            Ok(true)
        })
        .unwrap();
    let source = fixture.root.join("large.pst");
    let (_, write_peak) = allocations::measure(|| emit(&source, &mut table));
    assert!(
        write_peak < 2 * 1024 * 1024,
        "table emission allocation delta: {write_peak}"
    );
    drop(table);
    let (mut table, read_peak) = allocations::measure(|| read(&source, &Default::default()));
    assert!(
        read_peak < 2 * 1024 * 1024,
        "table traversal allocation delta: {read_peak}"
    );
    let (_, inspection_peak) = allocations::measure(|| {
        let mut pst = Pst::from_file(File::open(&source).unwrap()).unwrap();
        let node = pst.nodes()[0];
        validate_table(&mut pst, node.bid_data, node.bid_sub).unwrap();
    });
    assert!(
        inspection_peak < 2 * 1024 * 1024,
        "table inspection allocation delta: {inspection_peak}"
    );
    eprintln!("generated table allocation deltas: emission={write_peak}, traversal={read_peak}, inspection={inspection_peak}");
    assert_eq!(table.rows.count, COUNT);
    let mut ordinal = 0;
    table
        .rows
        .visit(|(id, version, cells)| {
            assert_eq!(id, (COUNT - ordinal) * 32 + 4);
            assert_eq!(version, ordinal ^ 0x5a5a0000);
            assert_eq!(cells[&0x71000102].bytes, vec![(ordinal % 251) as u8; 300]);
            for id in 0x7000..0x7050 {
                let tag = id << 16 | 0x14;
                if (id + ordinal).is_multiple_of(3) {
                    assert!(!cells.contains_key(&tag));
                } else {
                    assert_eq!(
                        cells[&tag].bytes,
                        (u64::from(id) << 32 | u64::from(ordinal)).to_le_bytes()
                    );
                }
            }
            ordinal += 1;
            Ok(())
        })
        .unwrap();
    // Independent format-reader path, deliberately outside the allocation gate.
    let mut pst = Pst::from_file(File::open(&source).unwrap()).unwrap();
    let node = pst.nodes()[0];
    let tc = pstoff_format::ltp::read_tc(&mut pst, node.bid_data, node.bid_sub).unwrap();
    assert_eq!(tc.rows.len(), COUNT as usize);
    let cancel = crate::CancelToken::default();
    cancel.cancel();
    assert_eq!(read(&source, &cancel).rows.count, 0);
}

#[test]
fn q16_preserves_versions_and_exact_types_through_row_spool() {
    let fixture = support::Fixture::invalid();
    let mut table = table::empty(vec![0x104]).unwrap();
    table
        .columns
        .extend([(0x70000003, 4), (0x70010003, 4), (0x70020003, 4)]);
    table
        .rows
        .rewrite(|(_, version, cells)| {
            *version = 91;
            cells.get_mut(&0x67f30003).unwrap().bytes = 91u32.to_le_bytes().to_vec();
            for tag in [0x70000003, 0x70010003, 0x70020003] {
                cells.insert(
                    tag,
                    context::Cell {
                        kind: 3,
                        bytes: 1u32.to_le_bytes().to_vec(),
                        subnode: None,
                    },
                );
            }
            Ok(true)
        })
        .unwrap();
    let planned = BTreeMap::from([(
        0x104,
        BTreeMap::from([
            (
                0x7000,
                context::Cell {
                    kind: 3,
                    bytes: 7u32.to_le_bytes().to_vec(),
                    subnode: None,
                },
            ),
            (
                0x7001,
                context::Cell {
                    kind: 0x14,
                    bytes: 8u64.to_le_bytes().to_vec(),
                    subnode: None,
                },
            ),
        ]),
    )]);
    let mut differences = vec![];
    table
        .rebuild_contents(
            |item| Ok(planned.get(&item).unwrap().clone()),
            &[pstoff_format::ndb::Node {
                nid: 0x104,
                nid_parent: 0x122,
                bid_data: 0,
                bid_sub: 0,
            }],
            0x12e,
            &mut differences,
        )
        .unwrap();
    assert_eq!(differences.len(), 3);
    let path = fixture.root.join("q16.pst");
    emit(&path, &mut table);
    read(&path, &Default::default())
        .rows
        .visit(|(_, version, cells)| {
            assert_eq!(version, 91);
            assert_eq!(cells[&0x70000003].bytes, 7u32.to_le_bytes());
            assert!(!cells.contains_key(&0x70010003));
            assert!(!cells.contains_key(&0x70020003));
            assert!(!cells.contains_key(&0x0e300102));
            Ok(())
        })
        .unwrap();
}

#[test]
fn row_rewrite_rolls_back_errors_and_emission_stops_on_cancellation() {
    let mut table = table::empty(vec![4, 36, 68]).unwrap();
    let mut visits = 0;
    assert!(table
        .rows
        .rewrite(|(id, _, _)| {
            visits += 1;
            if visits == 2 {
                return Err("generated failure".into());
            }
            *id = 123;
            Ok(true)
        })
        .is_err());
    let mut ids = vec![];
    table
        .rows
        .visit(|(id, _, _)| {
            ids.push(id);
            Ok(())
        })
        .unwrap();
    assert_eq!(ids, [4, 36, 68]);
    table.columns.push((0x71000102, 4));
    table
        .rows
        .rewrite(|(_, _, cells)| {
            cells.insert(
                0x71000102,
                context::Cell {
                    kind: 0x102,
                    bytes: vec![7; 9000],
                    subnode: None,
                },
            );
            Ok(true)
        })
        .unwrap();
    let cancel = table.rows.cancel.clone();
    let mut heap = context::HeapBuilder::new(0x7c).unwrap();
    let mut calls = 0;
    let root = table
        .build(&mut heap, |chunks| {
            calls += 1;
            assert_eq!(chunks.next().unwrap()?.len(), 9000);
            cancel.cancel();
            Ok(0x3f)
        })
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(root, 0);
}

// File-backed multi-Item/search-table fixture. Generation and independent
// readback are outside planner allocation measurements.
fn planner_source(
    path: &Path,
    items: u32,
    occurrences: u32,
    pcl_entries: u32,
    conflict: Option<u32>,
) {
    let mut out = output(path);
    let mut entries = vec![];
    let mut add = |nid: u32, data: u64, sub: u64| {
        let mut entry = vec![0; 32];
        p64(&mut entry, 0, u64::from(nid));
        p64(&mut entry, 8, data);
        p64(&mut entry, 16, sub);
        p32(&mut entry, 24, 0x122);
        entries.push((u64::from(nid), entry));
    };
    for item in 0..items {
        let mut heap = context::HeapBuilder::new(0xbc).unwrap();
        let root =
            context::pc(&BTreeMap::new(), &mut heap, |_| panic!("empty PC payload")).unwrap();
        let data = out
            .stream_chunks(
                &mut heap.finish(root, &Default::default()).unwrap().iter(),
                true,
            )
            .unwrap();
        add((item + 0x100) * 32 + 4, data, 0);
    }
    let mut key = vec![0; 24];
    key[23] = 1;
    let mut pcl = vec![];
    for i in 0..pcl_entries {
        pcl.push(24);
        let mut xid = [0; 24];
        xid[..4].copy_from_slice(&i.to_be_bytes());
        xid[23] = 1;
        pcl.extend(xid);
    }
    for occurrence in 0..occurrences {
        let mut table = table::empty(vec![]).unwrap();
        for tag in [0x30080040, 0x65e20102, 0x65e30102] {
            table
                .columns
                .push((tag, if tag == 0x30080040 { 8 } else { 4 }));
        }
        for item in 0..items {
            table.add_identity_row((item + 0x100) * 32 + 4).unwrap();
        }
        table
            .rows
            .rewrite(|(_, _, cells)| {
                for (tag, bytes) in [
                    (0x30080040, 42u64.to_le_bytes().to_vec()),
                    (0x65e20102, key.clone()),
                    (0x65e30102, pcl.clone()),
                ] {
                    let mut bytes = bytes;
                    if conflict == Some(tag) && occurrence + 1 == occurrences {
                        *bytes.last_mut().unwrap() = 2;
                    }
                    cells.insert(
                        tag,
                        context::Cell {
                            kind: tag as u16,
                            bytes,
                            subnode: None,
                        },
                    );
                }
                Ok(true)
            })
            .unwrap();
        let mut heap = context::HeapBuilder::new(0x7c).unwrap();
        let mut subs = vec![];
        let root = table
            .build(&mut heap, |chunks| {
                let nid = ((subs.len() as u32 + 1) << 5) | 0x1f;
                let bid = out.stream_chunks(chunks, true)?;
                subs.push((nid, bid, 0));
                Ok(nid)
            })
            .unwrap();
        let sub = out.subentries(&subs).unwrap();
        let data = out
            .stream_chunks(
                &mut heap.finish(root, &Default::default()).unwrap().iter(),
                true,
            )
            .unwrap();
        add((occurrence + 0x200) * 32 + 0x10, data, sub);
    }
    entries.sort_by_key(|e| e.0);
    finish(out, entries);
}
#[test]
fn planner_streams_large_pcl_occurrences_and_rejects_last_occurrence_conflict() {
    use sha2::{Digest, Sha256};
    let fixture = support::Fixture::invalid();
    for conflict in [None, Some(0x65e30102), Some(0x30080040)] {
        planner_source(&fixture.source, 2, 3, 90_000, conflict);
        let before = Sha256::digest(std::fs::read(&fixture.source).unwrap());
        let mut source = Pst::from_file(File::open(&fixture.source).unwrap()).unwrap();
        let nodes = source.nodes();
        let mut differences = vec![];
        let (result, peak) = allocations::measure(|| {
            cache::prepare(
                &mut source,
                &nodes,
                &mut differences,
                &Default::default(),
                &Default::default(),
            )
        });
        assert!(peak < 1024 * 1024, "large-PCL planner allocation {peak}");
        if let Some(tag) = conflict {
            let error = result
                .err()
                .expect("last occurrence conflict accepted")
                .to_string();
            let policy = if tag == 0x65e30102 { "Q15" } else { "Q12" };
            assert!(error.contains(&format!("{policy} conflict")), "{error}");
            if tag == 0x65e30102 {
                assert!(error.contains("65e30102"), "{error}");
            }
        } else {
            let mut plan = result.unwrap().unwrap();
            assert_eq!(differences.len(), 6);
            // Decisions reproduce the complete PCL; no source-local reference
            // is carried into another namespace.
            for node in nodes.iter().filter(|n| n.nid_type() == 4) {
                let mut cells = BTreeMap::new();
                plan.apply(node.nid, &mut cells).unwrap();
                assert_eq!(cells[&0x65e3].bytes.len(), 2_250_000);
                let mut expected = Sha256::new();
                for i in 0u32..90_000 {
                    let mut xid = [0; 25];
                    xid[0] = 24;
                    xid[1..5].copy_from_slice(&i.to_be_bytes());
                    xid[24] = 1;
                    expected.update(xid);
                }
                assert_eq!(Sha256::digest(&cells[&0x65e3].bytes), expected.finalize());
                assert_eq!(cells[&0x3008].bytes, 42u64.to_le_bytes());
                assert!(cells[&0x65e3].subnode.is_none());
            }
        }
        drop(source);
        assert_eq!(
            before,
            Sha256::digest(std::fs::read(&fixture.source).unwrap())
        );
    }
}
#[test]
fn planner_checks_all_search_occurrences_with_growing_item_counts() {
    let fixture = support::Fixture::invalid();
    for items in [16, 128, 1024] {
        planner_source(&fixture.source, items, 32, 1, None);
        let mut source = Pst::from_file(File::open(&fixture.source).unwrap()).unwrap();
        let nodes = source.nodes();
        let mut differences = vec![];
        let (result, peak) = allocations::measure(|| {
            cache::prepare(
                &mut source,
                &nodes,
                &mut differences,
                &Default::default(),
                &Default::default(),
            )
        });
        let mut plan = result.unwrap().unwrap();
        // This scope includes the acknowledged, count-dependent report Vec.
        assert!(
            peak < 2 * 1024 * 1024,
            "{items} Items/32 occurrences allocation {peak}"
        );
        assert_eq!(differences.len(), items as usize * 3);
        for node in nodes.iter().filter(|n| n.nid_type() == 4) {
            let mut cells = BTreeMap::new();
            plan.apply(node.nid, &mut cells).unwrap();
            assert_eq!(cells[&0x65e3].bytes.len(), 25);
            assert_eq!(cells[&0x3008].bytes, 42u64.to_le_bytes());
        }
    }
}
