#[path = "support/generated.rs"]
mod generated;
mod support;
use outlook_pst::{
    ndb::{node_id::NodeId, page::NodeBTreeEntry},
    PstFile, UnicodePstFile,
};
use pstoff_engine::{convert, CancelToken, Conversion, ConvertOptions, Split};
#[test]
fn generated_item_roundtrips_through_independent_reader_without_source_changes() {
    let f = support::Fixture::invalid();
    let source = generated::store();
    std::fs::write(&f.source, &source).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "converted".into());
    options.split = Split::Off;
    let result = convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap();
    let Conversion::Complete(report) = result else {
        panic!("unexpected cancellation")
    };
    assert_eq!(report.files.len(), 1);
    let pst = UnicodePstFile::open(&report.files[0].path).unwrap();
    let node = pst.read_node(NodeId::from(0x124u32)).unwrap();
    let bytes = pst.read_block(node.data()).unwrap();
    assert!(bytes.windows(8).any(|w| w == [8, 14, 3, 0, 210, 4, 0, 0]));
    assert_eq!(std::fs::read(&f.source).unwrap(), source);
}

#[test]
fn oversized_heap_with_short_nonfinal_blocks_retains_last_allocation() {
    let f = support::Fixture::invalid();
    let source = generated::skeleton(generated::oversized_pc());
    std::fs::write(&f.source, &source).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "oversized".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let outlook_pst::ltp::prop_context::PropertyValue::Binary(value) =
        store.properties().get(0x4017).unwrap()
    else {
        panic!("not binary")
    };
    assert_eq!(value.buffer(), vec![23; 1000]);
    assert_eq!(std::fs::read(&f.source).unwrap(), source);
}

#[test]
fn store_policy_uses_source_name_and_removes_password() {
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, generated::store()).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "chosen-output".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    assert_eq!(
        store.properties().display_name().unwrap(),
        "PstOff – synthetic"
    );
    assert!(matches!(
        store.properties().get(0x67ff),
        Some(outlook_pst::ltp::prop_context::PropertyValue::Integer32(0))
    ));
    assert_eq!(
        store
            .properties()
            .ipm_sub_tree_entry_id()
            .unwrap()
            .node_id(),
        NodeId::from(0x2062u32)
    );
    assert!(report.differences.iter().any(|d| d.property == 0x3001001f));
}
#[test]
#[ignore = "requires the hash-pinned public fixture staged under target/writer-fixtures"]
fn public_fixture_roundtrip() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/writer-fixtures");
    let source = root.join("example-2013.ost");
    use sha2::{Digest, Sha256};
    let before = std::fs::read(&source).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&before)),
        "b18ff390d0c22273029ee580ab6025976af69a4f84e276d1b1c01a988650853d"
    );
    let out = root.join("output");
    std::fs::create_dir_all(&out).unwrap();
    let mut options = ConvertOptions::new(out, "public".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    assert_eq!(
        store.properties().display_name().unwrap(),
        "PstOff – example-2013"
    );
    verify_public_content(&report.files[0].path);
    assert_eq!(std::fs::read(&source).unwrap(), before);
}

#[test]
fn contents_rows_use_item_properties_and_omit_only_q17_cell() {
    let f = support::Fixture::invalid();
    let mut nodes = generated::skeleton_nodes(generated::pc(&[(0x0ff9, 0x102, vec![1; 16])]));
    nodes.push((0x206e, 0, generated::cached_table()));
    std::fs::write(&f.source, generated::generated_store(&nodes)).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "table".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let id = store
        .properties()
        .make_entry_id(NodeId::from(0x2062u32))
        .unwrap();
    let folder = store.open_folder(&id).unwrap();
    let table = folder.contents_table().unwrap();
    let row = table.rows_matrix().next().unwrap();
    let values = row.columns(table.context()).unwrap();
    for (c, v) in table.context().columns().iter().zip(values) {
        if c.prop_id() == 0x0e08 {
            assert!(matches!(
                table
                    .read_column(v.as_ref().unwrap(), c.prop_type())
                    .unwrap(),
                outlook_pst::ltp::prop_context::PropertyValue::Integer32(1234)
            ));
        }
        if c.prop_id() == 0x6628 {
            assert!(v.is_none());
        }
    }
    assert!(report.differences.iter().any(|d| d.property == 0x66280014));
}

