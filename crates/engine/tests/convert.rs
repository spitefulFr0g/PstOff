mod support;
#[allow(dead_code)]
#[path = "support/synthetic.rs"]
mod synthetic;
use pstoff_engine::{convert, inspect, CancelToken, Conversion, ConvertOptions};
use synthetic::{Folder, Item, Store};

fn source() -> Vec<u8> {
    source_items(vec![Item::new("IPM.Note", 1234)])
}

fn source_items(items: Vec<Item>) -> Vec<u8> {
    Store {
        root: Folder {
            name: "Root".into(),
            children: vec![Folder {
                name: "Root - Mailbox".into(),
                children: vec![
                    Folder {
                        name: "Finder".into(),
                        ..Default::default()
                    },
                    Folder {
                        name: "IPM_SUBTREE".into(),
                        items,
                        children: vec![Folder {
                            name: "Deleted Items".into(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        },
    }
    .build()
}

#[test]
fn converts_synthetic_ost_to_readable_pst_without_modifying_source() {
    let f = support::Fixture::invalid();
    let bytes = source();
    std::fs::write(&f.source, &bytes).unwrap();
    let inspection = inspect(&f.source).unwrap();
    let options = ConvertOptions::new(f.root.join("out"), "converted".into());
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    assert_eq!(store.properties().display_name().unwrap(), "synthetic");
    assert_eq!(report.folders.len(), 5);
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 1);
    let mut pending: Vec<_> = inspection.folders.iter().collect();
    while let Some(expected) = pending.pop() {
        let id = store
            .properties()
            .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(expected.id.0))
            .unwrap();
        let actual = store.open_folder(&id).unwrap();
        assert_eq!(actual.properties().display_name().unwrap(), expected.name);
        assert_eq!(
            actual.properties().content_count().unwrap() as u64,
            expected.item_count
        );
        assert_eq!(
            actual.contents_table().unwrap().rows_matrix().count() as u64,
            expected.item_count
        );
        assert_eq!(
            actual.hierarchy_table().unwrap().rows_matrix().count(),
            expected.children.len()
        );
        pending.extend(&expected.children);
    }
    let header = std::fs::read(&report.files[0].path).unwrap();
    assert_eq!(&header[8..12], &[b'S', b'M', 23, 0]);
    assert_eq!(header[513], 1);
    assert!(matches!(
        store.properties().get(0x67ff),
        Some(outlook_pst::ltp::prop_context::PropertyValue::Integer32(0))
    ));
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}
#[test]
fn saves_reports_with_source_identity_counts_and_completeness_explanation() {
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, source()).unwrap();
    let options = ConvertOptions::new(f.root.join("out"), "converted".into());
    convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap();
    let json = std::fs::read_to_string(options.destination.join("converted.json")).unwrap();
    let text = std::fs::read_to_string(options.destination.join("converted.txt")).unwrap();
    let saved: pstoff_engine::ConversionReport = serde_json::from_str(&json).unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(saved.source, f.source);
    assert_eq!(
        saved.source_sha256,
        format!("{:x}", Sha256::digest(source()))
    );
    assert_eq!(
        saved.files[0].bytes,
        std::fs::metadata(&saved.files[0].path).unwrap().len()
    );
    assert_eq!(saved.folders.len(), 5);
    assert_eq!(saved.folders.iter().map(|f| f.items).sum::<u64>(), 1);
    assert_eq!(saved.folders.iter().map(|f| f.bytes).sum::<u64>(), 1234);
    for report in [&json, &text] {
        assert!(report.contains("Mailbox completeness: unknown"));
        assert!(report.contains("synced"));
        assert!(report.contains("source_sha256") || report.contains("SHA-256"));
    }
    assert!(text.contains("Headers-only Items: Not checked"));
    assert!(text.contains("Unreadable content: Not checked"));
    assert!(text.contains("External attachment references: Not checked"));
    assert!(text.contains("part 1"));
    assert!(text.contains("Property 0x3001001f"));
    assert!(text.contains("Source store and system folder hierarchy"));
    assert!(text.contains("PST display name comes from the source file name."));
}
#[test]
fn final_name_is_absent_until_publication_and_cancellation_cleans_staging() {
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, source()).unwrap();
    let options = ConvertOptions::new(f.root.join("out"), "converted".into());
    let token = CancelToken::default();
    let mut observed = false;
    let result = convert(
        &f.source,
        &options,
        &mut |p: pstoff_engine::Progress| {
            assert!(p.fraction < 1.0);
            assert!(!options.destination.join("converted.pst").exists());
            observed = true;
            token.cancel();
        },
        &token,
    )
    .unwrap();
    assert!(observed);
    assert!(
        matches!(result, Conversion::Cancelled { completed_files } if completed_files.is_empty())
    );
    assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 0);
}

#[test]
fn existing_pst_reports_and_staging_are_never_overwritten() {
    for name in [
        "converted.pst",
        "converted.json",
        "converted.txt",
        ".converted.pstoff-partial",
    ] {
        let f = support::Fixture::invalid();
        std::fs::write(&f.source, source()).unwrap();
        let options = ConvertOptions::new(f.root.join("out"), "converted".into());
        std::fs::create_dir(&options.destination).unwrap();
        let held = options.destination.join(name);
        std::fs::write(&held, b"keep").unwrap();
        let error = convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap_err();
        assert!(matches!(error.kind, pstoff_engine::ErrorKind::Collision));
        assert_eq!(std::fs::read(&held).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 1);
    }
}

#[test]
fn a_destination_created_during_conversion_is_preserved_and_reports_are_rolled_back() {
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, source()).unwrap();
    let options = ConvertOptions::new(f.root.join("out"), "converted".into());
    let final_path = options.destination.join("converted.pst");
    let error = convert(
        &f.source,
        &options,
        &mut |p: pstoff_engine::Progress| {
            if p.fraction == 0.95 {
                assert!(!final_path.exists());
                std::fs::write(&final_path, b"other caller").unwrap();
            }
        },
        &CancelToken::default(),
    )
    .unwrap_err();
    assert!(matches!(error.kind, pstoff_engine::ErrorKind::Collision));
    assert_eq!(std::fs::read(&final_path).unwrap(), b"other caller");
    assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 1);
}

