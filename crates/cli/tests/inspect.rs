use std::process::Command;

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn crc(bytes: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    crc
}

fn synthetic_v36_ost() -> Vec<u8> {
    let mut bytes = vec![0; 16 * 1024];
    bytes[..4].copy_from_slice(b"!BDN");
    bytes[8..10].copy_from_slice(b"SO");
    put16(&mut bytes, 10, 36);
    let length = bytes.len() as u64;
    put64(&mut bytes, 184, length);
    put64(&mut bytes, 216, 4);
    put64(&mut bytes, 224, 4096);
    put64(&mut bytes, 232, 8);
    put64(&mut bytes, 240, 8192);
    bytes[512] = 0x80;
    bytes[248] = 1;
    bytes[4096..8192].copy_from_slice(&synthetic_page(32, 0x81, 4));
    bytes[8192..12288].copy_from_slice(&synthetic_page(24, 0x80, 8));
    seal_header(&mut bytes);
    bytes
}

fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn seal_header(bytes: &mut [u8]) {
    let header_crc = crc(&bytes[8..479]);
    let b5_crc = crc(&bytes[8..524]);
    put32(bytes, 4, header_crc);
    put32(bytes, 524, b5_crc);
}

fn synthetic_page(width: u8, kind: u8, id: u64) -> Vec<u8> {
    let mut page = vec![0; 4096];
    put16(&mut page, 4058, 100);
    page[4060] = width;
    page[4072] = kind;
    page[4073] = kind;
    page[4080..4088].copy_from_slice(&id.to_le_bytes());
    let checksum = crc(&page[..4072]);
    put32(&mut page, 4076, checksum);
    page
}

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

    let output = Command::new(env!("CARGO_BIN_EXE_pstoff"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_pstoff"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_pstoff"))
        .arg("convert")
        .arg(&source)
        .arg("--out")
        .arg(&output_dir)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(std::fs::read_dir(&output_dir).unwrap().count(), 0);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_header_variants_are_reported_by_inspect_and_rejected_by_convert() {
    let cases = [
        ("ANSI", 14, 0, "Ansi"),
        ("v21 Unicode", 21, 0, "OlderUnicode(21)"),
        ("v23 Unicode", 23, 0, "OlderUnicode(23)"),
        ("nonzero bCryptMethod", 36, 1, "Encoding(1)"),
        ("unknown version", 37, 0, "UnknownVersion(37)"),
    ];
    for (name, version, method, reason) in cases {
        let root = std::env::temp_dir().join(format!(
            "pstoff-cli-header-case-{}-{}-{}",
            std::process::id(),
            name.replace(' ', "-"),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("synthetic.ost");
        let output_dir = root.join("out");
        std::fs::create_dir(&output_dir).unwrap();
        let mut bytes = synthetic_v36_ost();
        put16(&mut bytes, 10, version);
        bytes[513] = method;
        seal_header(&mut bytes);
        std::fs::write(&source, &bytes).unwrap();

        let inspected = Command::new(env!("CARGO_BIN_EXE_pstoff"))
            .arg("inspect")
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(inspected.status.code(), Some(3), "{name}");
        let stdout = String::from_utf8(inspected.stdout).unwrap();
        assert!(stdout.contains("Support: unsupported"), "{name}: {stdout}");
        assert!(stdout.contains(reason), "{name}: {stdout}");
        assert!(stdout.contains(pstoff_engine::MAILBOX_COMPLETENESS_STATEMENT));

        let converted = Command::new(env!("CARGO_BIN_EXE_pstoff"))
            .arg("convert")
            .arg(&source)
            .arg("--out")
            .arg(&output_dir)
            .output()
            .unwrap();
        assert_eq!(converted.status.code(), Some(3), "{name}");
        assert_eq!(std::fs::read_dir(&output_dir).unwrap().count(), 0, "{name}");
        assert_eq!(
            std::fs::read(&source).unwrap(),
            bytes,
            "{name} source changed"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