fn verify_public_content(path: &std::path::Path) {
    use outlook_pst::{
        ltp::table_context::TableContext,
        messaging::{
            attachment::{Attachment, AttachmentData, UnicodeAttachment},
            message::{Message, UnicodeMessage},
            store::{Store, UnicodeStore},
        },
    };
    use std::rc::Rc;
    fn read_table(table: &dyn TableContext) -> usize {
        let mut count = 0;
        for row in table.rows_matrix() {
            for (column, value) in table
                .context()
                .columns()
                .iter()
                .zip(row.columns(table.context()).unwrap())
            {
                if let Some(value) = value {
                    table.read_column(&value, column.prop_type()).unwrap();
                }
            }
            count += 1;
        }
        count
    }
    let store = UnicodeStore::read(Rc::new(UnicodePstFile::open(path).unwrap())).unwrap();
    let root = store.root_hierarchy_table().unwrap();
    read_table(root.as_ref());
    let mut folders: Vec<u32> = root.rows_matrix().map(|r| r.id().into()).collect();
    let (mut items, mut recipients, mut attachments, mut binaries, mut embedded) = (0, 0, 0, 0, 0);
    let mut unsupported_subless = 0;
    let ndb = UnicodePstFile::open(path).unwrap();
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = folders.pop() {
        assert!(visited.insert(id));
        let entry = store.properties().make_entry_id(NodeId::from(id)).unwrap();
        let folder = store.open_folder(&entry).unwrap();
        if let Some(table) = folder.hierarchy_table() {
            read_table(table.as_ref());
            folders.extend(table.rows_matrix().map(|r| u32::from(r.id())));
        }
        for table in [folder.contents_table(), folder.associated_table()]
            .into_iter()
            .flatten()
        {
            read_table(table.as_ref());
            for row in table.rows_matrix() {
                let entry = store
                    .properties()
                    .make_entry_id(NodeId::from(u32::from(row.id())))
                    .unwrap();
                items += 1;
                let node = ndb.read_node(NodeId::from(u32::from(row.id()))).unwrap();
                if node.sub_node().is_none() {
                    assert!(UnicodeMessage::read(store.clone(), &entry, None).is_err());
                    unsupported_subless += 1;
                    continue;
                }
                let message = UnicodeMessage::read(store.clone(), &entry, None).unwrap();
                if let Some(table) = message.recipient_table() {
                    recipients += read_table(table.as_ref());
                }
                if let Some(table) = message.attachment_table() {
                    attachments += read_table(table.as_ref());
                    for row in table.rows_matrix() {
                        let attachment = UnicodeAttachment::read(
                            message.clone(),
                            NodeId::from(u32::from(row.id())),
                            None,
                        )
                        .unwrap();
                        match attachment.data() {
                            Some(AttachmentData::Binary(data)) => {
                                assert!(!data.buffer().is_empty());
                                binaries += 1
                            }
                            Some(AttachmentData::Message(message)) => {
                                assert!(message.properties().iter().count() > 0);
                                embedded += 1
                            }
                            None => {}
                        }
                    }
                }
            }
        }
    }
    assert_eq!(items, 17);
    assert!(recipients > 0);
    eprintln!("independent public readback: folders={} items={items} recipients={recipients} attachments={attachments} binary_payloads={binaries} embedded={embedded} unsupported_subless={unsupported_subless}",visited.len());
}

