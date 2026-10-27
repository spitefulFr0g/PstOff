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
        nodes: &[],
        differences: vec![],
        highwater: 1024,
        planned: cache::Plan::new().unwrap(),
        omitted: Default::default(),
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
        let mut out = tables::output(&path);
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
        let mut out = tables::output(&path);
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
