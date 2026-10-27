use super::*;
use std::io::Read;
pub(super) mod allocations;
mod indexes;
#[path = "../../tests/support/mod.rs"]
pub(super) mod support;
#[allow(dead_code)]
#[path = "../../tests/support/synthetic.rs"]
pub(super) mod synthetic;
mod tables;

#[test]
fn source_payload_relocation_is_selective_ordered_and_cancellable() {
    let fixture = support::Fixture::invalid();
    let payload: Vec<_> = (0..180_013)
        .map(|i| ((i * 37 + i / 251) % 251) as u8)
        .collect();
    let mut item = synthetic::Item::new("IPM.Note", 1);
    item.properties
        .push(synthetic::Property::binary(0x6801, payload.clone()));
    let bytes = synthetic::Store {
        root: synthetic::Folder {
            name: "Generated".into(),
            items: vec![item],
            ..Default::default()
        },
    }
    .build();
    std::fs::write(&fixture.source, &bytes).unwrap();
    let mut source = Pst::from_file(File::open(&fixture.source).unwrap()).unwrap();
    let node = source
        .nodes()
        .into_iter()
        .find(|n| n.nid_type() == 4)
        .unwrap();
    let cancel = crate::CancelToken::default();
    let cells =
        context::relocation_properties(&mut source, node.bid_data, node.bid_sub, &cancel).unwrap();
    assert!(cells[&0x6801].bytes.is_empty());
    let local = cells[&0x6801].subnode.unwrap();
    let store_cells =
        context::store_properties(&mut source, node.bid_data, node.bid_sub, 32, &cancel).unwrap();
    assert!(store_cells[&0x6801].bytes.is_empty());
    assert_eq!(store_cells[&0x6801].subnode, Some(local));

    let selected = context::selected_properties(
        &mut source,
        node.bid_data,
        node.bid_sub,
        &[0x68010102].into_iter().collect(),
        &cancel,
    )
    .unwrap();
    assert_eq!(selected[&0x6801].bytes, payload);
    assert_eq!(selected[&0x6801].subnode, Some(local));
    let bid = source.subnodes(node.bid_sub).unwrap()[&local].data;
    let mut output = Output {
        cancel: cancel.clone(),
        file: File::create(fixture.root.join("blocks.bin")).unwrap(),
        pos: AMAP + 2048,
        next: 1,
        blocks: spool::Spool::new().unwrap(),
        nodes: &crate::nodes::Nodes::new().unwrap(),
        differences: crate::Records::default(),
        highwater: 1024,
        planned: cache::Plan::new().unwrap(),
        omitted: &Default::default(),
        created: Default::default(),
    };
    let root = output.source_stream(&mut source, bid, false).unwrap();
    let mut readback = Vec::new();
    let mut file = File::open(fixture.root.join("blocks.bin")).unwrap();
    for offset in (0..output.blocks.len).step_by(24) {
        let mut entry = [0; 24];
        output.blocks.read(offset, &mut entry).unwrap();
        let id = u64::from_le_bytes(entry[..8].try_into().unwrap());
        let at = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        let len = u16::from_le_bytes(entry[16..18].try_into().unwrap()) as usize;
        file.seek(SeekFrom::Start(at)).unwrap();
        let mut bytes = vec![0; len];
        file.read_exact(&mut bytes).unwrap();
        if id & 2 == 0 {
            pstoff_format::crypt::permute_decode(&mut bytes);
            readback.extend_from_slice(&bytes);
        } else if id == root {
            assert_eq!(bytes[1], 1);
            assert_eq!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize,
                payload.len()
            );
        }
    }
    assert_eq!(readback, payload);
    let count = output.blocks.len;
    let len = output.file.metadata().unwrap().len();
    cancel.cancel();
    assert!(
        context::relocation_properties(&mut source, node.bid_data, node.bid_sub, &cancel)
            .unwrap()
            .is_empty()
    );
    assert_eq!(output.source_stream(&mut source, bid, false).unwrap(), 0);
    assert_eq!(output.blocks.len, count);
    assert_eq!(output.file.metadata().unwrap().len(), len);
    assert_eq!(std::fs::read(&fixture.source).unwrap(), bytes);
}