#[test]
fn failed_writer_removes_its_partial_and_preserves_existing_temporary() {
    let f = support::Fixture::invalid();
    let mut bytes = generated::store();
    bytes[12288 + 6 * 512 + 24] ^= 1;
    std::fs::write(&f.source, &bytes).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "broken".into());
    options.split = Split::Off;
    assert!(convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).is_err());
    assert!(!f.root.join("broken.pst").exists());
    assert!(!f.root.join(".broken.pstoff-partial").exists());
    let partial = f.root.join(".broken.pstoff-partial");
    std::fs::write(&partial, b"existing user work").unwrap();
    assert!(convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).is_err());
    assert_eq!(std::fs::read(&partial).unwrap(), b"existing user work");
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}
#[test]
fn oversized_attachment_payload_roundtrips_with_independent_reader() {
    use outlook_pst::messaging::{
        attachment::{Attachment, AttachmentData, UnicodeAttachment},
        message::UnicodeMessage,
        store::{Store, UnicodeStore},
    };
    use std::rc::Rc;
    let f = support::Fixture::invalid();
    let bytes = generated::attachment_store();
    std::fs::write(&f.source, &bytes).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "attachment".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let store = UnicodeStore::read(Rc::new(
        UnicodePstFile::open(&report.files[0].path).unwrap(),
    ))
    .unwrap();
    let entry = store
        .properties()
        .make_entry_id(NodeId::from(0x124u32))
        .unwrap();
    let message = UnicodeMessage::read(store, &entry, None).unwrap();
    let attachment = UnicodeAttachment::read(message, NodeId::from(0x685u32), None).unwrap();
    let Some(AttachmentData::Binary(payload)) = attachment.data() else {
        panic!("missing payload")
    };
    assert_eq!(payload.buffer(), vec![0xa5; 40000]);
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}

// Decode the small generated PC from independently read Microsoft NDB bytes.
fn independent_properties(
    path: &std::path::Path,
    nid: u32,
) -> std::collections::BTreeMap<u16, Vec<u8>> {
    let pst = UnicodePstFile::open(path).unwrap();
    let node = pst.read_node(NodeId::from(nid)).unwrap();
    let bytes = pst.read_block(node.data()).unwrap();
    independent_small_pc(&bytes)
}

// Test-only decoder for single-page, leaf-BTH generated PCs. NDB blocks and
// nested subnode entries come from Microsoft, never the production reader.
fn independent_small_pc(bytes: &[u8]) -> std::collections::BTreeMap<u16, Vec<u8>> {
    independent_typed_small_pc(bytes)
        .into_iter()
        .map(|(id, (_, value))| (id, value))
        .collect()
}
fn independent_typed_small_pc(bytes: &[u8]) -> std::collections::BTreeMap<u16, (u16, Vec<u8>)> {
    let word = |at| u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()) as usize;
    let item = |hid: u32| {
        assert_eq!(hid >> 16, 0);
        let at = word(0) + 4 + ((hid as usize >> 5) - 1) * 2;
        &bytes[word(at)..word(at + 2)]
    };
    let root = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let bth = item(root);
    assert_eq!(&bth[..4], &[0xb5, 2, 6, 0]);
    item(u32::from_le_bytes(bth[4..8].try_into().unwrap()))
        .chunks_exact(8)
        .map(|r| {
            let id = u16::from_le_bytes(r[..2].try_into().unwrap());
            let kind = u16::from_le_bytes(r[2..4].try_into().unwrap());
            let value = if kind == 3 {
                r[4..8].to_vec()
            } else {
                item(u32::from_le_bytes(r[4..8].try_into().unwrap())).to_vec()
            };
            (id, (kind, value))
        })
        .collect()
}

