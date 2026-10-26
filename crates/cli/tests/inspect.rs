use std::process::Command;

#[test]
fn inspect_smoke_prints_store_support_and_folder_summary() {
    let root = std::env::temp_dir().join(format!(
        "pstoff-cli-inspect-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("synthetic.ost");
    std::fs::write(&source, [0; 512]).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pstoff-cli"))
        .arg("inspect")
        .arg(&source)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Support: unsupported (InvalidHeader)"));
    assert!(stdout.contains("Folders: none"));
    assert_eq!(std::fs::read(&source).unwrap(), [0; 512]);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn inspect_rejects_missing_path_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_pstoff-cli"))
        .arg("inspect")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr).unwrap().contains("Usage:"));
}
