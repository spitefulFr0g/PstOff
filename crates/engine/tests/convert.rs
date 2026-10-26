mod support;
#[allow(dead_code)]
#[path = "support/synthetic.rs"]
mod synthetic;
use pstoff_engine::{convert, inspect, CancelToken, Conversion, ConvertOptions};
use synthetic::{Folder, Item, Store};

fn unsplit_options(destination: std::path::PathBuf, name: String) -> ConvertOptions {
    let mut options = ConvertOptions::new(destination, name);
    options.split = pstoff_engine::Split::Off;
    options
}

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
    let options = unsplit_options(f.root.join("out"), "converted".into());
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
    let options = unsplit_options(f.root.join("out"), "converted".into());
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
    let options = unsplit_options(f.root.join("out"), "converted".into());
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
        let options = unsplit_options(f.root.join("out"), "converted".into());
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
    let options = unsplit_options(f.root.join("out"), "converted".into());
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
    let options = unsplit_options(f.root.join("out"), "cancelled".into());
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
    let options = unsplit_options(f.root.join("out"), "classes".into());
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
    let options = unsplit_options(f.root.join("out"), base.into());
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
    let options = unsplit_options(f.root.join("out"), "new".into());
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
    let mut options = unsplit_options(f.root.join("out"), "too-large".into());
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

fn split_fixture() -> (support::Fixture, Vec<pstoff_engine::FolderRef>, String) {
    use sha2::{Digest, Sha256};
    let mut store = source_store(vec![]);
    let subtree = &mut store.root.children[0].children[1];
    for (name, length, marker) in [("A", 30_000, 1), ("B", 30_000, 2), ("C", 120_000, 3)] {
        let mut item = Item::new("IPM.Note", length);
        item.properties.push(synthetic::Property::binary(0x6800, vec![marker; length as usize]));
        subtree.children.push(Folder { name: name.into(), items: vec![item], ..Default::default() });
    }
    subtree.children.push(Folder { name: "Empty".into(), ..Default::default() });
    let f = support::Fixture::invalid();
    let bytes = store.build();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    std::fs::write(&f.source, bytes).unwrap();
    let inspection = inspect(&f.source).unwrap();
    let folders = inspection.folders[0].children[0].children[1].children[1..4]
        .iter().map(|f| f.id.clone()).collect();
    (f, folders, hash)
}

fn assert_source_hash(f: &support::Fixture, hash: &str) {
    use sha2::{Digest, Sha256};
    assert_eq!(format!("{:x}", Sha256::digest(std::fs::read(&f.source).unwrap())), hash);
}

fn run_split(f: &support::Fixture, base: &str, limit: u64, exclusions: Vec<pstoff_engine::FolderRef>) -> Result<Conversion, pstoff_engine::EngineError> {
    let mut options = ConvertOptions::new(f.root.join(base), base.into());
    options.split = pstoff_engine::Split::Bytes(limit);
    options.exclusions = exclusions;
    convert(&f.source, &options, &mut |_| {}, &CancelToken::default())
}

fn split_report(result: Result<Conversion, pstoff_engine::EngineError>) -> Box<pstoff_engine::ConversionReport> {
    let Conversion::Complete(report) = result.unwrap() else { panic!("cancelled") };
    report
}