#[test]
fn validated_timestamp_is_added_to_item_and_contents_from_same_plan() {
    let f = support::Fixture::invalid();
    let mut nodes = generated::skeleton_nodes(generated::pc(&[(0x0ff9, 0x102, vec![1; 16])]));
    nodes.push((
        0x206e,
        0,
        generated::policy_table(&[(0x3008, 0x40, 123456789u64.to_le_bytes().to_vec())]),
    ));
    let source = generated::generated_store(&nodes);
    std::fs::write(&f.source, &source).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "timestamp".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let own = independent_properties(&report.files[0].path, 0x124);
    assert_eq!(own.get(&0x3008), Some(&123456789u64.to_le_bytes().to_vec()));
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let entry = store
        .properties()
        .make_entry_id(NodeId::from(0x2062u32))
        .unwrap();
    let folder = store.open_folder(&entry).unwrap();
    let table = folder.contents_table().unwrap();
    let row = table.rows_matrix().next().unwrap();
    let mut found = false;
    for (column, value) in table
        .context()
        .columns()
        .iter()
        .zip(row.columns(table.context()).unwrap())
    {
        if column.prop_id() == 0x3008 {
            assert!(matches!(
                table
                    .read_column(value.as_ref().unwrap(), column.prop_type())
                    .unwrap(),
                outlook_pst::ltp::prop_context::PropertyValue::Time(123456789)
            ));
            found = true;
        }
    }
    assert!(found);
    assert!(report
        .differences
        .iter()
        .any(|d| d.property == 0x30080040 && d.reason.contains("Q12")));
    assert_eq!(std::fs::read(&f.source).unwrap(), source);
}

#[test]
fn validated_change_history_pair_is_promoted_together() {
    let f = support::Fixture::invalid();
    let mut nodes = generated::skeleton_nodes(generated::pc(&[(0x0ff9, 0x102, vec![1; 16])]));
    let mut key = vec![0x11; 16];
    key.extend([0, 0, 0, 0, 0, 7]);
    let mut pcl = vec![22];
    pcl.extend(&key);
    nodes.push((
        0x206e,
        0,
        generated::policy_table(&[
            (0x3008, 0x40, 123456789u64.to_le_bytes().to_vec()),
            (0x65e2, 0x102, key.clone()),
            (0x65e3, 0x102, pcl.clone()),
        ]),
    ));
    std::fs::write(&f.source, generated::generated_store(&nodes)).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "pair".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let own = independent_properties(&report.files[0].path, 0x124);
    assert_eq!(own.get(&0x65e2), Some(&key));
    assert_eq!(own.get(&0x65e3), Some(&pcl));
    assert_eq!(
        report
            .differences
            .iter()
            .filter(|d| d.reason.contains("Q15"))
            .count(),
        2
    );
}

#[test]
fn invalid_cache_identity_cannot_authorize_property_promotion() {
    let f = support::Fixture::invalid();
    let mut nodes = generated::skeleton_nodes(generated::pc(&[(0x0ff9, 0x102, vec![1; 16])]));
    let mut table = generated::policy_table(&[(0x3008, 0x40, 123456789u64.to_le_bytes().to_vec())]);
    // Third allocation is the row index; contradict its row identity.
    let map = u16::from_le_bytes(table[..2].try_into().unwrap()) as usize;
    let index = u16::from_le_bytes(table[map + 8..map + 10].try_into().unwrap()) as usize;
    table[index..index + 4].copy_from_slice(&0x144u32.to_le_bytes());
    nodes.push((0x206e, 0, table));
    let source = generated::generated_store(&nodes);
    std::fs::write(&f.source, &source).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "bad-identity".into());
    options.split = Split::Off;
    assert!(convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).is_err());
    assert!(!f.root.join("bad-identity.pst").exists());
    assert!(!f.root.join(".bad-identity.pstoff-partial").exists());
    assert_eq!(std::fs::read(&f.source).unwrap(), source);
}

