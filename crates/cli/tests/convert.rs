#[allow(dead_code)]
#[path = "../../engine/tests/support/synthetic.rs"]
mod synthetic;
use std::process::Command;
use synthetic::{Folder, Item, Store};

#[test]
fn convert_out_writes_pst_and_reports_and_exits_zero() {
    let root = std::env::temp_dir().join(format!("pstoff-cli-convert-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("synthetic.ost");
    let bytes = Store {
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
                        items: vec![Item::new("IPM.Note", 42)],
                        children: vec![Folder {
                            name: "Deleted Items".into(),
                            ..Default::default()
                        }],
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        },
    }
    .build();
    std::fs::write(&source, &bytes).unwrap();
    let out = root.join("out");
    let result = Command::new(env!("CARGO_BIN_EXE_pstoff"))
        .arg("convert")
        .arg(&source)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for name in ["synthetic.pst", "synthetic.json", "synthetic.txt"] {
        assert!(out.join(name).is_file(), "{name}");
    }
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 3);
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    assert!(String::from_utf8_lossy(&result.stdout).contains("Mailbox completeness: unknown"));
    std::fs::remove_dir_all(root).unwrap();
}
