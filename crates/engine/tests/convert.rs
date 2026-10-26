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
    source_store(items).build()
}
fn source_store(items: Vec<Item>) -> Store {
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
    assert!(text.contains("Headers-only Items: 0"));
    assert!(text.contains("Unreadable content: 0"));
    assert!(text.contains("External attachment references: 0"));
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

fn complete(bytes: &[u8], base: &str) -> (support::Fixture, pstoff_engine::ConversionReport) {
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, bytes).unwrap();
    let options = ConvertOptions::new(f.root.join("out"), base.into());
    let Conversion::Complete(report) =
        convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
    else {
        panic!("cancelled")
    };
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    (f, *report)
}
fn checked(e: &pstoff_engine::Evidence, expected: usize) {
    let pstoff_engine::Evidence::Checked { count, references } = e else {
        panic!("{e:?}")
    };
    assert_eq!(*count as usize, expected);
    assert_eq!(references.len(), expected);
}
#[test]
fn unreadable_item_is_skipped_and_contents_counts_are_repaired() {
    let mut damaged = Item::new("IPM.Note", 100);
    damaged.unreadable = true;
    let bytes = source_items(vec![damaged, Item::new("IPM.Contact", 200)]);
    let (_f, report) = complete(&bytes, "item-recovery");
    assert_eq!(report.skipped.len(), 1);
    assert!(matches!(
        report.skipped[0].code,
        pstoff_engine::SkipReason::UnreadableItem
    ));
    let pstoff_engine::LocalRef::Item(item) = &report.skipped[0].reference else {
        panic!("item expected")
    };
    checked(&report.evidence.unreadable, 1);
    assert!(
        matches!(&report.evidence.headers_only, pstoff_engine::Evidence::Incomplete { count: 0, unchecked, .. } if unchecked == &vec![item.clone()])
    );
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 1);
    assert_eq!(report.folders.iter().map(|f| f.bytes).sum::<u64>(), 200);
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let id = store
        .properties()
        .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(item.folder.0))
        .unwrap();
    let folder = store.open_folder(&id).unwrap();
    assert_eq!(folder.properties().content_count().unwrap(), 1);
    assert_eq!(folder.contents_table().unwrap().rows_matrix().count(), 1);
}
#[test]
fn unreadable_folder_skips_its_subtree_and_preserves_healthy_siblings() {
    let mut store = source_store(vec![Item::new("IPM.Note", 90)]);
    let subtree = &mut store.root.children[0].children[1];
    subtree.children.push(Folder {
        name: "Damaged".into(),
        unreadable: true,
        items: vec![Item::new("IPM.Note", 800)],
        children: vec![Folder {
            name: "Descendant".into(),
            items: vec![Item::new("IPM.Task", 900)],
            ..Default::default()
        }],
        ..Default::default()
    });
    let bytes = store.build();
    let (_f, report) = complete(&bytes, "folder-recovery");
    assert_eq!(report.skipped.len(), 4); // Two folders, two Items, each with a local reason.
    assert_eq!(report.folders.len(), 5);
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 1);
    checked(&report.evidence.unreadable, 2);
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    for part in &report.folders {
        let id = store
            .properties()
            .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(part.folder.0))
            .unwrap();
        let folder = store.open_folder(&id).unwrap();
        assert_eq!(
            folder.contents_table().unwrap().rows_matrix().count() as u64,
            part.items
        );
        // Deleted Items is the only remaining child of IPM_SUBTREE.
        if folder.properties().display_name().unwrap() == "IPM_SUBTREE" {
            assert_eq!(folder.hierarchy_table().unwrap().rows_matrix().count(), 1);
        }
    }
}
#[test]
fn validated_markers_and_external_refs_are_separate_from_unreadable_content() {
    let mut headers = Item::new("IPM.Note", 10);
    headers.headers_only = true;
    headers.attachments = vec![synthetic::Attachment::External(
        "https://example.invalid/attachment".into(),
    )];
    let mut unrelated = Item::new("IPM.Note", 20);
    unrelated
        .properties
        .push(synthetic::Property::int(0x8001, 1));
    let (_f, report) = complete(&source_items(vec![headers, unrelated]), "evidence");
    checked(&report.evidence.headers_only, 1);
    checked(&report.evidence.external_references, 1);
    checked(&report.evidence.unreadable, 0);
    assert!(report.skipped.is_empty());
}
#[test]
fn invalid_marker_type_or_external_path_is_not_reported_as_checked_zero() {
    let mut item = Item::new("IPM.Note", 10);
    item.properties.push(synthetic::Property::text(0x8000, "1"));
    item.attachments
        .push(synthetic::Attachment::External("".into()));
    let (f, report) = complete(&source_items(vec![item]), "invalid-evidence");
    for evidence in [
        &report.evidence.headers_only,
        &report.evidence.external_references,
    ] {
        assert!(
            matches!(evidence, pstoff_engine::Evidence::Incomplete { count: 0, unchecked, .. } if unchecked.len() == 1)
        );
    }
    checked(&report.evidence.unreadable, 0);
    assert!(report.skipped.is_empty());
    let text = std::fs::read_to_string(f.root.join("out/invalid-evidence.txt")).unwrap();
    assert!(text.contains("Not checked for 1 Items"));
    assert!(text.contains("not missing content"));
}
#[test]
fn leftover_staging_for_another_base_is_flagged_without_touching_it() {
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, source()).unwrap();
    let options = ConvertOptions::new(f.root.join("out"), "new".into());
    let stale = options.destination.join(".old.pstoff-partial");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(stale.join("output.pst"), b"synthetic unfinished output").unwrap();
    let e = convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap_err();
    assert!(matches!(e.kind, pstoff_engine::ErrorKind::Collision));
    assert!(e.message.contains("stale or active"));
    assert!(e.message.contains(".old.pstoff-partial"));
    assert_eq!(
        std::fs::read(stale.join("output.pst")).unwrap(),
        b"synthetic unfinished output"
    );
    assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 1);
}
#[test]
fn free_space_preflight_rejects_large_declared_output_before_staging() {
    let f = support::Fixture::invalid();
    let bytes = source();
    std::fs::write(&f.source, &bytes).unwrap();
    let mut options = ConvertOptions::new(f.root.join("out"), "too-large".into());
    options.space_reserve_bytes = u64::MAX;
    let e = convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap_err();
    assert!(
        matches!(e.kind, pstoff_engine::ErrorKind::InsufficientSpace),
        "{e:?}"
    );
    assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 0);
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}

