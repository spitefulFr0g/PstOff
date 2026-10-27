// Tests that reach disk preflight require Windows. Ignore them on other
// platforms so they remain visible and compile under Clippy --all-targets.
// Collision/stale-staging and invalid-option rejections below run before
// preflight and stay enabled on every platform.
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
                        role: Some(synthetic::SystemFolder::Finder),
                        ..Default::default()
                    },
                    Folder {
                        name: "IPM_SUBTREE".into(),
                        role: Some(synthetic::SystemFolder::IpmSubtree),
                        items,
                        children: vec![Folder {
                            name: "Deleted Items".into(),
                            role: Some(synthetic::SystemFolder::Wastebasket),
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

fn folder_entry(id: u16, nid: u32) -> synthetic::Property {
    let mut bytes = vec![0; 4];
    bytes.extend_from_slice(&[0x23; 16]);
    bytes.extend_from_slice(&nid.to_le_bytes());
    synthetic::Property::binary(id, bytes)
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn localized_system_folder_entry_ids_retain_source_hierarchy_and_payload() {
    let payload: Vec<_> = (0..20_000).map(|i| (i * 37 % 251) as u8).collect();
    let mut item = Item::new("IPM.Note", 1234);
    item.properties
        .push(synthetic::Property::binary(0x6800, payload.clone()));
    let mut input = source_store(vec![item]);
    input.root.name = "Wurzel".into();
    let mailbox = &mut input.root.children[0];
    mailbox.name = "Persönlicher Speicher".into();
    mailbox.children[0].name = "Recherche personnalisée".into();
    mailbox.children[1].name = "Mes dossiers".into();
    mailbox.children[1].children[0].name = "Gelöschte Objekte".into();
    // IDs follow source traversal: root, mailbox, finder, subtree, Item, deleted.
    let bytes = input.build_with_store_properties(&[
        folder_entry(0x35e0, 0x182),
        folder_entry(0x35e3, 0x1c2),
        folder_entry(0x35e7, 0x162),
    ]);
    let (f, report) = complete(&bytes, "localized-identity");
    let output = outlook_pst::open_store(&report.files[0].path).unwrap();
    for (tag, nid) in [(0x35e0, 0x182), (0x35e3, 0x1c2), (0x35e7, 0x162)] {
        let Some(outlook_pst::ltp::prop_context::PropertyValue::Binary(value)) =
            output.properties().get(tag)
        else {
            panic!("missing store folder entry ID")
        };
        assert_eq!(value.buffer(), folder_entry(tag, nid).bytes);
    }
    assert_eq!(report.folders.len(), 5);
    assert!(!report
        .differences
        .iter()
        .any(|d| d.reason.contains("scaffolding")));
    let id = output
        .properties()
        .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(0x182u32))
        .unwrap();
    let subtree = output.open_folder(&id).unwrap();
    assert_eq!(subtree.properties().display_name().unwrap(), "Mes dossiers");
    let rows = independent_rows(subtree.contents_table().unwrap().as_ref());
    assert_eq!(rows.iter().map(|row| row.0).collect::<Vec<_>>(), [0x1a4]);
    assert_eq!(subtree.hierarchy_table().unwrap().rows_matrix().count(), 1);
    let id = output.properties().make_entry_id(0x1a4u32.into()).unwrap();
    let message = output.open_message(&id, None).unwrap();
    let Some(outlook_pst::ltp::prop_context::PropertyValue::Binary(value)) =
        message.properties().get(0x6800)
    else {
        panic!("missing synthetic payload")
    };
    assert_eq!(value.buffer(), payload);
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn source_folder_identity_overrides_english_decoys_in_split_and_exclusions() {
    let mut input = source_store(vec![Item::new("IPM.Note", 100)]);
    input.root.children[0].name = "Custom mailbox".into();
    input.root.children[0].children[0].name = "Custom search".into();
    input.root.children[0].children[1].name = "Custom subtree".into();
    input.root.children[0].children[1].children[0].name = "Custom wastebasket".into();
    input.root.children[0].children.push(Folder {
        name: "IPM_SUBTREE".into(),
        children: vec![Folder {
            name: "Deleted Items".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    input.root.children[0].children.push(Folder {
        name: "IPM_SUBTREE".into(),
        ..Default::default()
    });
    let bytes = input.build_with_store_properties(&[
        folder_entry(0x35e0, 0x182),
        folder_entry(0x35e3, 0x1c2),
        folder_entry(0x35e7, 0x162),
    ]);
    for excluded in [false, true] {
        let f = support::Fixture::invalid();
        std::fs::write(&f.source, &bytes).unwrap();
        let mut options = ConvertOptions::new(f.root.join("out"), "identity".into());
        options.split = pstoff_engine::Split::Bytes(u64::MAX);
        if excluded {
            options.exclusions.push(pstoff_engine::FolderRef(0x182));
        }
        let Conversion::Complete(report) =
            convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap()
        else {
            panic!("cancelled")
        };
        let report = Box::new(report.collect_report().unwrap());
        let output = outlook_pst::open_store(&report.files[0].path).unwrap();
        assert_eq!(
            report.folders.len(),
            5,
            "only resolved system roles and their ancestors"
        );
        assert!(!report
            .differences
            .iter()
            .any(|d| d.reason.contains("scaffolding")));
        let Some(outlook_pst::ltp::prop_context::PropertyValue::Binary(entry)) =
            output.properties().get(0x35e0)
        else {
            panic!("missing subtree entry ID")
        };
        assert_eq!(entry.buffer(), folder_entry(0x35e0, 0x182).bytes);
        let id = output.properties().make_entry_id(0x182u32.into()).unwrap();
        let subtree = output.open_folder(&id).unwrap();
        assert_eq!(
            subtree.properties().display_name().unwrap(),
            "Custom subtree"
        );
        assert_eq!(
            subtree.contents_table().unwrap().rows_matrix().count(),
            usize::from(!excluded)
        );
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn source_identity_supports_an_ipm_subtree_directly_under_root() {
    let input = Store {
        root: Folder {
            name: "Custom root".into(),
            children: vec![
                Folder {
                    name: "Custom IPM".into(),
                    items: vec![Item::new("IPM.Note", 100)],
                    children: vec![Folder {
                        name: "Custom deleted".into(),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                Folder {
                    name: "Custom search".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };
    let bytes = input.build_with_store_properties(&[
        folder_entry(0x35e0, 0x142),
        folder_entry(0x35e3, 0x182),
        folder_entry(0x35e7, 0x1a2),
    ]);
    let (_f, report) = complete(&bytes, "direct-root");
    assert_eq!(report.folders.len(), 4);
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 1);
    assert!(!report
        .differences
        .iter()
        .any(|d| d.reason.contains("scaffolding")));
    let output = outlook_pst::open_store(&report.files[0].path).unwrap();
    let id = output.properties().make_entry_id(0x142u32.into()).unwrap();
    assert_eq!(
        output
            .open_folder(&id)
            .unwrap()
            .contents_table()
            .unwrap()
            .rows_matrix()
            .count(),
        1
    );
}

#[test]
fn unidentified_healthy_system_folders_fail_without_creating_scaffolding() {
    for variant in 0..6 {
        let mut input = source_store(vec![Item::new("IPM.Note", 100)]);
        if variant == 1 {
            input.root.children[0].name = "Custom mailbox".into();
        }
        if variant == 2 {
            input.root.children[0].children.push(Folder {
                name: "Unreadable sibling".into(),
                unreadable: true,
                ..Default::default()
            });
        }
        if variant < 3 {
            input.root.children[0].children[1].name = "Custom subtree".into();
        }
        if variant == 3 {
            input.root.children[0].children[1].children[0].name = "Custom deleted".into();
        }
        if variant == 4 {
            input.root.children[0].children[0].name = "Custom finder".into();
        }
        let properties = match variant {
            3 => vec![folder_entry(0x35e0, 0x182), folder_entry(0x35e7, 0x162)],
            4 => vec![folder_entry(0x35e0, 0x182), folder_entry(0x35e3, 0x1c2)],
            _ => vec![], // Even canonical English names cannot establish identity.
        };
        let bytes = input.build_with_store_properties(&properties);
        let f = support::Fixture::invalid();
        std::fs::write(&f.source, &bytes).unwrap();
        let options = unsplit_options(f.root.join("out"), "unidentified".into());
        let error = convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap_err();
        assert!(matches!(
            error.kind,
            pstoff_engine::ErrorKind::DamagedSource
        ));
        assert!(error.message.contains("Cannot identify the source"));
        assert!(error.completed_files.is_empty());
        assert!(!options.destination.exists());
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}

#[test]
fn invalid_source_system_entry_ids_do_not_fall_back_to_english_names() {
    let mut variants = Vec::new();
    let entry = folder_entry(0x35e0, 0x182);
    let mut flags = entry.clone();
    flags.bytes[0] = 1;
    variants.push(flags);
    let mut provider = entry.clone();
    provider.bytes[4] ^= 1;
    variants.push(provider);
    let mut short = entry.clone();
    short.bytes.pop();
    variants.push(short);
    let mut kind = entry.clone();
    kind.kind = 0x1f;
    variants.push(kind);
    variants.extend([
        folder_entry(0x35e0, 0x1a4),      // Item, not a folder.
        folder_entry(0x35e0, 0xffffffe2), // Missing folder.
        folder_entry(0x35e0, 0x122),      // Root cannot be its own subtree.
        folder_entry(0x35e3, 0x162),      // Finder is outside the IPM subtree.
        folder_entry(0x35e7, 0x1c2),      // Finder cannot be a child of IPM.
    ]);
    for invalid in variants {
        let mut properties = vec![
            folder_entry(0x35e0, 0x182),
            folder_entry(0x35e3, 0x1c2),
            folder_entry(0x35e7, 0x162),
        ];
        properties.retain(|p| p.id != invalid.id);
        properties.push(invalid);
        let bytes =
            source_store(vec![Item::new("IPM.Note", 100)]).build_with_store_properties(&properties);
        let f = support::Fixture::invalid();
        std::fs::write(&f.source, &bytes).unwrap();
        let options = unsplit_options(f.root.join("out"), "invalid-identity".into());
        let error = convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap_err();
        assert!(matches!(
            error.kind,
            pstoff_engine::ErrorKind::DamagedSource
        ));
        assert!(error.message.contains("system folder"));
        assert!(error.completed_files.is_empty());
        assert!(!options.destination.exists());
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn identified_unreadable_localized_subtree_gets_reported_empty_scaffolding() {
    let mut input = source_store(vec![Item::new("IPM.Note", 100)]);
    input.root.children[0].name = "Custom mailbox".into();
    input.root.children[0].children[0].name = "Custom search".into();
    let subtree = &mut input.root.children[0].children[1];
    subtree.name = "Custom IPM".into();
    subtree.unreadable = true;
    let bytes = input.build_with_store_properties(&[
        folder_entry(0x35e0, 0x182),
        folder_entry(0x35e3, 0x1c2),
        folder_entry(0x35e7, 0x162),
    ]);
    let (_f, report) = complete(&bytes, "unreadable-identity");
    checked(&report.evidence.unreadable, 1);
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 0);
    assert!(report.skipped.iter().any(|s| matches!(&s.reference,
        pstoff_engine::LocalRef::Folder(folder) if folder.0 == 0x182)));
    assert!(report
        .differences
        .iter()
        .any(|d| d.reason.contains("scaffolding")));
    let output = outlook_pst::open_store(&report.files[0].path).unwrap();
    let Some(outlook_pst::ltp::prop_context::PropertyValue::Binary(entry)) =
        output.properties().get(0x35e0)
    else {
        panic!("missing replacement subtree entry ID")
    };
    let replacement = u32::from_le_bytes(entry.buffer()[20..24].try_into().unwrap());
    assert_ne!(replacement, 0x182);
    let id = output
        .properties()
        .make_entry_id(replacement.into())
        .unwrap();
    assert_eq!(
        output
            .open_folder(&id)
            .unwrap()
            .contents_table()
            .unwrap()
            .rows_matrix()
            .count(),
        0
    );
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
    let report = Box::new(report.collect_report().unwrap());
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
    let report = Box::new(report.collect_report().unwrap());
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
        for (i, entry) in data[8..8 + count * 8].as_chunks::<8>().0.iter().enumerate() {
            let child = u64::from_le_bytes(*entry);
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
    let report = Box::new(report.collect_report().unwrap());
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
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
        item.properties.push(synthetic::Property::binary(
            0x6800,
            vec![marker; length as usize],
        ));
        subtree.children.push(Folder {
            name: name.into(),
            items: vec![item],
            ..Default::default()
        });
    }
    subtree.children.push(Folder {
        name: "Empty".into(),
        ..Default::default()
    });
    let f = support::Fixture::invalid();
    let bytes = store.build();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    std::fs::write(&f.source, bytes).unwrap();
    let inspection = inspect(&f.source).unwrap();
    let folders = inspection.folders[0].children[0].children[1].children[1..4]
        .iter()
        .map(|f| f.id.clone())
        .collect();
    (f, folders, hash)
}

fn assert_source_hash(f: &support::Fixture, hash: &str) {
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&f.source).unwrap())),
        hash
    );
}

fn run_split(
    f: &support::Fixture,
    base: &str,
    limit: u64,
    exclusions: Vec<pstoff_engine::FolderRef>,
) -> Result<Conversion, pstoff_engine::EngineError> {
    let mut options = ConvertOptions::new(f.root.join(base), base.into());
    options.split = pstoff_engine::Split::Bytes(limit);
    options.exclusions = exclusions;
    convert(&f.source, &options, &mut |_| {}, &CancelToken::default())
}

fn split_report(
    result: Result<Conversion, pstoff_engine::EngineError>,
) -> Box<pstoff_engine::ConversionReport> {
    let Conversion::Complete(report) = result.unwrap() else {
        panic!("cancelled")
    };
    Box::new(report.collect_report().unwrap())
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn split_uses_finished_file_boundary_including_overhead_and_keeps_whole_items() {
    use outlook_pst::ltp::prop_context::PropertyValue;
    let (f, folders, hash) = split_fixture();
    let first = split_report(run_split(
        &f,
        "measure-a",
        u64::MAX,
        vec![folders[1].clone(), folders[2].clone()],
    ));
    let second = split_report(run_split(
        &f,
        "measure-b",
        u64::MAX,
        vec![folders[0].clone(), folders[2].clone()],
    ));
    let limit = first.files[0].bytes.max(second.files[0].bytes);
    assert!(limit > 30_000, "limit must include PST overhead");
    let report = split_report(run_split(&f, "parts", limit, vec![folders[2].clone()]));
    assert_eq!(report.files.len(), 2);
    assert_eq!(report.source_sha256, hash);
    let mut payloads = vec![];
    for file in &report.files {
        assert!(file.bytes <= limit);
        assert_eq!(std::fs::metadata(&file.path).unwrap().len(), file.bytes);
        assert_eq!(
            file.path.file_name().unwrap().to_str().unwrap(),
            format!("parts-part{:03}.pst", file.part)
        );
        let store = outlook_pst::open_store(&file.path).unwrap();
        let mut output = pstoff_format::ndb::Pst::open(file.path.to_str().unwrap()).unwrap();
        let nodes = output.nodes();
        let part_folders: Vec<_> = report
            .folders
            .iter()
            .filter(|f| f.part == file.part)
            .collect();
        assert_eq!(part_folders.iter().map(|f| f.items).sum::<u64>(), 1);
        assert_eq!(
            part_folders.len(),
            6,
            "five system folders plus populated folder"
        );
        assert_eq!(
            nodes
                .iter()
                .filter(|n| matches!(n.nid_type(), 2 | 3))
                .count(),
            6
        );
        for folder in part_folders {
            let id = store
                .properties()
                .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(folder.folder.0))
                .unwrap();
            let actual = store.open_folder(&id).unwrap();
            assert_eq!(
                actual.properties().content_count().unwrap() as u64,
                folder.items
            );
            assert_eq!(
                actual.contents_table().unwrap().rows_matrix().count() as u64,
                folder.items
            );
        }
        for item in nodes.iter().filter(|n| n.nid_type() == 4) {
            let id = store
                .properties()
                .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(item.nid))
                .unwrap();
            let message = store.open_message(&id, None).unwrap();
            let Some(PropertyValue::Binary(value)) = message.properties().get(0x6800) else {
                panic!("missing payload")
            };
            assert_eq!(value.buffer().len(), 30_000);
            assert!(value.buffer().iter().all(|b| *b == value.buffer()[0]));
            payloads.push(value.buffer()[0]);
        }
    }
    payloads.sort();
    assert_eq!(payloads, [1, 2]);
    assert_source_hash(&f, &hash);

    let exact = first.files[0].bytes;
    let report = split_report(run_split(
        &f,
        "exact",
        exact,
        vec![folders[1].clone(), folders[2].clone()],
    ));
    assert_eq!(report.files[0].bytes, exact);
    let error = run_split(
        &f,
        "one-byte-under",
        exact - 1,
        vec![folders[1].clone(), folders[2].clone()],
    )
    .unwrap_err();
    assert!(matches!(
        error.kind,
        pstoff_engine::ErrorKind::OversizedItem(_)
    ));
    assert!(error.completed_files.is_empty());
    assert_eq!(
        std::fs::read_dir(f.root.join("one-byte-under"))
            .unwrap()
            .count(),
        0
    );
    assert_source_hash(&f, &hash);
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn oversized_item_and_cancel_keep_finished_parts_and_clean_unfinished_output() {
    let (f, folders, hash) = split_fixture();
    let measured = split_report(run_split(
        &f,
        "measure",
        u64::MAX,
        vec![folders[1].clone(), folders[2].clone()],
    ));
    let limit = measured.files[0].bytes;
    let error = run_split(&f, "oversized", limit, vec![]).unwrap_err();
    let pstoff_engine::ErrorKind::OversizedItem(item) = &error.kind else {
        panic!("{error:?}")
    };
    assert_eq!(item.folder, folders[2]);
    assert!(error.message.contains(&format!("Item 0x{:x}", item.node)));
    assert!(error.message.contains("Increase the Split limit"));
    assert_eq!(error.completed_files.len(), 2);
    for file in error.completed_files.iter() {
        let file = file.unwrap();
        assert!(file.bytes <= limit);
        outlook_pst::open_store(&file.path).unwrap();
    }
    assert_eq!(
        std::fs::read_dir(f.root.join("oversized")).unwrap().count(),
        2
    );
    assert_source_hash(&f, &hash);

    let mut options = ConvertOptions::new(f.root.join("cancel-parts"), "cancel".into());
    options.split = pstoff_engine::Split::Bytes(limit);
    let token = CancelToken::default();
    let result = convert(
        &f.source,
        &options,
        &mut |p: pstoff_engine::Progress| {
            if p.part == 2 {
                token.cancel();
            }
        },
        &token,
    )
    .unwrap();
    let Conversion::Cancelled { completed_files } = result else {
        panic!("expected cancellation")
    };
    assert_eq!(completed_files.len(), 1);
    assert!(completed_files
        .iter()
        .next()
        .unwrap()
        .unwrap()
        .path
        .exists());
    assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 1);
    assert_source_hash(&f, &hash);
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn exclusions_cascade_and_update_progress_evidence_and_folder_mapping() {
    let mut store = source_store(vec![Item::new("IPM.Note", 100)]);
    let mut header = Item::new("IPM.Contact", 200);
    header.headers_only = true;
    let mut damaged = Item::new("IPM.Task", 300);
    damaged.unreadable = true;
    store.root.children[0].children[1].children.push(Folder {
        name: "Excluded".into(),
        items: vec![header],
        children: vec![Folder {
            name: "Child".into(),
            items: vec![damaged],
            ..Default::default()
        }],
        ..Default::default()
    });
    let f = support::Fixture::invalid();
    std::fs::write(&f.source, store.build()).unwrap();
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(std::fs::read(&f.source).unwrap()));
    let inspection = inspect(&f.source).unwrap();
    let parent = &inspection.folders[0].children[0].children[1].children[1];
    for split in [pstoff_engine::Split::Off, pstoff_engine::Split::default()] {
        let mut options =
            ConvertOptions::new(f.root.join(format!("exclude-{split:?}")), "selected".into());
        options.split = split;
        options.exclusions = vec![parent.id.clone()];
        let mut events = vec![];
        let Conversion::Complete(report) = convert(
            &f.source,
            &options,
            &mut |p| events.push(p),
            &CancelToken::default(),
        )
        .unwrap() else {
            panic!("cancelled")
        };
        let report = Box::new(report.collect_report().unwrap());
        assert!(events.iter().all(|p| p.total == 1));
        assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 1);
        assert_eq!(report.folders.iter().map(|f| f.bytes).sum::<u64>(), 100);
        assert!(report
            .folders
            .iter()
            .all(|f| f.folder != parent.id && f.folder != parent.children[0].id));
        assert!(report.skipped.is_empty());
        checked(&report.evidence.headers_only, 0);
        checked(&report.evidence.unreadable, 0);
        let mut output =
            pstoff_format::ndb::Pst::open(report.files[0].path.to_str().unwrap()).unwrap();
        assert!(output
            .nodes()
            .iter()
            .all(|n| n.nid != parent.id.0 && n.nid != parent.children[0].id.0));
        assert_source_hash(&f, &hash);
    }
    let report = split_report(run_split(
        &f,
        "exclude-all",
        u64::MAX,
        vec![inspection.folders[0].id.clone()],
    ));
    assert_eq!(report.folders.len(), 5);
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 0);
    assert_source_hash(&f, &hash);
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn split_retains_populated_folder_ancestors_and_omits_empty_siblings() {
    let mut store = source_store(vec![]);
    store.root.children[0].children[1].children.push(Folder {
        name: "Ancestor".into(),
        children: vec![
            Folder {
                name: "Populated".into(),
                items: vec![Item::new("IPM.Note", 123)],
                ..Default::default()
            },
            Folder {
                name: "Empty sibling".into(),
                ..Default::default()
            },
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
    assert!(report
        .folders
        .iter()
        .all(|f| f.folder != ancestor.children[1].id));
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let id = store
        .properties()
        .make_entry_id(outlook_pst::ndb::node_id::NodeId::from(ancestor.id.0))
        .unwrap();
    let actual = store.open_folder(&id).unwrap();
    assert_eq!(actual.hierarchy_table().unwrap().rows_matrix().count(), 1);
    assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
}

#[test]
fn invalid_split_and_unknown_exclusion_publish_nothing_and_preserve_source_hash() {
    let (f, _, hash) = split_fixture();
    for (base, limit, exclusions) in [
        ("zero", 0, vec![]),
        (
            "unknown-folder",
            u64::MAX,
            vec![pstoff_engine::FolderRef(0xffffffe2)],
        ),
    ] {
        let error = run_split(&f, base, limit, exclusions).unwrap_err();
        assert!(matches!(
            error.kind,
            pstoff_engine::ErrorKind::InvalidOptions
        ));
        assert!(error.completed_files.is_empty());
        assert!(!f.root.join(base).exists());
        assert_source_hash(&f, &hash);
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn later_split_name_collision_preserves_finished_parts_and_existing_file() {
    let (f, folders, hash) = split_fixture();
    let measured = split_report(run_split(
        &f,
        "measure",
        u64::MAX,
        vec![folders[1].clone(), folders[2].clone()],
    ));
    let destination = f.root.join("collision");
    std::fs::create_dir(&destination).unwrap();
    let held = destination.join("collision-part002.pst");
    std::fs::write(&held, b"existing output").unwrap();
    let error = run_split(
        &f,
        "collision",
        measured.files[0].bytes,
        vec![folders[2].clone()],
    )
    .unwrap_err();
    assert!(matches!(error.kind, pstoff_engine::ErrorKind::Collision));
    assert_eq!(error.completed_files.len(), 1);
    assert_eq!(std::fs::read(&held).unwrap(), b"existing output");
    outlook_pst::open_store(&error.completed_files.iter().next().unwrap().unwrap().path).unwrap();
    assert_eq!(std::fs::read_dir(destination).unwrap().count(), 2);
    assert_source_hash(&f, &hash);
}

// Decode values with Microsoft's reader, including the existence bitmap and
// heap/subnode references. Row counts alone do not establish cache fidelity.
type IndependentProperties =
    std::collections::BTreeMap<u16, outlook_pst::ltp::prop_context::PropertyValue>;
fn independent_rows(
    table: &dyn outlook_pst::ltp::table_context::TableContext,
) -> Vec<(u32, IndependentProperties)> {
    table
        .rows_matrix()
        .map(|row| {
            let cells = row.columns(table.context()).unwrap();
            let values = table
                .context()
                .columns()
                .iter()
                .zip(cells)
                .filter_map(|(column, cell)| {
                    cell.map(|cell| {
                        (
                            column.prop_id(),
                            table.read_column(&cell, column.prop_type()).unwrap(),
                        )
                    })
                })
                .collect();
            (u32::from(row.id()), values)
        })
        .collect()
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn q16_aliased_cells_require_an_exact_item_property_in_normal_and_associated_tables() {
    use outlook_pst::ltp::prop_context::PropertyValue;
    use synthetic::Property;
    for associated in [false, true] {
        // A corrupt contents cell refers to the TCINFO allocation itself.
        let mut alias = Property::text(0x0c1a, "");
        alias.table_root_alias = true;
        let mut missing = Item::new("IPM.Note", 10);
        missing.cached.push(alias.clone());
        let mut table_only = Property::time(0x6628, 42);
        table_only.kind = 0x14;
        missing.cached.push(table_only);
        let mut wrong_type = Item::new("IPM.Note", 20);
        wrong_type
            .properties
            .push(Property::binary(0x0c1a, vec![0x42; 8]));
        wrong_type.cached.push(alias.clone());
        let mut exact = Item::new("IPM.Note", 30);
        exact
            .properties
            .push(Property::text(0x0c1a, "Generated sender"));
        exact.cached.push(alias);
        let mut items = vec![missing, wrong_type, exact];
        for item in &mut items {
            item.associated = associated;
        }
        let bytes = source_items(items);
        let (_f, report) = complete(&bytes, "aliased-cache");
        assert!(report.skipped.is_empty());
        let store = outlook_pst::open_store(&report.files[0].path).unwrap();
        let folder = report.folders.iter().find(|f| f.items == 3).unwrap();
        let id = store
            .properties()
            .make_entry_id(folder.folder.0.into())
            .unwrap();
        let folder = store.open_folder(&id).unwrap();
        let table = if associated {
            folder.associated_table()
        } else {
            folder.contents_table()
        }
        .unwrap();
        let rows = independent_rows(table.as_ref());
        assert_eq!(rows.len(), 3);
        for (index, (nid, cells)) in rows.iter().enumerate() {
            assert!(cells.contains_key(&0x67f2));
            assert!(cells.contains_key(&0x67f3));
            if index < 2 {
                assert!(
                    !cells.contains_key(&0x0c1a),
                    "untrusted cached sender retained"
                );
            } else {
                let Some(PropertyValue::Unicode(value)) = cells.get(&0x0c1a) else {
                    panic!("missing exact sender")
                };
                assert_eq!(value.to_string(), "Generated sender");
            }
            assert!(!cells.contains_key(&0x6628));
            for id in [0x0e30, 0x0e33, 0x0e34, 0x0e3d, 0x0e3c, 0x0e38] {
                assert!(!cells.contains_key(&id), "invented default cell");
            }
            let difference = report
                .differences
                .iter()
                .find(|d| {
                    d.property == 0x0c1a001f && d.item.as_ref().is_some_and(|i| i.node == *nid)
                })
                .unwrap();
            assert!(difference.source.contains("Contents table 0x"));
            assert!(difference.reason.contains(if index < 2 {
                "no exact property tag"
            } else {
                "rebuilt"
            }));
        }
        assert!(report
            .differences
            .iter()
            .any(|d| d.property == 0x66280014 && d.reason.contains("Q17")));
    }
}

fn cache_pair(counter: u8) -> Vec<synthetic::Property> {
    let mut key = vec![0x23; 16];
    key.extend([0, 0, 0, 0, 0, counter]);
    let mut pcl = vec![key.len() as u8];
    pcl.extend(&key);
    vec![
        synthetic::Property::binary(0x65e2, key),
        synthetic::Property::binary(0x65e3, pcl),
    ]
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn q15_conflict_preserves_public_source_error_kind_and_local_context() {
    for property in [0x65e2, 0x65e3] {
        let f = support::Fixture::invalid();
        let mut item = Item::new("IPM.Note", 10);
        item.cached = cache_pair(1);
        item.properties = cache_pair(2)
            .into_iter()
            .filter(|p| p.id == property)
            .collect();
        let bytes = source_items(vec![item]);
        std::fs::write(&f.source, &bytes).unwrap();
        let options = unsplit_options(f.root.join("out"), "conflict".into());
        let error = convert(&f.source, &options, &mut |_| {}, &CancelToken::default()).unwrap_err();
        assert!(
            matches!(error.kind, pstoff_engine::ErrorKind::DamagedSource),
            "{error:?}"
        );
        let mut source = pstoff_format::ndb::Pst::open(f.source.to_str().unwrap()).unwrap();
        let item = source
            .nodes()
            .into_iter()
            .find(|n| n.nid_type() == 4)
            .unwrap();
        for context in [
            "Q15 conflict".into(),
            format!("Folder 0x{:x}", item.nid_parent),
            format!("Item 0x{:x}", item.nid),
            format!("property 0x{property:04x}0102"),
            format!("contents table 0x{:x}", item.nid_parent & !31 | 0xe),
            "no cache value chosen".into(),
        ] {
            assert!(
                error.to_string().contains(&context),
                "missing {context}: {error}"
            );
        }
        assert!(error.completed_files.is_empty());
        assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 0);
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn q12_and_q15_agreeing_cache_values_are_recovered_and_reported() {
    use outlook_pst::ltp::prop_context::PropertyValue;
    let mut item = Item::new("IPM.Note", 10);
    item.cached = cache_pair(1);
    item.cached
        .push(synthetic::Property::time(0x3008, 132_000_000_000_000_000));
    let (_f, report) = complete(&source_items(vec![item]), "recovered-cache");
    let store = outlook_pst::open_store(&report.files[0].path).unwrap();
    let folder = report.folders.iter().find(|f| f.items == 1).unwrap();
    let id = store
        .properties()
        .make_entry_id(folder.folder.0.into())
        .unwrap();
    let folder = store.open_folder(&id).unwrap();
    let rows = independent_rows(folder.contents_table().unwrap().as_ref());
    for expected in cache_pair(1) {
        let Some(PropertyValue::Binary(value)) = rows[0].1.get(&expected.id) else {
            panic!("missing recovered pair member")
        };
        assert_eq!(value.buffer(), expected.bytes);
    }
    assert!(matches!(
        rows[0].1.get(&0x3008),
        Some(PropertyValue::Time(132_000_000_000_000_000))
    ));
    for (tag, rule) in [
        (0x30080040, "Q12"),
        (0x65e20102, "Q15"),
        (0x65e30102, "Q15"),
    ] {
        assert!(report
            .differences
            .iter()
            .any(|d| d.property == tag && d.reason.contains(rule) && d.item.is_some()));
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn report_failures_after_publication_roll_back_unsplit_outputs_but_keep_split_parts() {
    // Arm deterministic filesystem faults through the public progress sink.
    // At 0.95 staging exists; PST publication precedes report writing/renaming.
    for split in [false, true] {
        for fault in ["write-json", "write-text", "publish-text"] {
            let f = support::Fixture::invalid();
            let bytes = source();
            std::fs::write(&f.source, &bytes).unwrap();
            let mut options = unsplit_options(f.root.join("out"), "rollback".into());
            if split {
                options.split = pstoff_engine::Split::Bytes(u64::MAX);
            }
            let mut injected = false;
            let error = convert(
                &f.source,
                &options,
                &mut |p: pstoff_engine::Progress| {
                    if p.fraction == 0.95 {
                        injected = true;
                        let staging = options.destination.join(".rollback.pstoff-partial");
                        match fault {
                            "write-json" => {
                                std::fs::create_dir(staging.join("report.json")).unwrap()
                            }
                            "write-text" => {
                                std::fs::create_dir(staging.join("report.txt")).unwrap()
                            }
                            "publish-text" => std::fs::write(
                                options.destination.join("rollback.txt"),
                                b"other caller",
                            )
                            .unwrap(),
                            _ => unreachable!(),
                        }
                    }
                },
                &CancelToken::default(),
            )
            .unwrap_err();
            assert!(injected);
            assert!(
                matches!(
                    error.kind,
                    pstoff_engine::ErrorKind::Io | pstoff_engine::ErrorKind::Collision
                ),
                "{error:?}"
            );
            assert_eq!(error.completed_files.len(), usize::from(split));
            let pst = options.destination.join(if split {
                "rollback-part001.pst"
            } else {
                "rollback.pst"
            });
            assert_eq!(pst.exists(), split);
            if split {
                outlook_pst::open_store(&pst).unwrap();
            }
            assert!(!options.destination.join("rollback.json").exists());
            assert!(!options
                .destination
                .join(".rollback.pstoff-partial")
                .exists());
            if fault == "publish-text" {
                assert_eq!(
                    std::fs::read(options.destination.join("rollback.txt")).unwrap(),
                    b"other caller"
                );
            } else {
                assert!(!options.destination.join("rollback.txt").exists());
            }
            assert_eq!(
                std::fs::read_dir(&options.destination).unwrap().count(),
                usize::from(split) + usize::from(fault == "publish-text")
            );
            assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
        }
    }
}

// Microsoft's high-level embedded attachment reader looks for the embedded NID
// in the parent's subnodes. Its public NDB APIs can still traverse the actual
// attachment-local namespace and read its complete logical payload streams.
fn independent_subnodes(
    pst: &outlook_pst::UnicodePstFile,
    bid: outlook_pst::ndb::block_id::UnicodeBlockId,
) -> Vec<outlook_pst::ndb::block::UnicodeLeafSubNodeTreeEntry> {
    use outlook_pst::{
        ndb::{
            block::SubNodeTree, block_id::BlockId, header::Header, page::UnicodeBlockBTree,
            root::Root,
        },
        PstFile,
    };
    let mut file = pst.reader().lock().unwrap();
    let tree = UnicodeBlockBTree::read(&mut *file, *pst.header().root().block_btree()).unwrap();
    let mut cache = Default::default();
    let block = tree
        .find_entry(&mut *file, bid.search_key(), &mut cache)
        .unwrap();
    SubNodeTree::<outlook_pst::UnicodePstFile>::read(&mut *file, &block)
        .unwrap()
        .entries(&mut *file, &tree, &mut cache)
        .unwrap()
        .collect()
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn microsoft_reader_verifies_recipients_attachment_bytes_and_associated_item_properties() {
    use outlook_pst::{
        ltp::prop_context::PropertyValue,
        messaging::{
            attachment::{Attachment as _, AttachmentData, UnicodeAttachment},
            message::{Message as _, UnicodeMessage},
            store::{Store as _, UnicodeStore},
        },
        ndb::page::NodeBTreeEntry,
        PstFile, UnicodePstFile,
    };
    use synthetic::{Attachment, Property, Recipient};
    // Cross many unequal 8000-byte source / 8176-byte writer boundaries, in
    // repeated attachment-local namespaces in both ordinary and associated Items.
    let payload: Vec<_> = (0..180_000u32).map(|i| i.wrapping_mul(31) as u8).collect();
    let embedded_payload: Vec<_> = (0..140_000u32)
        .map(|i| i.wrapping_mul(17).wrapping_add(3) as u8)
        .collect();
    let mut items = vec![];
    for associated in [false, true] {
        let mut item = Item::new(
            if associated {
                "IPM.Configuration"
            } else {
                "IPM.Note"
            },
            100,
        );
        item.associated = associated;
        item.properties
            .push(Property::binary(0x6800, vec![u8::from(associated); 32]));
        item.recipient_row_ids = vec![7, 44];
        item.recipients = vec![
            Recipient {
                name: "Generated To".into(),
                address: "to@example.invalid".into(),
                kind: 1,
            },
            Recipient {
                name: "Generated Cc".into(),
                address: "cc@example.invalid".into(),
                kind: 2,
            },
        ];
        let mut inner = Item::new("IPM.Note", 18_000);
        inner.properties = vec![
            Property::text(0x0037, "Generated embedded subject"),
            Property::binary(0x6801, embedded_payload.clone()),
        ];
        // A positive embedded recipient table also exercises the nested namespace.
        inner.recipients.push(Recipient {
            name: "Generated nested".into(),
            address: "nested@example.invalid".into(),
            kind: 1,
        });
        item.attachments = vec![
            Attachment::Bytes(payload.clone()),
            Attachment::Embedded(Box::new(inner)),
        ];
        items.push(item);
    }
    let (_f, report) = complete(&source_items(items), "nested-fidelity");
    assert!(report.skipped.is_empty());
    assert_eq!(report.folders.iter().map(|f| f.items).sum::<u64>(), 2);
    let pst = std::rc::Rc::new(UnicodePstFile::open(&report.files[0].path).unwrap());
    let store = UnicodeStore::read(pst.clone()).unwrap();
    let folder = report.folders.iter().find(|f| f.items == 2).unwrap();
    let id = store
        .properties()
        .make_entry_id(folder.folder.0.into())
        .unwrap();
    let folder = store.open_folder(&id).unwrap();
    for (associated, table) in [
        (false, folder.contents_table().unwrap()),
        (true, folder.associated_table().unwrap()),
    ] {
        let rows = independent_rows(table.as_ref());
        assert_eq!(rows.len(), 1);
        let (nid, _) = &rows[0];
        assert_eq!(*nid & 31, if associated { 8 } else { 4 });
        let id = store.properties().make_entry_id((*nid).into()).unwrap();
        let message = UnicodeMessage::read(store.clone(), &id, None).unwrap();
        assert_eq!(
            message.properties().message_class().unwrap(),
            if associated {
                "IPM.Configuration"
            } else {
                "IPM.Note"
            }
        );
        let Some(PropertyValue::Binary(marker)) = message.properties().get(0x6800) else {
            panic!("missing Item marker")
        };
        assert_eq!(marker.buffer(), [u8::from(associated); 32]);
        let recipients = independent_rows(message.recipient_table().unwrap().as_ref());
        assert_eq!(recipients.len(), 2);
        for ((row, cells), (expected_row, name, address, kind)) in recipients.iter().zip([
            (7, "Generated To", "to@example.invalid", 1),
            (44, "Generated Cc", "cc@example.invalid", 2),
        ]) {
            assert_eq!(*row, expected_row);
            assert!(
                matches!(cells.get(&0x0c15), Some(PropertyValue::Integer32(value)) if *value == kind)
            );
            for (tag, expected) in [(0x3001, name), (0x3002, "SMTP"), (0x3003, address)] {
                let Some(PropertyValue::Unicode(value)) = cells.get(&tag) else {
                    panic!("missing recipient field")
                };
                assert_eq!(value.to_string(), expected);
            }
        }
        let attachments = independent_rows(message.attachment_table().unwrap().as_ref());
        assert_eq!(attachments.len(), 2);
        for (index, (_, cells)) in attachments.iter().enumerate() {
            assert!(
                matches!(cells.get(&0x0e21), Some(PropertyValue::Integer32(value)) if *value == index as i32)
            );
        }
        let attachment = UnicodeAttachment::read(message, attachments[0].0.into(), None).unwrap();
        assert_eq!(attachment.properties().attachment_method().unwrap(), 1);
        let Some(AttachmentData::Binary(data)) = attachment.data() else {
            panic!("missing attachment bytes")
        };
        assert_eq!(data.buffer(), payload);

        // Traverse nested subnodes using only Microsoft's reader. No writer or
        // pstoff-format output reader supplies these identities or bytes.
        let top = pst.read_node((*nid).into()).unwrap();
        let subs = independent_subnodes(&pst, top.sub_node().unwrap());
        let embedded_attachment = subs
            .iter()
            .find(|n| u32::from(n.node()) == attachments[1].0)
            .unwrap();
        let nested = independent_subnodes(&pst, embedded_attachment.sub_node().unwrap());
        let embedded = nested
            .iter()
            .find(|n| u32::from(n.node()) & 31 == 4)
            .unwrap();
        let pc = pst.read_block(embedded.block()).unwrap();
        assert_eq!(&pc[2..4], &[0xec, 0xbc]);
        let subject: Vec<_> = "Generated embedded subject"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert!(pc.windows(subject.len()).any(|window| window == subject));
        let inner_subs = independent_subnodes(&pst, embedded.sub_node().unwrap());
        let streams: Vec<_> = inner_subs
            .iter()
            .filter(|n| u32::from(n.node()) & 31 == 0x1f)
            .map(|n| pst.read_block(n.block()).unwrap())
            .collect();
        assert!(streams.iter().any(|bytes| bytes == &embedded_payload));
        let recipient = inner_subs
            .iter()
            .find(|n| u32::from(n.node()) & 31 == 0x12)
            .unwrap();
        let node = outlook_pst::ndb::page::UnicodeNodeBTreeEntry::new(
            recipient.node(),
            recipient.block(),
            recipient.sub_node(),
            None,
        );
        let table = outlook_pst::ltp::table_context::UnicodeTableContext::read(store.clone(), node)
            .unwrap();
        let recipients = independent_rows(table.as_ref());
        assert_eq!(recipients.len(), 1);
        let Some(PropertyValue::Unicode(address)) = recipients[0].1.get(&0x3003) else {
            panic!("missing embedded recipient")
        };
        assert_eq!(address.to_string(), "nested@example.invalid");
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn arbitrary_store_payload_survives_bounded_store_preparation() {
    let payload: Vec<_> = (0..180_013).map(|i| (i * 37 % 251) as u8).collect();
    let bytes = source_store(vec![Item::new("IPM.Note", 1234)]).build_with_store_properties(&[
        folder_entry(0x35e0, 0x182),
        folder_entry(0x35e3, 0x1c2),
        folder_entry(0x35e7, 0x162),
        synthetic::Property::binary(0x6801, payload.clone()),
    ]);
    let (_fixture, report) = complete(&bytes, "store-payload");
    let output = outlook_pst::open_store(&report.files[0].path).unwrap();
    let Some(outlook_pst::ltp::prop_context::PropertyValue::Binary(value)) =
        output.properties().get(0x6801)
    else {
        panic!("missing synthetic store payload")
    };
    assert!(value.buffer() == payload, "Synthetic store payload differs");
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn progress_finishes_monotonically_and_all_cancellation_boundaries_clean_output() {
    for boundary in [0, 1, 2, 3, 4] {
        let f = support::Fixture::invalid();
        let bytes = source_items((0..3).map(|_| Item::new("IPM.Note", 10)).collect());
        std::fs::write(&f.source, &bytes).unwrap();
        let options = unsplit_options(f.root.join("out"), "progress".into());
        let cancel = CancelToken::default();
        if boundary == 0 {
            cancel.cancel();
        }
        let mut fraction = 0.0;
        let mut done = 0;
        let mut events = 0;
        let result = convert(
            &f.source,
            &options,
            &mut |p: pstoff_engine::Progress| {
                assert!(p.fraction >= fraction && p.fraction <= 1.0);
                assert!(p.done >= done && p.done <= p.total);
                fraction = p.fraction;
                done = p.done;
                events += 1;
                if (boundary == 1 && p.done == 1)
                    || (boundary == 2 && p.done == p.total && p.fraction < 0.95)
                    || (boundary == 3 && p.fraction == 0.95)
                {
                    cancel.cancel();
                }
            },
            &cancel,
        )
        .unwrap();
        if boundary == 4 {
            assert!(matches!(result, Conversion::Complete(_)));
            assert_eq!(fraction, 1.0);
            assert_eq!(done, 3);
        } else {
            assert!(
                matches!(result, Conversion::Cancelled { completed_files } if completed_files.is_empty())
            );
            assert!(fraction < 1.0);
            if options.destination.exists() {
                assert_eq!(std::fs::read_dir(&options.destination).unwrap().count(), 0);
            }
            if boundary == 0 {
                assert_eq!(events, 0);
            }
        }
        assert_eq!(std::fs::read(&f.source).unwrap(), bytes);
    }
}

#[test]
#[cfg_attr(not(windows), ignore = "requires Windows disk preflight")]
fn split_trial_progress_never_rewinds_and_ends_at_one() {
    let (f, folders, hash) = split_fixture();
    let mut options = ConvertOptions::new(f.root.join("progress-parts"), "progress".into());
    options.split = pstoff_engine::Split::Bytes(80_000);
    options.exclusions = vec![folders[2].clone()];
    let mut fraction = 0.0;
    let mut done = 0;
    let mut last_part = 0;
    let result = convert(
        &f.source,
        &options,
        &mut |p: pstoff_engine::Progress| {
            assert!(p.fraction >= fraction && p.fraction <= 1.0);
            assert!(p.done >= done && p.done <= p.total);
            assert!(p.part >= last_part);
            fraction = p.fraction;
            done = p.done;
            last_part = p.part;
        },
        &CancelToken::default(),
    )
    .unwrap();
    assert!(matches!(result, Conversion::Complete(_)));
    assert_eq!(fraction, 1.0);
    assert_eq!(done, 2);
    assert_source_hash(&f, &hash);
}