#[test]
#[ignore = "pinned Microsoft messaging API omits PtypObject decoding; see docs/writer-verification.md"]
fn embedded_message_high_level_reader_reproduction() {
    use outlook_pst::messaging::{
        attachment::{Attachment, AttachmentData, UnicodeAttachment},
        message::UnicodeMessage,
        store::{Store, UnicodeStore},
    };
    use std::rc::Rc;
    let f = support::Fixture::invalid();
    let source = generated::embedded_store();
    std::fs::write(&f.source, &source).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "embedded".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let store = UnicodeStore::read(Rc::new(
        UnicodePstFile::open(&report.files[0].path).unwrap(),
    ))
    .unwrap();
    let entry = store
        .properties()
        .make_entry_id(NodeId::from(0x124u32))
        .unwrap();
    let message = UnicodeMessage::read(store, &entry, None).unwrap();
    let attachment = UnicodeAttachment::read(message, NodeId::from(0x685u32), None).unwrap();
    let Some(AttachmentData::Message(embedded)) = attachment.data() else {
        panic!("missing embedded message")
    };
    assert_eq!(embedded.properties().message_size().unwrap(), 4321);
    let Some(outlook_pst::ltp::prop_context::PropertyValue::Binary(payload)) =
        embedded.properties().get(0x4000)
    else {
        panic!("missing embedded property")
    };
    assert_eq!(payload.buffer(), vec![0x5a; 64]);
    assert_eq!(std::fs::read(&f.source).unwrap(), source);
}

#[test]
fn change_history_does_not_depend_on_timestamp_cache_column() {
    let f = support::Fixture::invalid();
    let mut nodes = generated::skeleton_nodes(generated::pc(&[(0x0ff9, 0x102, vec![1; 16])]));
    let key = vec![0x11; 22];
    let mut pcl = vec![22];
    pcl.extend(&key);
    nodes.push((
        0x206e,
        0,
        generated::policy_table(&[(0x65e2, 0x102, key.clone()), (0x65e3, 0x102, pcl)]),
    ));
    std::fs::write(&f.source, generated::generated_store(&nodes)).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "pair-only".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    assert_eq!(
        independent_properties(&report.files[0].path, 0x124).get(&0x65e2),
        Some(&key)
    );
}

#[test]
fn invalid_or_conflicting_cache_is_reported_without_publishing() {
    let mut key = vec![0x11; 22];
    key[21] = 7;
    let mut pcl = vec![22];
    pcl.extend(&key);
    for case in 0..12 {
        let f = support::Fixture::invalid();
        let mut cells = vec![
            (0x3008, 0x40, 123456789u64.to_le_bytes().to_vec()),
            (0x65e2, 0x102, key.clone()),
            (0x65e3, 0x102, pcl.clone()),
        ];
        let mut own = vec![(0x0e08, 3, 1234u32.to_le_bytes().to_vec())];
        match case {
            0 => cells[0].2 = 0u64.to_le_bytes().to_vec(),
            1 => cells[0].2 = u64::MAX.to_le_bytes().to_vec(),
            2 => cells[1].2 = vec![1; 16],
            3 => {
                cells[2].2.pop();
            }
            4 => {
                cells[2].2.extend(&pcl);
            }
            5 => cells[2].2[22] = 6,
            6 => cells[2].2[1] = 0x12,
            7 => own.push((0x65e2, 0x1f, vec![0; 22])),
            8 => own.push((0x65e3, 0x102, vec![0; 23])),
            9 => {
                cells.pop();
            }
            _ => {}
        }
        let mut nodes = generated::skeleton_nodes(generated::pc(&[(0x0ff9, 0x102, vec![1; 16])]));
        nodes[6].2 = generated::pc(&own);
        nodes.push((0x206e, 0, generated::policy_table(&cells)));
        if case >= 10 {
            if case == 10 {
                cells[0].2 = 123456790u64.to_le_bytes().to_vec();
            } else {
                cells[1].2[21] = 6;
                cells[2].2[22] = 6;
            }
            nodes.push((0x2090, 0, generated::policy_table(&cells)));
        }
        let source = generated::generated_store(&nodes);
        std::fs::write(&f.source, &source).unwrap();
        let mut options = ConvertOptions::new(f.root.clone(), "conflict".into());
        options.split = Split::Off;
        let error = convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap_err();
        assert!(
            error.message.contains("Q12") || error.message.contains("Q15"),
            "case {case}: {}",
            error.message
        );
        assert!(!f.root.join("conflict.pst").exists());
        assert!(!f.root.join(".conflict.pstoff-partial").exists());
        assert_eq!(std::fs::read(&f.source).unwrap(), source);
    }
}

