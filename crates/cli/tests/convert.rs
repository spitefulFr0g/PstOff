#[allow(dead_code)]
#[path = "../../engine/tests/support/synthetic.rs"]
mod synthetic;
use std::path::PathBuf;
use std::process::{Command, Output};
use synthetic::{Folder, Item, Store};

struct Fixture {
    root: PathBuf,
    source: PathBuf,
    out: PathBuf,
    bytes: Vec<u8>,
}
impl Fixture {
    fn new(items: Vec<Item>) -> Self {
        let root = std::env::temp_dir().join(format!(
            "pstoff-cli-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
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
        .build();
        std::fs::write(&source, &bytes).unwrap();
        let out = root.join("out");
        Self {
            root,
            source,
            out,
            bytes,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pstoff"));
        command
            .arg("convert")
            .arg(&self.source)
            .arg("--out")
            .arg(&self.out);
        command
    }

    fn report(&self, base: &str) -> pstoff_engine::ConversionReport {
        serde_json::from_slice(&std::fs::read(self.out.join(format!("{base}.json"))).unwrap())
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(std::fs::read(&self.source).unwrap(), self.bytes);
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

fn status(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn convert_out_writes_pst_and_reports_with_stderr_progress() {
    let f = Fixture::new(vec![Item::new("IPM.Note", 42)]);
    let result = f.command().output().unwrap();
    status(&result, 0);
    for name in ["synthetic-part001.pst", "synthetic.json", "synthetic.txt"] {
        assert!(f.out.join(name).is_file(), "{name}");
    }
    assert_eq!(std::fs::read_dir(&f.out).unwrap().count(), 3);
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("Mailbox completeness: unknown"));
    assert!(!stdout.contains("Progress:"));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("Progress: 100.0% items 1/1 part 1"));
    assert!(f.report("synthetic").skipped.is_empty());
}

#[test]
fn explicit_options_reach_engine_and_inspect_exposes_exclusion_ids() {
    let f = Fixture::new(vec![Item::new("IPM.Note", 42)]);
    let inspected = Command::new(env!("CARGO_BIN_EXE_pstoff"))
        .arg("inspect")
        .arg(&f.source)
        .output()
        .unwrap();
    status(&inspected, 0);
    let tree = String::from_utf8(inspected.stdout).unwrap();
    let line = tree
        .lines()
        .find(|line| line.contains("IPM_SUBTREE"))
        .unwrap();
    let id = line.split('[').nth(1).unwrap().split(']').next().unwrap();
    let result = f
        .command()
        .args([
            "--base-name",
            "chosen",
            "--exclude-folder",
            id,
            "--exclude-folder",
            &u32::from_str_radix(&id[2..], 16).unwrap().to_string(),
            "--split-limit",
            "off",
        ])
        .output()
        .unwrap();
    status(&result, 0);
    assert!(f.out.join("chosen.pst").is_file());
    assert!(f.out.join("chosen.txt").is_file());
    let report = f.report("chosen");
    assert_eq!(
        report
            .folders
            .iter()
            .map(|folder| folder.items)
            .sum::<u64>(),
        0
    );
    assert_eq!(std::fs::read_dir(&f.out).unwrap().count(), 3);
}

#[test]
fn skipped_items_capacity_and_other_failures_have_distinct_codes() {
    let mut damaged = Item::new("IPM.Note", 42);
    damaged.unreadable = true;
    let f = Fixture::new(vec![Item::new("IPM.Note", 42), damaged]);
    let result = f.command().args(["--split-limit", "off"]).output().unwrap();
    status(&result, 6);
    assert_eq!(f.report("synthetic").skipped.len(), 1);
    // Collision is a fatal failure and must preserve the existing output.
    let before = std::fs::read(f.out.join("synthetic.pst")).unwrap();
    status(
        &f.command().args(["--split-limit", "off"]).output().unwrap(),
        1,
    );
    assert_eq!(std::fs::read(f.out.join("synthetic.pst")).unwrap(), before);

    let f = Fixture::new(vec![Item::new("IPM.Note", 42)]);
    status(
        &f.command().args(["--split-limit", "1"]).output().unwrap(),
        5,
    );
    assert_eq!(std::fs::read_dir(&f.out).unwrap().count(), 0);
}

#[test]
fn invalid_cli_options_are_usage_errors_without_outputs() {
    let f = Fixture::new(vec![Item::new("IPM.Note", 42)]);
    for args in [
        vec!["--split-limit", "0"],
        vec!["--split-limit", "18446744073709551616"],
        vec!["--exclude-folder", "not-an-id"],
        vec!["--split-limit", "off", "--split-limit", "1"],
        vec!["--out", "other"],
        vec!["--unknown", "value"],
        vec!["--base-name"],
    ] {
        status(&f.command().args(args).output().unwrap(), 2);
        assert!(!f.out.exists());
    }
}

#[cfg(windows)]
#[test]
fn locked_input_has_distinct_status_for_both_commands() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new(vec![Item::new("IPM.Note", 42)]);
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&f.source)
        .unwrap();
    status(&f.command().output().unwrap(), 4);
    status(
        &Command::new(env!("CARGO_BIN_EXE_pstoff"))
            .arg("inspect")
            .arg(&f.source)
            .output()
            .unwrap(),
        4,
    );
    assert!(!f.out.exists());
    drop(lock);
}

#[cfg(windows)]
#[path = "support/cancel.rs"]
mod cancel;
