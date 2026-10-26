use std::process::Command;

#[test]
fn inspect_rejects_non_ost_as_damaged() {
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
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("not a valid OST header"));
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

#[test]
fn convert_unsupported_input_exits_three_without_output() {
    let root = std::env::temp_dir().join(format!("pstoff-cli-unsupported-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("unsupported.ost");
    let output_dir = root.join("out");
    std::fs::create_dir(&output_dir).unwrap();
    let mut header = vec![0; 564];
    header[..4].copy_from_slice(b"!BDN");
    header[8..10].copy_from_slice(b"SO");
    header[10..12].copy_from_slice(&14u16.to_le_bytes());
    std::fs::write(&source, header).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_pstoff-cli"))
        .arg("convert")
        .arg(&source)
        .arg(&output_dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(std::fs::read_dir(&output_dir).unwrap().count(), 0);
    std::fs::remove_dir_all(root).unwrap();
}