#[test]
fn existing_timestamp_wins_and_matching_pair_member_is_preserved() {
    for kind in [0x40, 0x1f] {
        let f = support::Fixture::invalid();
        let key = vec![0x11; 22];
        let mut pcl = vec![22];
        pcl.extend(&key);
        let own_time = 987654321u64.to_le_bytes().to_vec();
        let mut nodes = generated::skeleton_nodes(generated::pc(&[(0x0ff9, 0x102, vec![1; 16])]));
        nodes[6].2 = generated::pc(&[
            (0x0e08, 3, 1234u32.to_le_bytes().to_vec()),
            (0x3008, kind, own_time.clone()),
            (0x65e2, 0x102, key.clone()),
        ]);
        let cells = vec![
            (0x3008, 0x40, 123456789u64.to_le_bytes().to_vec()),
            (0x65e2, 0x102, key.clone()),
            (0x65e3, 0x102, pcl.clone()),
        ];
        nodes.push((0x206e, 0, generated::policy_table(&cells)));
        nodes.push((0x2090, 0, generated::policy_table(&cells)));
        let source = generated::generated_store(&nodes);
        std::fs::write(&f.source, &source).unwrap();
        let mut options = ConvertOptions::new(f.root.clone(), "existing".into());
        options.split = Split::Off;
        let Conversion::Complete(report) =
            convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
        else {
            panic!("cancelled")
        };
        let own = independent_properties(&report.files[0].path, 0x124);
        assert_eq!(own.get(&0x3008), Some(&own_time));
        assert_eq!(own.get(&0x65e2), Some(&key));
        assert_eq!(own.get(&0x65e3), Some(&pcl));
        assert!(!report.differences.iter().any(|d| d.reason.contains("Q12")));
        assert_eq!(
            report
                .differences
                .iter()
                .filter(|d| d.reason.contains("Q15"))
                .count(),
            1
        );
        assert_eq!(std::fs::read(&f.source).unwrap(), source);
    }
}

#[test]
fn table_preserves_distinct_types_for_same_property_id() {
    let f = support::Fixture::invalid();
    let mut nodes = generated::skeleton_nodes(generated::pc(&[(0x0ff9, 0x102, vec![1; 16])]));
    nodes.push((
        0x206e,
        0,
        generated::policy_table(&[
            (0x4000, 3, 17u32.to_le_bytes().to_vec()),
            (0x4000, 0x40, 123456789u64.to_le_bytes().to_vec()),
        ]),
    ));
    std::fs::write(&f.source, generated::generated_store(&nodes)).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "typed-cells".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let entry = store
        .properties()
        .make_entry_id(NodeId::from(0x2062u32))
        .unwrap();
    let folder = store.open_folder(&entry).unwrap();
    let table = folder.contents_table().unwrap();
    let row = table.rows_matrix().next().unwrap();
    let mut count = 0;
    for (column, value) in table
        .context()
        .columns()
        .iter()
        .zip(row.columns(table.context()).unwrap())
    {
        if column.prop_id() == 0x4000 {
            let value = table
                .read_column(value.as_ref().expect("lost typed cell"), column.prop_type())
                .unwrap();
            match value {
                outlook_pst::ltp::prop_context::PropertyValue::Integer32(n) => assert_eq!(n, 17),
                outlook_pst::ltp::prop_context::PropertyValue::Time(n) => assert_eq!(n, 123456789),
                _ => panic!("wrong cell type"),
            };
            count += 1;
        }
    }
    assert_eq!(count, 2);
}

fn independent_subnodes(
    pst: &UnicodePstFile,
    bid: outlook_pst::ndb::block_id::UnicodeBlockId,
) -> Vec<outlook_pst::ndb::block::UnicodeLeafSubNodeTreeEntry> {
    use outlook_pst::ndb::{
        block::SubNodeTree, block_id::BlockId, header::Header, page::UnicodeBlockBTree, root::Root,
    };
    let mut reader = pst.reader().lock().unwrap();
    let bbt = UnicodeBlockBTree::read(&mut *reader, *pst.header().root().block_btree()).unwrap();
    let mut cache = Default::default();
    let block = bbt
        .find_entry(&mut *reader, bid.search_key(), &mut cache)
        .unwrap();
    SubNodeTree::<UnicodePstFile>::read(&mut *reader, &block)
        .unwrap()
        .entries(&mut *reader, &bbt, &mut cache)
        .unwrap()
        .collect()
}

