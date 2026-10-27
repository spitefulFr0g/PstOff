mod support;
#[path = "support/synthetic.rs"]
mod synthetic;

use pstoff_engine::{inspect, Support};
use synthetic::{Attachment, Folder, Item, Property, Recipient, Store};

fn rich_sample() -> Store {
    let mut items: Vec<_> = [
        "IPM.Note",
        "IPM.Appointment",
        "IPM.Contact",
        "IPM.Task",
        "IPM.StickyNote",
        "IPM.Activity",
    ]
    .into_iter()
    .enumerate()
    .map(|(i, class)| Item::new(class, (i as u32 + 1) * 100))
    .collect();
    items[0].properties = vec![Property::text(0x1000, &"Synthetic body ".repeat(2000))];
    // Multiple allocations span more than one HN page, independently of the body stream.
    for id in 0x6800..0x680a {
        items[0]
            .properties
            .push(Property::binary(id, vec![0x23; 1000]));
    }
    items[0].recipients.push(Recipient {
        name: "Synthetic recipient".into(),
        address: "fixture@example.invalid".into(),
        kind: 1,
    });
    items[0].attachments = vec![
        Attachment::Bytes(vec![0x42; 20000]),
        Attachment::Embedded(Box::new(Item::new("IPM.Note", 17))),
        Attachment::External("https://example.invalid/synthetic".into()),
    ];
    items[1].headers_only = true;
    items[2].cached = vec![
        Property::time(0x3008, 132_000_000_000_000_000),
        Property::binary(0x65e2, vec![1; 22]),
        Property::binary(0x65e3, vec![2; 23]),
    ];
    Store {
        root: Folder {
            name: "Synthetic rich".into(),
            items,
            ..Default::default()
        },
    }
}

#[test]
fn every_item_class_with_nested_content_and_large_heaps_is_readable() {
    let fixture = support::Fixture::invalid();
    let bytes = rich_sample().build();
    std::fs::write(&fixture.source, &bytes).unwrap();
    let result = inspect(&fixture.source).unwrap();
    assert_eq!(result.folders[0].item_count, 6);
    assert_eq!(result.folders[0].bytes, 2100);
    assert!(
        result.folders[0].unreadable.is_none(),
        "{:?}",
        result.folders[0].unreadable
    );
    assert_eq!(std::fs::read(&fixture.source).unwrap(), bytes);
}

fn defective_sample(folder: bool) -> Store {
    let mut store = sample();
    if folder {
        store.root.children[0].unreadable = true;
    } else {
        store.root.children[0].items[0].unreadable = true;
    }
    store
}

#[test]
fn unreadable_items_and_folders_remain_visible_in_inspection() {
    for folder in [false, true] {
        let fixture = support::Fixture::invalid();
        let bytes = defective_sample(folder).build();
        std::fs::write(&fixture.source, &bytes).unwrap();
        let result = inspect(&fixture.source).unwrap();
        let root = &result.folders[0];
        assert!(root.unreadable.is_none());
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].item_count, 2);
        assert!(root.children[0].unreadable.is_some());
        assert_eq!(root.children[0].bytes, if folder { 1290 } else { 56 });
        assert_eq!(std::fs::read(&fixture.source).unwrap(), bytes);
    }
}

fn sample() -> Store {
    Store {
        root: Folder {
            name: "Synthetic root".into(),
            children: vec![Folder {
                name: "Synthetic child".into(),
                items: vec![Item::new("IPM.Note", 1234), Item::new("IPM.Contact", 56)],
                ..Default::default()
            }],
            ..Default::default()
        },
    }
}

// Explicit opt-in: artifacts leave the guest only via the wrapper's writable output.
#[test]
#[ignore = "export for the WSL libpff check; run in Windows Sandbox"]
fn export_for_libpff() {
    let out = std::path::Path::new(r"C:\PstOffOutput\synthetic-23");
    std::fs::create_dir_all(out).unwrap();
    std::fs::write(out.join("tree.ost"), sample().build()).unwrap();
    std::fs::write(out.join("rich.ost"), rich_sample().build()).unwrap();
    std::fs::write(
        out.join("unreadable-item.ost"),
        defective_sample(false).build(),
    )
    .unwrap();
    std::fs::write(
        out.join("unreadable-folder.ost"),
        defective_sample(true).build(),
    )
    .unwrap();
}

#[test]
fn described_tree_counts_and_sizes_round_trip_read_only() {
    let description = Store {
        root: Folder {
            name: "Synthetic root".into(),
            children: vec![Folder {
                name: "Synthetic child".into(),
                items: vec![Item::new("IPM.Note", 1234), Item::new("IPM.Contact", 56)],
                ..Default::default()
            }],
            ..Default::default()
        },
    };
    let bytes = description.build();
    assert_eq!(bytes, description.build());
    let fixture = support::Fixture::invalid();
    std::fs::write(&fixture.source, &bytes).unwrap();
    let result = inspect(&fixture.source).unwrap();
    assert_eq!(result.support, Support::Supported);
    assert_eq!(result.folders.len(), 1);
    let root = &result.folders[0];
    assert_eq!(
        (root.name.as_str(), root.item_count, root.bytes),
        ("Synthetic root", 0, 0)
    );
    assert_eq!(root.children.len(), 1);
    let child = &root.children[0];
    assert_eq!(
        (child.name.as_str(), child.item_count, child.bytes),
        ("Synthetic child", 2, 1290)
    );
    assert!(root.unreadable.is_none());
    assert!(child.unreadable.is_none());
    assert_eq!(std::fs::read(&fixture.source).unwrap(), bytes);
}