#[test]
fn cancellation_between_items_reports_progress_and_removes_partial_output() {
    let f = support::Fixture::invalid();
    let mut item = Item::new("IPM.Note", 100);
    item.properties
        .push(synthetic::Property::binary(0x6800, vec![0x42; 20000]));
    let bytes = source_items(vec![
        item,
        Item::new("IPM.Contact", 200),
        Item::new("IPM.Task", 300),
    ]);
    std::fs::write(&f.source, &bytes).unwrap();
    let inspection = inspect(&f.source).unwrap();
    let folder = inspection.folders[0].children[0]
        .children
        .iter()
        .find(|f| f.item_count == 3)
        .unwrap()
        .id
        .clone();
    let options = ConvertOptions::new(f.root.join("out"), "cancelled".into());
    let token = CancelToken::default();
    let mut events = vec![];
    let result = convert(
        &f.source,
        &options,
        &mut |p: pstoff_engine::Progress| {
            assert!(!options.destination.join("cancelled.pst").exists());
            if p.done == 1 {
                assert_eq!(p.current_folder, Some(folder.clone()));
                assert_eq!(p.total, 3);
                assert!(p.fraction > 0.0 && p.fraction < 0.95);
                // Cancellation occurs after one whole Item, before the next Item.
                token.cancel();
            }
            events.push(p);
        },
        &token,
    )
    .unwrap();
    assert!(
        matches!(result, Conversion::Cancelled { completed_files } if completed_files.is_empty())
    );
    assert_eq!(events.last().unwrap().done, 1);
    assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 0);
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}

#[test]
fn all_item_classes_and_short_non_final_streams_round_trip_with_valid_totals() {
    use outlook_pst::ltp::prop_context::PropertyValue;
    use pstoff_format::ndb::Pst;
    let f = support::Fixture::invalid();
    let classes = [
        "IPM.Note",
        "IPM.Appointment",
        "IPM.Contact",
        "IPM.Task",
        "IPM.StickyNote",
        "IPM.Activity",
    ];
    // A heap-signature payload with short non-final source blocks exercises
    // relocation without relying on the writer's heap allocation strategy.
    let mut payload = vec![0x23; 9000];
    payload[2] = 0xec;
    payload[3] = 0x6c;
    let items = classes
        .iter()
        .map(|class| {
            let mut item = Item::new(class, 9000);
            item.properties
                .push(synthetic::Property::binary(0x6800, payload.clone()));
            item
        })
        .collect();
    let bytes = source_items(items);
    std::fs::write(&f.source, &bytes).unwrap();
    let options = ConvertOptions::new(f.root.join("out"), "classes".into());
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    let mut output = Pst::open(report.files[0].path.to_str().unwrap()).unwrap();
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let mut actual = vec![];
    for item in output.nodes().into_iter().filter(|n| n.nid_type() == 4) {
        let id = store
            .properties()
            .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(item.nid))
            .unwrap();
        let message = store.open_message(&id, None).unwrap();
        actual.push(message.properties().message_class().unwrap());
        let Some(PropertyValue::Binary(value)) = message.properties().get(0x6800) else {
            panic!("missing binary payload")
        };
        assert_eq!(value.buffer(), payload);
    }
    actual.sort();
    let mut expected = classes.map(str::to_string);
    expected.sort();
    assert_eq!(actual, expected);
    // Validate the serialized MS-PST data-tree length invariant. Both bundled
    // readers tolerate incorrect cbTotal, so property read-back alone misses it.
    let mut short_non_final = 0;
    for block in output.blocks() {
        if block.bid & 2 == 0 {
            continue;
        }
        let data = output.block(block.bid).unwrap();
        if data.get(..2) != Some(&[1, 1]) {
            continue;
        }
        let count = u16::from_le_bytes(data[2..4].try_into().unwrap()) as usize;
        let mut total = 0;
        for (i, entry) in data[8..8 + count * 8].chunks_exact(8).enumerate() {
            let child = u64::from_le_bytes(entry.try_into().unwrap());
            let length = output.block(child).unwrap().len();
            if i + 1 < count && length < 8176 {
                short_non_final += 1;
            }
            total += length as u32;
        }
        assert_eq!(
            u32::from_le_bytes(data[4..8].try_into().unwrap()),
            total,
            "XBLOCK cbTotal must cover actual child bytes"
        );
    }
    assert!(short_non_final >= 6);
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 6);
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}