#[test]
fn embedded_message_properties_roundtrip_through_independent_ndb_reader() {
    let f = support::Fixture::invalid();
    let source = generated::embedded_store();
    std::fs::write(&f.source, &source).unwrap();
    let mut options = ConvertOptions::new(f.root.clone(), "embedded-ndb".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    verify_embedded_content(&report.files[0].path);
    assert_eq!(std::fs::read(&f.source).unwrap(), source);
}

fn verify_embedded_content(path: &std::path::Path) {
    use outlook_pst::messaging::{
        message::{Message, UnicodeMessage},
        store::{Store, UnicodeStore},
    };
    use std::rc::Rc;
    let pst = UnicodePstFile::open(path).unwrap();
    let store = UnicodeStore::read(Rc::new(UnicodePstFile::open(path).unwrap())).unwrap();
    let entry = store
        .properties()
        .make_entry_id(NodeId::from(0x124u32))
        .unwrap();
    let message = UnicodeMessage::read(store, &entry, None).unwrap();
    let table = message.attachment_table().unwrap();
    let rows: Vec<_> = table.rows_matrix().collect();
    assert_eq!(rows.len(), 1);
    for (column, value) in table
        .context()
        .columns()
        .iter()
        .zip(rows[0].columns(table.context()).unwrap())
    {
        if let Some(value) = value {
            let value = table.read_column(&value, column.prop_type()).unwrap();
            if column.prop_id() == 0x3705 {
                assert!(matches!(
                    value,
                    outlook_pst::ltp::prop_context::PropertyValue::Integer32(5)
                ));
            }
        }
    }
    assert_eq!(u32::from(rows[0].id()), 0x685);
    let node = pst.read_node(NodeId::from(0x124u32)).unwrap();
    let subs = independent_subnodes(&pst, node.sub_node().unwrap());
    let attachment = subs
        .iter()
        .find(|s| s.node() == NodeId::from(0x685u32))
        .unwrap();
    let bytes = pst.read_block(attachment.block()).unwrap();
    let typed = independent_typed_small_pc(&bytes);
    assert_eq!(typed[&0x3701].0, 0x000d);
    let props = independent_small_pc(&bytes);
    assert_eq!(props[&0x3705], 5u32.to_le_bytes());
    let object = &props[&0x3701];
    assert_eq!(object.len(), 8);
    let embedded_id = NodeId::from(u32::from_le_bytes(object[..4].try_into().unwrap()));
    let attachment_subs = independent_subnodes(&pst, attachment.sub_node().unwrap());
    let embedded = attachment_subs
        .iter()
        .find(|s| s.node() == embedded_id)
        .unwrap();
    let props = independent_small_pc(&pst.read_block(embedded.block()).unwrap());
    assert_eq!(props[&0x0e08], 4321u32.to_le_bytes());
    assert_eq!(props[&0x4000], vec![0x5a; 64]);
    let nested = independent_subnodes(&pst, embedded.sub_node().unwrap());
    assert_eq!(nested.len(), 1);
    assert_eq!(pst.read_block(nested[0].block()).unwrap(), vec![0x7b; 16]);
}

#[test]
#[ignore = "writes persistent generated acceptance files under target/writer-fixtures"]
fn generated_embedded_acceptance_artifact() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/writer-fixtures/embedded");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.ost");
    let source = generated::embedded_store();
    std::fs::write(&path, &source).unwrap();
    let mut options = ConvertOptions::new(root, "embedded".into());
    options.split = Split::Off;
    let Conversion::Complete(report) =
        convert(&path, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    verify_embedded_content(&report.files[0].path);
    assert_eq!(std::fs::read(path).unwrap(), source);
}
