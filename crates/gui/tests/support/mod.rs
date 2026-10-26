//! Generated fixtures only: synthetic OSTs from the engine's test builder,
//! written into a fresh temp folder per test. No real mail is ever read.
#![allow(dead_code)]
#[path = "../../../engine/tests/support/synthetic.rs"]
pub mod synthetic;

use pstoff_engine::{
    CancelToken, CompletenessEvidence, Conversion, ConversionReport, ConvertOptions, Difference,
    EngineError, Evidence, FolderPart, Inspection, ItemRef, LocalRef, MailboxCompleteness,
    OutputFile, Progress, ProgressSink, Skipped, MAILBOX_COMPLETENESS_STATEMENT,
};
use pstoff_gui::{Engine, RealEngine};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use synthetic::{Folder, Item, Store, SystemFolder};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Temp {
    pub root: PathBuf,
}

impl Temp {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "pstoff-gui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    pub fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    pub fn out(&self) -> PathBuf {
        self.root.join("out")
    }

    /// Every file under the output folder, relative to it.
    pub fn outputs(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.out()) {
            for entry in entries.flatten() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        names.sort();
        names
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn folder(name: &str, items: Vec<Item>, children: Vec<Folder>) -> Folder {
    Folder {
        name: name.into(),
        items,
        children,
        ..Default::default()
    }
}

fn notes(n: u32, size: u32) -> Vec<Item> {
    (0..n).map(|_| Item::new("IPM.Note", size)).collect()
}

/// Root → Root - Mailbox → {Finder, IPM_SUBTREE → {Inbox → Projects, Sent Items,
/// Deleted Items}}. The writer needs Finder and Deleted Items.
pub fn mailbox() -> Vec<u8> {
    let mut store = Store {
        root: folder(
            "Root",
            vec![],
            vec![folder(
                "Root - Mailbox",
                vec![],
                vec![
                    folder("Finder", vec![], vec![]),
                    folder(
                        "IPM_SUBTREE",
                        vec![],
                        vec![
                            folder(
                                "Inbox",
                                notes(3, 1000),
                                vec![folder("Projects", notes(2, 500), vec![])],
                            ),
                            folder("Sent Items", notes(1, 700), vec![]),
                            folder("Deleted Items", vec![], vec![]),
                        ],
                    ),
                ],
            )],
        ),
    };
    store.root.children[0].children[0].role = Some(SystemFolder::Finder);
    let subtree = &mut store.root.children[0].children[1];
    subtree.role = Some(SystemFolder::IpmSubtree);
    subtree.children[2].role = Some(SystemFolder::Wastebasket);
    store.build()
}

/// A mailbox with one folder whose table can't be read.
pub fn damaged_folder() -> Vec<u8> {
    let mut broken = folder("Broken", notes(1, 100), vec![]);
    broken.unreadable = true;
    let mut store = Store {
        root: folder(
            "Root",
            vec![],
            vec![folder(
                "Root - Mailbox",
                vec![],
                vec![folder("IPM_SUBTREE", vec![], vec![broken])],
            )],
        ),
    };
    store.root.children[0].children[0].role = Some(SystemFolder::IpmSubtree);
    store.build()
}

fn put16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
// Independent checksum, as in the engine's reader tests.
fn crc(b: &[u8]) -> u32 {
    let mut c = 0u32;
    for &v in b {
        c ^= v as u32;
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    c
}

/// A sealed header of another OST version or encoding.
pub fn unsupported(version: u16, encoding: u8) -> Vec<u8> {
    let mut b = vec![0; 564];
    b[..4].copy_from_slice(b"!BDN");
    b[8..10].copy_from_slice(b"SO");
    put16(&mut b, 10, version);
    b[512] = 0x80;
    b[513] = encoding;
    b[248] = 1;
    let header = crc(&b[8..479]);
    put32(&mut b, 4, header);
    let partial = crc(&b[8..524]);
    put32(&mut b, 524, partial);
    b
}

/// Holds a file open without sharing, as Outlook holds its OST.
#[cfg(windows)]
pub fn hold_exclusively(path: &Path) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap()
}

/// Lets a test hold a real conversion part-way and release it later.
#[derive(Clone, Default)]
pub struct Gate {
    state: Arc<(Mutex<GateState>, Condvar)>,
}

#[derive(Default)]
struct GateState {
    waiting: u32,
    released: bool,
}

impl Gate {
    pub fn release(&self) {
        let (lock, cv) = &*self.state;
        lock.lock().unwrap().released = true;
        cv.notify_all();
    }

    /// Waits until a conversion is held at the gate.
    pub fn wait_until_held(&self) {
        let (lock, cv) = &*self.state;
        let guard = lock.lock().unwrap();
        let (_guard, timeout) = cv
            .wait_timeout_while(guard, std::time::Duration::from_secs(60), |s| {
                s.waiting == 0
            })
            .unwrap();
        assert!(!timeout.timed_out(), "no conversion reached the gate");
    }

    fn hold(&self) {
        let (lock, cv) = &*self.state;
        let mut guard = lock.lock().unwrap();
        guard.waiting += 1;
        cv.notify_all();
        let _guard = cv.wait_while(guard, |s| !s.released).unwrap();
    }
}

/// Holds the `hold_on`-th conversion (0-based) at its first progress event.
pub struct GatedEngine {
    pub gate: Gate,
    pub hold_on: usize,
    started: AtomicU64,
}

impl GatedEngine {
    pub fn new(gate: Gate, hold_on: usize) -> Self {
        Self {
            gate,
            hold_on,
            started: AtomicU64::new(0),
        }
    }
}

impl Engine for GatedEngine {
    fn inspect(&self, path: &Path) -> Result<Inspection, EngineError> {
        RealEngine.inspect(path)
    }
    fn convert(
        &self,
        path: &Path,
        options: &ConvertOptions,
        progress: &mut dyn ProgressSink,
        cancel: &CancelToken,
    ) -> Result<Conversion, EngineError> {
        let index = self.started.fetch_add(1, Ordering::SeqCst) as usize;
        let gate = self.gate.clone();
        let mut held = index != self.hold_on;
        let mut sink = |p: Progress| {
            progress.update(p);
            if !held {
                held = true;
                gate.hold();
            }
        };
        RealEngine.convert(path, options, &mut sink, cancel)
    }
}

/// Records the options each `convert` call received, then converts for real.
#[derive(Default)]
pub struct RecordingEngine {
    pub calls: Mutex<Vec<(PathBuf, ConvertOptions)>>,
}

impl Engine for RecordingEngine {
    fn inspect(&self, path: &Path) -> Result<Inspection, EngineError> {
        RealEngine.inspect(path)
    }
    fn convert(
        &self,
        path: &Path,
        options: &ConvertOptions,
        progress: &mut dyn ProgressSink,
        cancel: &CancelToken,
    ) -> Result<Conversion, EngineError> {
        self.calls
            .lock()
            .unwrap()
            .push((path.to_path_buf(), options.clone()));
        RealEngine.convert(path, options, progress, cancel)
    }
}

/// Ready-made engine handles.
pub fn real() -> Arc<dyn Engine> {
    Arc::new(RealEngine)
}

/// Inspects for real, but answers `convert` with prepared results: report
/// builder fixtures covering Split parts, Skipped items and evidence counts.
#[derive(Default)]
pub struct ScriptedEngine {
    pub results: Mutex<std::collections::VecDeque<Result<Conversion, EngineError>>>,
}

impl Engine for ScriptedEngine {
    fn inspect(&self, path: &Path) -> Result<Inspection, EngineError> {
        RealEngine.inspect(path)
    }
    fn convert(
        &self,
        _: &Path,
        _: &ConvertOptions,
        progress: &mut dyn ProgressSink,
        _: &CancelToken,
    ) -> Result<Conversion, EngineError> {
        progress.update(Progress {
            fraction: 1.0,
            current_folder: None,
            done: 0,
            total: 0,
            part: 1,
        });
        self.results
            .lock()
            .unwrap()
            .pop_front()
            .expect("a scripted result")
    }
}

/// The folder with this name, searching depth first.
pub fn folder_named(tree: &[pstoff_engine::Folder], name: &str) -> pstoff_engine::FolderRef {
    fn find(tree: &[pstoff_engine::Folder], name: &str) -> Option<pstoff_engine::FolderRef> {
        tree.iter().find_map(|f| {
            if f.name == name {
                Some(f.id.clone())
            } else {
                find(&f.children, name)
            }
        })
    }
    find(tree, name).unwrap_or_else(|| panic!("no folder named {name}"))
}

pub fn item(folder: u32, node: u32) -> ItemRef {
    ItemRef {
        folder: pstoff_engine::FolderRef(folder),
        node,
    }
}

/// A report shaped like what Split, Skipped items and the completeness
/// checks will produce, for folders the real engine inspected.
pub fn builder_report(t: &Temp, inbox: u32, sent: u32) -> ConversionReport {
    ConversionReport {
        schema_version: pstoff_engine::REPORT_SCHEMA_VERSION,
        mailbox_completeness: MailboxCompleteness::Unknown,
        mailbox_completeness_statement: MAILBOX_COMPLETENESS_STATEMENT.into(),
        source: t.root.join("m.ost"),
        source_sha256: "00".repeat(32),
        files: vec![
            OutputFile {
                path: t.out().join("m-part001.pst"),
                bytes: 3 * 1024 * 1024,
                part: 1,
            },
            OutputFile {
                path: t.out().join("m-part002.pst"),
                bytes: 1024 * 1024,
                part: 2,
            },
        ],
        folders: vec![
            FolderPart {
                folder: pstoff_engine::FolderRef(inbox),
                part: 1,
                items: 2,
                bytes: 2000,
            },
            FolderPart {
                folder: pstoff_engine::FolderRef(inbox),
                part: 2,
                items: 1,
                bytes: 1000,
            },
            FolderPart {
                folder: pstoff_engine::FolderRef(sent),
                part: 2,
                items: 1,
                bytes: 700,
            },
        ],
        skipped: vec![
            Skipped {
                code: pstoff_engine::SkipReason::UnreadableItem,
                reference: LocalRef::Item(item(inbox, 0x200024)),
                reason: "The message body could not be read.".into(),
            },
            Skipped {
                code: pstoff_engine::SkipReason::UnreadableFolder,
                reference: LocalRef::Folder(pstoff_engine::FolderRef(sent)),
                reason: "The folder table could not be read.".into(),
            },
        ],
        differences: vec![
            Difference {
                property: 0x3001001f,
                item: None,
                source: "OST file name".into(),
                reason: "PST display name".into(),
            },
            Difference {
                property: 0x30080040,
                item: Some(item(inbox, 0x200044)),
                source: "contents table cache".into(),
                reason: "Q12 missing modification time".into(),
            },
        ],
        evidence: CompletenessEvidence {
            headers_only: Evidence::Checked {
                count: 2,
                references: vec![item(inbox, 0x200064), item(sent, 0x200084)],
            },
            unreadable: Evidence::Checked {
                count: 0,
                references: vec![],
            },
            external_references: Evidence::NotChecked,
        },
    }
}