#[test]
fn split_uses_finished_file_boundary_including_overhead_and_keeps_whole_items() {
    use outlook_pst::ltp::prop_context::PropertyValue;
    let (f, folders, hash) = split_fixture();
    let first = split_report(run_split(&f, "measure-a", u64::MAX, vec![folders[1].clone(), folders[2].clone()]));
    let second = split_report(run_split(&f, "measure-b", u64::MAX, vec![folders[0].clone(), folders[2].clone()]));
    let limit = first.files[0].bytes.max(second.files[0].bytes);
    assert!(limit > 30_000, "limit must include PST overhead");
    let report = split_report(run_split(&f, "parts", limit, vec![folders[2].clone()]));
    assert_eq!(report.files.len(), 2);
    assert_eq!(report.source_sha256, hash);
    let mut payloads = vec![];
    for file in &report.files {
        assert!(file.bytes <= limit);
        assert_eq!(std::fs::metadata(&file.path).unwrap().len(), file.bytes);
        assert_eq!(file.path.file_name().unwrap().to_str().unwrap(), format!("parts-part{:03}.pst", file.part));
        let store = outlook_pst::open_store(&file.path).unwrap();
        let mut output = pstoff_format::ndb::Pst::open(file.path.to_str().unwrap()).unwrap();
        let nodes = output.nodes();
        let part_folders: Vec<_> = report.folders.iter().filter(|f| f.part == file.part).collect();
        assert_eq!(part_folders.iter().map(|f| f.items).sum::<u64>(), 1);
        assert_eq!(part_folders.len(), 6, "five system folders plus populated folder");
        assert_eq!(nodes.iter().filter(|n| matches!(n.nid_type(), 2 | 3)).count(), 6);
        for folder in part_folders {
            let id = store.properties().make_entry_id(outlook_pst::ndb::node_id::NodeId::from(folder.folder.0)).unwrap();
            let actual = store.open_folder(&id).unwrap();
            assert_eq!(actual.properties().content_count().unwrap() as u64, folder.items);
            assert_eq!(actual.contents_table().unwrap().rows_matrix().count() as u64, folder.items);
        }
        for item in nodes.iter().filter(|n| n.nid_type() == 4) {
            let id = store.properties().make_entry_id(outlook_pst::ndb::node_id::NodeId::from(item.nid)).unwrap();
            let message = store.open_message(&id, None).unwrap();
            let Some(PropertyValue::Binary(value)) = message.properties().get(0x6800) else { panic!("missing payload") };
            assert_eq!(value.buffer().len(), 30_000);
            assert!(value.buffer().iter().all(|b| *b == value.buffer()[0]));
            payloads.push(value.buffer()[0]);
        }
    }
    payloads.sort();
    assert_eq!(payloads, [1, 2]);
    assert_source_hash(&f, &hash);

    let exact = first.files[0].bytes;
    let report = split_report(run_split(&f, "exact", exact, vec![folders[1].clone(), folders[2].clone()]));
    assert_eq!(report.files[0].bytes, exact);
    let error = run_split(&f, "one-byte-under", exact - 1, vec![folders[1].clone(), folders[2].clone()]).unwrap_err();
    assert!(matches!(error.kind, pstoff_engine::ErrorKind::OversizedItem(_)));
    assert!(error.completed_files.is_empty());
    assert_eq!(std::fs::read_dir(f.root.join("one-byte-under")).unwrap().count(), 0);
    assert_source_hash(&f, &hash);
}

#[test]
fn oversized_item_and_cancel_keep_finished_parts_and_clean_unfinished_output() {
    let (f, folders, hash) = split_fixture();
    let measured = split_report(run_split(&f, "measure", u64::MAX, vec![folders[1].clone(), folders[2].clone()]));
    let limit = measured.files[0].bytes;
    let error = run_split(&f, "oversized", limit, vec![]).unwrap_err();
    let pstoff_engine::ErrorKind::OversizedItem(item) = &error.kind else { panic!("{error:?}") };
    assert_eq!(item.folder, folders[2]);
    assert!(error.message.contains(&format!("Item 0x{:x}", item.node)));
    assert!(error.message.contains("Increase the Split limit"));
    assert_eq!(error.completed_files.len(), 2);
    for file in &error.completed_files {
        assert!(file.bytes <= limit);
        outlook_pst::open_store(&file.path).unwrap();
    }
    assert_eq!(std::fs::read_dir(f.root.join("oversized")).unwrap().count(), 2);
    assert_source_hash(&f, &hash);

    let mut options = ConvertOptions::new(f.root.join("cancel-parts"), "cancel".into());
    options.split = pstoff_engine::Split::Bytes(limit);
    let token = CancelToken::default();
    let result = convert(&f.source, &options, &mut |p: pstoff_engine::Progress| {
        if p.part == 2 { token.cancel(); }
    }, &token).unwrap();
    let Conversion::Cancelled { completed_files } = result else { panic!("expected cancellation") };
    assert_eq!(completed_files.len(), 1);
    assert!(completed_files[0].path.exists());
    assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 1);
    assert_source_hash(&f, &hash);
}