#[test]
fn generated_large_output_indexes_are_bounded_and_reopen_with_exact_payloads() {
    const COUNT: u32 = 50_000;
    let fixture = support::Fixture::invalid();
    let path = fixture.root.join("many-small-blocks.pst");
    let (_, peak) = allocations::measure(|| {
        let nodes = crate::nodes::Nodes::new().unwrap();
        let omitted = crate::nodes::IdSet::default();
        let mut out = tables::output(&path, &nodes, &omitted);
        let mut entries = spool::Spool::new().unwrap();
        let mut order = spool::Index::new().unwrap();
        for ordinal in (0..COUNT).rev() {
            let bid = out.block(&ordinal.to_le_bytes(), false).unwrap();
            let mut entry = [0; 32];
            p64(&mut entry, 0, u64::from(ordinal) * 32 + 4);
            p64(&mut entry, 8, bid);
            let offset = entries.append(&entry).unwrap();
            order.insert(ordinal * 32 + 4, offset).unwrap();
        }
        let entries = ordered_nodes(entries, order, &Default::default()).unwrap();
        tables::finish_records(out, entries);
    });
    eprintln!("writer indexes entries={COUNT}, allocation delta={peak}");
    assert!(peak < 128 * 1024, "writer index allocation delta {peak}");
    // Exercise the disk-backed finalizer on the same multilevel indexes.
    let mut header = [0; 564];
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    file.read_exact(&mut header).unwrap();
    header[248] = 0;
    let crc = crc32(&header[8..479]);
    p32(&mut header, 4, crc);
    let crc = crc32(&header[8..524]);
    p32(&mut header, 524, crc);
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(&header).unwrap();
    drop(file);
    let (result, finalizer_peak) = allocations::measure(|| {
        UnicodePstFile::open(&path)
            .unwrap()
            .finalize_with_cancel(&|| false)
    });
    result.unwrap();
    eprintln!("large-index finalizer allocation delta={finalizer_peak}");
    assert!(finalizer_peak < 128 * 1024);
    {
        use outlook_pst::PstFile;
        let reader = UnicodePstFile::open(&path).unwrap();
        for ordinal in [0, COUNT - 1] {
            let node = reader.read_node((ordinal * 32 + 4).into()).unwrap();
            use outlook_pst::ndb::page::NodeBTreeEntry;
            assert_eq!(
                reader.read_block(node.data()).unwrap(),
                ordinal.to_le_bytes()
            );
        }
    }
    let mut pst = Pst::from_file(File::open(&path).unwrap()).unwrap();
    let nodes = pst.nodes();
    assert_eq!(nodes.len(), COUNT as usize);
    for (ordinal, node) in nodes.iter().enumerate() {
        assert_eq!(node.nid, ordinal as u32 * 32 + 4);
        assert_eq!(
            pst.node_chunks(node.bid_data)
                .unwrap()
                .next()
                .unwrap()
                .unwrap(),
            (ordinal as u32).to_le_bytes()
        );
    }
}

#[test]
fn output_tree_rejects_truncation_and_cancellation_and_partial_is_removed() {
    let fixture = support::Fixture::invalid();
    let path = fixture.root.join("partial-index.pst");
    {
        let _partial = Partial {
            path: &path,
            keep: false,
        };
        let nodes = crate::nodes::Nodes::new().unwrap();
        let omitted = crate::nodes::IdSet::default();
        let mut out = tables::output(&path, &nodes, &omitted);
        let mut records = spool::Spool::new().unwrap();
        records.append(&[0; 31]).unwrap();
        assert!(out.tree(records, 0x81, 32).is_err());
        let mut records = spool::Spool::new().unwrap();
        records.append(&[0; 32]).unwrap();
        out.cancel.cancel();
        let before = out.file.metadata().unwrap().len();
        assert!(out.tree(records, 0x81, 32).is_err());
        assert_eq!(out.file.metadata().unwrap().len(), before);
    }
    assert!(!path.exists());
}

