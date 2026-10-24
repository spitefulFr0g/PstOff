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