#[test]
fn exclusions_cascade_and_update_progress_evidence_and_folder_mapping() {
    let mut store = source_store(vec![Item::new("IPM.Note", 100)]);
    let mut header = Item::new("IPM.Contact", 200);
    header.headers_only = true;
    let mut damaged = Item::new("IPM.Task", 300);
    damaged.unreadable = true;
    store.root.children[0].children[1].children.push(Folder {
        name: "Excluded".into(), items: vec![header],
        children: vec![Folder { name: "Child".into(), items: vec![damaged], ..Default::default() }],
        ..Default::default()
    });
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, store.build()).unwrap();
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(std::fs::read(&f.source).unwrap()));
    let inspection = inspect(&f.source).unwrap();
    let parent = &inspection.folders[0].children[0].children[1].children[1];
    for split in [pstoff_engine::Split::Off, pstoff_engine::Split::default()] {
        let mut options = ConvertOptions::new(f.root.join(format!("exclude-{split:?}")), "selected".into());
        options.split = split;
        options.exclusions = vec![parent.id.clone()];
        let mut events = vec![];
        let Conversion::Complete(report) = convert(&f.source, &options, &mut |p| events.push(p), &CancelToken::default()).unwrap() else { panic!("cancelled") };
        assert!(events.iter().all(|p| p.total == 1));
        assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 1);
        assert_eq!(report.folders.iter().map(|f| f.bytes).sum::<u64>(), 100);
        assert!(report.folders.iter().all(|f| f.folder != parent.id && f.folder != parent.children[0].id));
        assert!(report.skipped.is_empty());
        checked(&report.evidence.headers_only, 0);
        checked(&report.evidence.unreadable, 0);
        let mut output = pstoff_format::ndb::Pst::open(report.files[0].path.to_str().unwrap()).unwrap();
        assert!(output.nodes().iter().all(|n| n.nid != parent.id.0 && n.nid != parent.children[0].id.0));
        assert_source_hash(&f, &hash);
    }
    let report = split_report(run_split(&f, "exclude-all", u64::MAX, vec![inspection.folders[0].id.clone()]));
    assert_eq!(report.folders.len(), 5);
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 0);
    assert_source_hash(&f, &hash);
}

#[test]
fn split_retains_populated_folder_ancestors_and_omits_empty_siblings() {
    let mut store = source_store(vec![]);
    store.root.children[0].children[1].children.push(Folder {
        name: "Ancestor".into(),
        children: vec![
            Folder { name: "Populated".into(), items: vec![Item::new("IPM.Note", 123)], ..Default::default() },
            Folder { name: "Empty sibling".into(), ..Default::default() },
        ],
        ..Default::default()
    });
    let f = support::Fixture::invalid();
    let bytes = store.build();
    std::fs::write(&f.source, &bytes).unwrap();
    let inspection = inspect(&f.source).unwrap();
    let ancestor = &inspection.folders[0].children[0].children[1].children[1];
    let report = split_report(run_split(&f, "ancestors", u64::MAX, vec![]));
    assert_eq!(report.folders.len(), 7);
    for id in [&ancestor.id, &ancestor.children[0].id] {
        assert!(report.folders.iter().any(|f| &f.folder == id));
    }
    assert!(report.folders.iter().all(|f| f.folder != ancestor.children[1].id));
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let id = store.properties().make_entry_id(outlook_pst::ndb::node_id::NodeId::from(ancestor.id.0)).unwrap();
    let actual = store.open_folder(&id).unwrap();
    assert_eq!(actual.hierarchy_table().unwrap().rows_matrix().count(), 1);
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}

#[test]
fn invalid_split_and_unknown_exclusion_publish_nothing_and_preserve_source_hash() {
    let (f, _, hash) = split_fixture();
    for (base, limit, exclusions) in [
        ("zero", 0, vec![]),
        ("unknown-folder", u64::MAX, vec![pstoff_engine::FolderRef(0xffffffe2)]),
    ] {
        let error = run_split(&f, base, limit, exclusions).unwrap_err();
        assert!(matches!(error.kind, pstoff_engine::ErrorKind::InvalidOptions));
        assert!(error.completed_files.is_empty());
        assert!(!f.root.join(base).exists());
        assert_source_hash(&f, &hash);
    }
}

#[test]
fn later_split_name_collision_preserves_finished_parts_and_existing_file() {
    let (f, folders, hash) = split_fixture();
    let measured = split_report(run_split(&f, "measure", u64::MAX, vec![folders[1].clone(), folders[2].clone()]));
    let destination = f.root.join("collision");
    std::fs::create_dir(&destination).unwrap();
    let held = destination.join("collision-part002.pst");
    std::fs::write(&held, b"existing output").unwrap();
    let error = run_split(&f, "collision", measured.files[0].bytes, vec![folders[2].clone()]).unwrap_err();
    assert!(matches!(error.kind, pstoff_engine::ErrorKind::Collision));
    assert_eq!(error.completed_files.len(), 1);
    assert_eq!(std::fs::read(&held).unwrap(), b"existing output");
    outlook_pst::open_store(&error.completed_files[0].path).unwrap();
    assert_eq!(std::fs::read_dir(destination).unwrap().count(), 2);
    assert_source_hash(&f, &hash);
}