#[test]
fn damaged_nested_attachment_is_an_item_skip_not_a_store_failure() {
    let mut inner = Item::new("IPM.Note", 10);
    inner.unreadable = true;
    let mut outer = Item::new("IPM.Note", 100);
    outer
        .attachments
        .push(synthetic::Attachment::Embedded(Box::new(inner)));
    let (_f, report) = complete(
        &source_items(vec![outer, Item::new("IPM.Task", 20)]),
        "nested-damage",
    );
    assert_eq!(report.skipped.len(), 1);
    checked(&report.evidence.unreadable, 1);
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 1);
}
#[test]
fn headers_only_uses_named_property_identity_not_a_fixed_property_id() {
    let mut item = Item::new("IPM.Note", 10);
    item.properties.push(synthetic::Property::int(0x8123, 1));
    let store = source_store(vec![item]);
    let guid = [8, 0x20, 6, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0x46];
    let (_f, report) = complete(
        &store.build_with_header_identity(0x123, guid, 0x8578),
        "remapped",
    );
    checked(&report.evidence.headers_only, 1);
    let (_f, report) = complete(
        &store.build_with_header_identity(0x123, [0x42; 16], 0x8578),
        "unrelated-guid",
    );
    checked(&report.evidence.headers_only, 0);
    let (_f, report) = complete(
        &store.build_with_header_identity(0x123, guid, 0x8579),
        "unrelated-lid",
    );
    checked(&report.evidence.headers_only, 0);
}
#[test]
fn unreadable_required_folder_or_table_gets_empty_output_scaffolding() {
    for table in [false, true] {
        let mut store = source_store(vec![Item::new("IPM.Note", 100)]);
        let subtree = &mut store.root.children[0].children[1];
        subtree.unreadable = !table;
        subtree.unreadable_table = table;
        let (_f, report) = complete(&store.build(), "required-folder");
        checked(&report.evidence.unreadable, 1);
        assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 0);
        assert!(report
            .differences
            .iter()
            .any(|d| d.reason.contains("scaffolding")));
        let pst = outlook_pst::open_store(&report.files[0].path).unwrap();
        for folder in &report.folders {
            let id = pst
                .properties()
                .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(folder.folder.0))
                .unwrap();
            let actual = pst.open_folder(&id).unwrap();
            assert_eq!(actual.contents_table().unwrap().rows_matrix().count(), 0);
            actual.hierarchy_table().unwrap();
        }
    }
}

#[test]
fn skipped_global_item_ids_do_not_remove_local_recipient_rows() {
    let mut damaged = Item::new("IPM.Note", 10);
    damaged.unreadable = true;
    let (_, first) = complete(
        &source_items(vec![damaged, Item::new("IPM.Note", 20)]),
        "ids",
    );
    let pstoff_engine::LocalRef::Item(skipped) = &first.skipped[0].reference else {
        panic!("Item expected")
    };
    let mut damaged = Item::new("IPM.Note", 10);
    damaged.unreadable = true;
    let mut healthy = Item::new("IPM.Note", 20);
    healthy.recipient_row_ids.push(skipped.node);
    healthy.recipients.push(synthetic::Recipient {
        name: "Synthetic".into(),
        address: "fixture@example.invalid".into(),
        kind: 1,
    });
    let (_f, report) = complete(&source_items(vec![damaged, healthy]), "local-ids");
    let mut pst = pstoff_format::ndb::Pst::open(report.files[0].path.to_str().unwrap()).unwrap();
    let item = pst.nodes().into_iter().find(|n| n.nid_type() == 4).unwrap();
    let recipient = pst.subnodes(item.bid_sub).unwrap()[&0x692];
    let table = pstoff_format::ltp::read_tc(&mut pst, recipient.data, recipient.sub).unwrap();
    assert_eq!(table.rows.len(), 1);
}
