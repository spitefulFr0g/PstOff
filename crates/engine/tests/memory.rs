//! Issue #32 process-memory gate. Each phase is a freshly created guest process.
#![cfg(windows)]
#[allow(dead_code)]
mod support;
#[allow(dead_code)]
#[path = "support/synthetic.rs"]
mod synthetic;
use pstoff_engine::{CancelToken, Conversion, ConvertOptions, Split};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn hash(path: &Path) -> String {
    let mut file = File::open(path).unwrap();
    let mut digest = Sha256::new();
    let mut b = [0; 65536];
    loop {
        let n = file.read(&mut b).unwrap();
        if n == 0 {
            break;
        }
        digest.update(&b[..n]);
    }
    format!("{:x}", digest.finalize())
}
#[repr(C)]
#[derive(Default)]
struct Counters {
    cb: u32,
    faults: u32,
    peak_working: usize,
    working: usize,
    peak_paged: usize,
    paged: usize,
    peak_nonpaged: usize,
    nonpaged: usize,
    pagefile: usize,
    peak_pagefile: usize,
    private: usize,
}
#[link(name = "psapi")]
extern "system" {
    fn GetProcessMemoryInfo(process: isize, counters: *mut Counters, size: u32) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
}
fn peak_commit() -> u64 {
    let mut counters = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        ..Default::default()
    };
    // SAFETY: the OS pseudo-handle is valid for this process; the writable structure
    // has the documented PROCESS_MEMORY_COUNTERS_EX layout and cb/size agree.
    assert_ne!(
        unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                &mut counters,
                std::mem::size_of::<Counters>() as u32,
            )
        },
        0
    );
    counters.peak_pagefile as u64
}
fn phase(root: &Path, mode: &str, count: u32, length: usize) {
    eprintln!("ISSUE32_PHASE {mode} items={count}");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "isolated_phase",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("PSTOFF_MEMORY_PHASE", mode)
        .env("PSTOFF_MEMORY_ROOT", root)
        .env("PSTOFF_MEMORY_COUNT", count.to_string())
        .env("PSTOFF_MEMORY_LENGTH", length.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(600);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{mode} timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{mode}: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn isolated_phase() {
    let Some(mode) = std::env::var_os("PSTOFF_MEMORY_PHASE") else {
        return;
    };
    let root = std::path::PathBuf::from(std::env::var_os("PSTOFF_MEMORY_ROOT").unwrap());
    let count: u32 = std::env::var("PSTOFF_MEMORY_COUNT")
        .unwrap()
        .parse()
        .unwrap();
    let length: usize = std::env::var("PSTOFF_MEMORY_LENGTH")
        .unwrap()
        .parse()
        .unwrap();
    let source = root.join("generated.ost");
    let output = root.join("out");
    match mode.to_str().unwrap() {
        "calibrate" => {
            let bytes = vec![0x5au8; 32 * 1024 * 1024];
            std::hint::black_box(&bytes);
            assert!(
                peak_commit() >= 32 * 1024 * 1024,
                "OS peak must include touched private allocation"
            );
            drop(bytes);
            let peak = peak_commit();
            assert!(
                peak >= 32 * 1024 * 1024,
                "OS peak must survive allocation release"
            );
            std::fs::write(root.join("control.json"), serde_json::to_vec(&serde_json::json!({
                "touched_allocation_bytes": 32 * 1024 * 1024u64, "peak_private_commit_bytes": peak
            })).unwrap()).unwrap();
        }
        "generate" => {
            synthetic::large::generate(&source, count, length);
            std::fs::write(root.join("source.sha256"), hash(&source)).unwrap();
        }
        "convert" => {
            let mut options = ConvertOptions::new(output, "generated".into());
            options.split = Split::Off;
            let mut fraction = 0.0;
            let mut done = 0;
            let result = pstoff_engine::convert(
                &source,
                &options,
                &mut |p: pstoff_engine::Progress| {
                    assert!(p.fraction >= fraction && p.fraction <= 1.0);
                    assert!(p.done >= done && p.done <= p.total);
                    fraction = p.fraction;
                    done = p.done;
                },
                &CancelToken::default(),
            )
            .unwrap();
            let Conversion::Complete(report) = result else {
                panic!("conversion cancelled")
            };
            assert_eq!(fraction, 1.0);
            assert_eq!(done, u64::from(count));
            assert!(report.skipped.is_empty());
            assert_eq!(report.summary_counts().unwrap().0, u64::from(count));
            let peak = peak_commit();
            std::fs::write(root.join("peak.json"), serde_json::to_vec(&serde_json::json!({
                "items": count, "item_payload_bytes": length, "logical_payload_bytes": u64::from(count) * length as u64,
                "peak_private_commit_bytes": peak, "metric": "GetProcessMemoryInfo.PeakPagefileUsage",
                "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
                "source_sha256": report.source_sha256,
                "source_bytes": std::fs::metadata(&source).unwrap().len(),
                "output_bytes": std::fs::metadata(root.join("out/generated.pst")).unwrap().len()
            })).unwrap()).unwrap();
        }
        "cancel" => {
            for boundary in 0..4 {
                let cancel = CancelToken::default();
                if boundary == 0 {
                    cancel.cancel();
                }
                let destination = root.join(format!("cancel-{boundary}"));
                let mut options = ConvertOptions::new(destination.clone(), "generated".into());
                options.split = Split::Off;
                let result = pstoff_engine::convert(
                    &source,
                    &options,
                    &mut |p: pstoff_engine::Progress| {
                        if (boundary == 1 && p.done >= u64::from(count / 2))
                            || (boundary == 2 && p.done == p.total && p.fraction < 0.95)
                            || (boundary == 3 && p.fraction == 0.95)
                        {
                            cancel.cancel();
                        }
                    },
                    &cancel,
                )
                .unwrap();
                assert!(
                    matches!(result, Conversion::Cancelled { completed_files } if completed_files.is_empty())
                );
                if destination.exists() {
                    assert_eq!(std::fs::read_dir(destination).unwrap().count(), 0);
                }
                let prefix = format!("pstoff-table-{}-", std::process::id());
                assert!(
                    !std::fs::read_dir(std::env::temp_dir()).unwrap().any(|p| p
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(&prefix)),
                    "cancelled conversion retained a staging spool"
                );
            }
            assert_eq!(
                std::fs::read_to_string(root.join("source.sha256")).unwrap(),
                hash(&source)
            );
        }
        "verify" => {
            use outlook_pst::ltp::prop_context::PropertyValue;
            assert_eq!(
                std::fs::read_to_string(root.join("source.sha256")).unwrap(),
                hash(&source)
            );
            let report: pstoff_engine::StreamedReport =
                serde_json::from_reader(File::open(root.join("out/generated.json")).unwrap())
                    .unwrap();
            assert_eq!(report.source_sha256, hash(&source));
            let store = outlook_pst::open_store(output.join("generated.pst")).unwrap();
            let mut reader =
                pstoff_format::ndb::Pst::from_file(File::open(&source).unwrap()).unwrap();
            let nodes = reader.nodes();
            assert!(reader.warnings.is_empty());
            assert_eq!(
                nodes.iter().filter(|n| n.nid_type() == 4).count(),
                count as usize
            );
            for group in 0..count.div_ceil(64) {
                let nid = 0x1c2 + group * 32;
                let node = nodes.iter().find(|n| n.nid == (nid & !31 | 0xe)).unwrap();
                let table =
                    pstoff_format::ltp::read_tc(&mut reader, node.bid_data, node.bid_sub).unwrap();
                let expected = count.min((group + 1) * 64) - group * 64;
                assert_eq!(table.rows.len(), expected as usize);
                for (ordinal, row) in table.rows.iter().enumerate() {
                    assert_eq!(row.id, 0x10004 + (group * 64 + ordinal as u32) * 32);
                }
                let id = store.properties().make_entry_id(nid.into()).unwrap();
                let folder = store.open_folder(&id).unwrap();
                assert_eq!(
                    folder.properties().content_count().unwrap() as u32,
                    expected
                );
                let actual = folder.contents_table().unwrap();
                assert_eq!(actual.rows_matrix().count(), expected as usize);
                for (ordinal, row) in actual.rows_matrix().enumerate() {
                    assert_eq!(
                        u32::from(row.id()),
                        0x10004 + (group * 64 + ordinal as u32) * 32
                    );
                }
            }
            for i in 0..count {
                let nid = 0x10004 + i * 32;
                let node = nodes.iter().find(|n| n.nid == nid).unwrap();
                let pc = pstoff_format::ltp::read_node_pc(&mut reader, node).unwrap();
                let pstoff_format::ltp::Value::Bytes(bytes) = &pc.props[&0x6801] else {
                    panic!("missing source payload")
                };
                assert_eq!(bytes.len(), length);
                assert!(bytes
                    .iter()
                    .enumerate()
                    .all(|(at, b)| *b == synthetic::large::payload_byte(i, at)));
                let id = store.properties().make_entry_id(nid.into()).unwrap();
                let message = store.open_message(&id, None).unwrap();
                let Some(PropertyValue::Binary(value)) = message.properties().get(0x6801) else {
                    panic!("missing output payload")
                };
                assert_eq!(value.buffer(), bytes.as_slice());
            }
        }
        _ => panic!("unknown phase"),
    }
}
#[test]
fn large_generated_ost_has_fixed_separate_process_peak() {
    // Initial Windows Sandbox debug sweep peaked at 1,601,536 bytes for 1 GiB
    // logical payload. 8 MiB gives >5x headroom while rejecting retained corpus
    // payloads. This is a corpus regression gate, not the shared-budget contract.
    const CEILING_BYTES: u64 = 8 * 1024 * 1024;
    let control = support::Fixture::invalid();
    phase(&control.root, "calibrate", 0, 0);
    let control: serde_json::Value =
        serde_json::from_reader(File::open(control.root.join("control.json")).unwrap()).unwrap();
    let rustc = Command::new("rustc").arg("-Vv").output().unwrap();
    assert!(rustc.status.success());
    let mut samples = Vec::new();
    let destination = std::path::PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR").expect("run through the approved Sandbox wrapper"),
    )
    .join("issue32-memory")
    .join(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string(),
    );
    std::fs::create_dir_all(&destination).unwrap();
    for count in [16, 256, 1024, 4096] {
        let fixture = support::Fixture::invalid();
        phase(&fixture.root, "generate", count, 256 * 1024);
        phase(&fixture.root, "convert", count, 256 * 1024);
        phase(&fixture.root, "verify", count, 256 * 1024);
        if count == 16 {
            phase(&fixture.root, "cancel", count, 256 * 1024);
        }
        let data: serde_json::Value =
            serde_json::from_reader(File::open(fixture.root.join("peak.json")).unwrap()).unwrap();
        eprintln!("ISSUE32_MEMORY {data}");
        samples.push(data.clone());
        std::fs::write(destination.join("evidence.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "samples": samples, "expected_case_count": 4, "complete": samples.len() == 4, "control": control, "rustc": String::from_utf8(rustc.stdout.clone()).unwrap(),
            "cargo_lock_sha256": format!("{:x}", Sha256::digest(include_bytes!("../../../Cargo.lock"))),
            "ceiling_bytes": CEILING_BYTES, "payloads_verified": true, "source_immutability_verified": true
        })).unwrap()).unwrap();
        assert!(
            data["peak_private_commit_bytes"].as_u64().unwrap() <= CEILING_BYTES,
            "{data}"
        );
    }
}