#[test]
fn inspection_and_recovery_discard_large_unselected_properties() {
    for len in [64_000, 700_000] {
        let fixture = support::Fixture::invalid();
        let mut item = synthetic::Item::new("IPM.Note", 123);
        item.properties
            .push(synthetic::Property::binary(0x6801, vec![0x5a; len]));
        let mut store = synthetic::Store {
            root: synthetic::Folder {
                name: "Generated".into(),
                role: Some(synthetic::SystemFolder::IpmSubtree),
                items: vec![item],
                ..Default::default()
            },
        };
        store.root.children.push(synthetic::Folder {
            name: "Generated".into(),
            role: Some(synthetic::SystemFolder::Wastebasket),
            ..Default::default()
        });
        let bytes = synthetic::Store {
            root: synthetic::Folder {
                children: vec![
                    store.root,
                    synthetic::Folder {
                        name: "Generated".into(),
                        role: Some(synthetic::SystemFolder::Finder),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        }
        .build();
        std::fs::write(&fixture.source, &bytes).unwrap();
        let (mut source, nodes) = crate::reader::read_source(&fixture.source).unwrap();
        let item = nodes.iter().find(|n| n.nid_type() == 4).unwrap().unwrap();
        let (pc, peak) =
            allocations::measure(|| crate::reader::item_properties(&mut source, &item).unwrap());
        assert!(!pc.props.contains_key(&0x6801));
        assert!(
            peak < 256 * 1024,
            "unselected payload {len}, allocation delta {peak}"
        );
        let full = pstoff_format::ltp::read_node_pc(&mut source, &item).unwrap();
        assert!(
            matches!(&full.props[&0x6801], pstoff_format::ltp::Value::Bytes(b) if b.len() == len && b.iter().all(|v| *v == 0x5a))
        );
        let recovery = recovery::prepare(&mut source, nodes, &Default::default()).unwrap();
        assert!(recovery.omitted.is_empty());
        let path = fixture.root.join("readback.pst");
        write(
            &mut source,
            &recovery,
            &path,
            "Generated",
            &mut |_| {},
            &Default::default(),
        )
        .unwrap()
        .unwrap();
        let mut readback = Pst::from_file(File::open(path).unwrap()).unwrap();
        let item = readback
            .nodes()
            .into_iter()
            .find(|n| n.nid_type() == 4)
            .unwrap();
        let pc = pstoff_format::ltp::read_node_pc(&mut readback, &item).unwrap();
        assert!(
            matches!(&pc.props[&0x6801], pstoff_format::ltp::Value::Bytes(b) if b.len() == len && b.iter().all(|v| *v == 0x5a))
        );
        assert_eq!(std::fs::read(&fixture.source).unwrap(), bytes);
    }
}

#[test]
fn growing_generated_folder_item_counts_keep_selection_identity_and_source_bytes() {
    for count in [1, 8, 16] {
        let fixture = support::Fixture::invalid();
        let children = (0..count)
            .map(|_| synthetic::Folder {
                name: "Generated".into(),
                items: vec![synthetic::Item::new("IPM.Note", 19)],
                ..Default::default()
            })
            .collect();
        let mut store = synthetic::Store {
            root: synthetic::Folder {
                name: "Generated".into(),
                role: Some(synthetic::SystemFolder::IpmSubtree),
                children,
                ..Default::default()
            },
        };
        store.root.children.push(synthetic::Folder {
            name: "Generated".into(),
            role: Some(synthetic::SystemFolder::Wastebasket),
            ..Default::default()
        });
        let bytes = synthetic::Store {
            root: synthetic::Folder {
                children: vec![
                    store.root,
                    synthetic::Folder {
                        name: "Generated".into(),
                        role: Some(synthetic::SystemFolder::Finder),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        }
        .build();
        std::fs::write(&fixture.source, &bytes).unwrap();
        let (mut source, nodes) = crate::reader::read_source(&fixture.source).unwrap();
        let original: Vec<_> = nodes
            .iter()
            .filter(|n| n.nid_type() == 4)
            .map(|n| {
                let n = n.unwrap();
                (n.nid, n.nid_parent, n.bid_data, n.bid_sub)
            })
            .collect();
        let inspection =
            crate::reader::inspect_nodes(&fixture.source, &mut source, &nodes).unwrap();
        let mut pending: Vec<_> = inspection.folders.iter().collect();
        let mut items = 0;
        while let Some(folder) = pending.pop() {
            assert!(folder.unreadable.is_none());
            items += folder.item_count;
            pending.extend(&folder.children);
        }
        assert_eq!(items, count);
        let recovery = recovery::prepare(&mut source, nodes, &Default::default()).unwrap();
        let selection =
            crate::planner::Selection::new(recovery, Default::default(), &Default::default())
                .unwrap();
        for i in 0..selection.items.len() {
            let part = selection.part(i..i + 1, true, &Default::default()).unwrap();
            let retained: Vec<_> = part
                .nodes
                .iter()
                .filter(|n| n.nid_type() == 4)
                .map(Result::unwrap)
                .collect();
            assert_eq!(retained.len(), 1);
            assert!(original.contains(&(
                retained[0].nid,
                retained[0].nid_parent,
                retained[0].bid_data,
                retained[0].bid_sub
            )));
            assert!(part.skipped.is_empty());
        }
        assert_eq!(std::fs::read(&fixture.source).unwrap(), bytes);
    }
}

#[test]
fn high_count_node_ownership_stays_on_disk_through_selection_and_parts() {
    use crate::nodes::{IdSet, Nodes};
    let cancel = crate::CancelToken::default();
    let ((), peak) = allocations::measure(|| {
        let mut nodes = Nodes::new().unwrap();
        for nid in [0x122, 0x222, 0x242, 0x262] {
            nodes
                .push(Node {
                    nid,
                    nid_parent: 0x122,
                    bid_data: 0,
                    bid_sub: 0,
                })
                .unwrap();
        }
        // Reverse input makes disk ordering observable without an in-memory expected vector.
        for i in (0..25_000u32).rev() {
            nodes
                .push(Node {
                    nid: 0x10004 + i * 32,
                    nid_parent: 0x222,
                    bid_data: u64::from(i) * 4,
                    bid_sub: u64::from(i) * 8,
                })
                .unwrap();
        }
        let recovery = recovery::Recovery {
            nodes,
            system: system::SystemFolders {
                subtree: 0x222,
                deleted: 0x242,
                finder: 0x262,
            },
            created: Default::default(),
            omitted: IdSet::default(),
            skipped: Default::default(),
            evidence: crate::CompletenessEvidence {
                headers_only: crate::Evidence::NotChecked,
                unreadable: crate::Evidence::NotChecked,
                external_references: crate::Evidence::NotChecked,
            },
        };
        let selection =
            crate::planner::Selection::new(recovery, IdSet::default(), &cancel).unwrap();
        assert_eq!(selection.items.len(), 25_000);
        for (i, n) in selection.items.iter().enumerate() {
            let n = n.unwrap();
            assert_eq!(n.nid, 0x10004 + i as u32 * 32);
            assert_eq!((n.bid_data, n.bid_sub), (i as u64 * 4, i as u64 * 8));
        }
        let part = selection.part(24_993..25_000, true, &cancel).unwrap();
        assert_eq!(
            part.nodes
                .iter()
                .filter(|n| n.nid_type() == 4)
                .count()
                .unwrap(),
            7
        );
        assert_eq!(part.omitted.len(), 24_993);
        assert_eq!(selection.recovery.nodes.len(), 25_004);
        assert_eq!(
            selection.recovery.nodes.get(4).unwrap().unwrap().nid,
            0x10004 + 24_999 * 32
        );
    });
    assert!(peak < 256 * 1024, "node ownership allocated {peak} bytes");
}
