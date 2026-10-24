//! Platform-neutral public seam shared by both frontends.
//! Format readers, writers and conversion policy are private implementation details.
mod reader;
mod writer;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub const DEFAULT_SPLIT_BYTES: u64 = 21_474_836_480;
pub const REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FolderRef(pub u32);
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemRef {
    pub folder: FolderRef,
    pub node: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnsupportedReason {
    InvalidHeader,
    Ansi,
    OlderUnicode(u16),
    UnknownVersion(u16),
    Encoding(u8),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Support {
    Supported,
    Unsupported(UnsupportedReason),
    InUse,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderRef,
    pub name: String,
    pub item_count: u64,
    pub bytes: u64,
    pub unreadable: Option<String>,
    pub children: Vec<Folder>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inspection {
    pub source: PathBuf,
    pub support: Support,
    pub folders: Vec<Folder>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Split {
    Off,
    Bytes(u64),
}
impl Default for Split {
    fn default() -> Self {
        Self::Bytes(DEFAULT_SPLIT_BYTES)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertOptions {
    pub destination: PathBuf,
    pub base_name: String,
    pub exclusions: Vec<FolderRef>,
    pub split: Split,
}
impl ConvertOptions {
    pub fn new(destination: PathBuf, base_name: String) -> Self {
        Self {
            destination,
            base_name,
            exclusions: Vec::new(),
            split: Split::default(),
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);
impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    /// A value between zero and one, including finalization work.
    pub fraction: f64,
    pub current_folder: Option<FolderRef>,
    pub done: u64,
    pub total: u64,
    pub part: u32,
}
pub trait ProgressSink {
    fn update(&mut self, progress: Progress);
}
impl<F: FnMut(Progress)> ProgressSink for F {
    fn update(&mut self, p: Progress) {
        self(p);
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub part: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderPart {
    pub folder: FolderRef,
    pub part: u32,
    pub items: u64,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LocalRef {
    Folder(FolderRef),
    Item(ItemRef),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skipped {
    pub reference: LocalRef,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Difference {
    pub property: u32,
    pub item: Option<ItemRef>,
    pub source: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Evidence {
    NotChecked,
    Checked {
        count: u64,
        references: Vec<ItemRef>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletenessEvidence {
    pub headers_only: Evidence,
    pub unreadable: Evidence,
    pub external_references: Evidence,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailboxCompleteness {
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionReport {
    pub schema_version: u32,
    pub mailbox_completeness: MailboxCompleteness,
    pub source: PathBuf,
    pub source_sha256: String,
    pub files: Vec<OutputFile>,
    pub folders: Vec<FolderPart>,
    pub skipped: Vec<Skipped>,
    pub differences: Vec<Difference>,
    pub evidence: CompletenessEvidence,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorKind {
    Unsupported(UnsupportedReason),
    InUse,
    Io,
    InvalidOptions,
    InsufficientSpace,
    OversizedItem(ItemRef),
    Collision,
    DamagedSource,
    NotImplemented,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineError {
    pub kind: ErrorKind,
    pub message: String,
    pub completed_files: Vec<OutputFile>,
}
impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for EngineError {}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Conversion {
    Complete(Box<ConversionReport>),
    Cancelled { completed_files: Vec<OutputFile> },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchResult {
    Finished {
        source: PathBuf,
        conversion: Conversion,
    },
    Failed {
        source: PathBuf,
        error: EngineError,
    },
    NotStarted {
        source: PathBuf,
    },
}

/// Inspect a supported v36 OST without modifying the source.
/// Unsupported formats and Windows sharing violations are explicit store outcomes.
pub fn inspect(path: impl AsRef<Path>) -> Result<Inspection, EngineError> {
    reader::inspect(path.as_ref())
}
/// Convert through the shared policy layer. Never reports success before a writer exists.
pub fn convert(
    path: impl AsRef<Path>,
    options: &ConvertOptions,
    progress: &mut dyn ProgressSink,
    cancel: &CancelToken,
) -> Result<Conversion, EngineError> {
    if cancel.is_cancelled() {
        return Ok(Conversion::Cancelled {
            completed_files: Vec::new(),
        });
    }
    let path = path.as_ref();
    let (mut source, nodes, source_sha256) = reader::conversion_source(path)?;
    let failure = |kind, message: String| EngineError {
        kind,
        message,
        completed_files: vec![],
    };
    if options.base_name.is_empty()
        || options.base_name.contains(['/', '\\'])
        || options.base_name == "."
        || options.base_name == ".."
    {
        return Err(failure(
            ErrorKind::InvalidOptions,
            "Choose a plain output base name.".into(),
        ));
    }
    if !options.exclusions.is_empty() || options.split != Split::Off {
        return Err(failure(
            ErrorKind::NotImplemented,
            "Split and exclusions require the conversion planner.".into(),
        ));
    }
    let output = options
        .destination
        .join(format!("{}.pst", options.base_name));
    if output.exists() {
        return Err(failure(
            ErrorKind::Collision,
            "The output already exists.".into(),
        ));
    }
    let temporary = options
        .destination
        .join(format!(".{}.pstoff-partial", options.base_name));
    let display = format!(
        "PstOff – {}",
        path.file_stem().unwrap_or_default().to_string_lossy()
    );
    let differences = writer::write(&mut source, &nodes, &temporary, &display).map_err(|e| {
        // A failed create_new must never delete a pre-existing temporary.
        failure(ErrorKind::Io, e.to_string())
    })?;
    if cancel.is_cancelled() {
        let _ = std::fs::remove_file(&temporary);
        return Ok(Conversion::Cancelled {
            completed_files: vec![],
        });
    }
    let bytes = std::fs::metadata(&temporary)
        .map_err(|e| failure(ErrorKind::Io, e.to_string()))?
        .len();
    let publish = std::fs::hard_link(&temporary, &output);
    let _ = std::fs::remove_file(&temporary);
    publish.map_err(|e| {
        failure(
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                ErrorKind::Collision
            } else {
                ErrorKind::Io
            },
            e.to_string(),
        )
    })?;
    progress.update(Progress {
        fraction: 1.0,
        current_folder: None,
        done: 0,
        total: 0,
        part: 1,
    });
    Ok(Conversion::Complete(Box::new(ConversionReport {
        schema_version: REPORT_SCHEMA_VERSION,
        mailbox_completeness: MailboxCompleteness::Unknown,
        source: path.into(),
        source_sha256,
        files: vec![OutputFile {
            path: output,
            bytes,
            part: 1,
        }],
        folders: nodes
            .iter()
            .filter(|n| n.nid_type() == 4 || n.nid_type() == 8)
            .fold(
                std::collections::BTreeMap::<u32, FolderPart>::new(),
                |mut all, n| {
                    let folder = all.entry(n.nid_parent).or_insert(FolderPart {
                        folder: FolderRef(n.nid_parent),
                        part: 1,
                        items: 0,
                        bytes: 0,
                    });
                    folder.items += 1;
                    if let Ok(pc) = pstoff_format::ltp::read_node_pc(&mut source, n) {
                        if let Some(size) = pc.int(0x0e08).filter(|s| *s >= 0) {
                            folder.bytes = folder.bytes.saturating_add(size as u64);
                        }
                    }
                    all
                },
            )
            .into_values()
            .collect(),
        skipped: vec![],
        differences,
        evidence: CompletenessEvidence {
            headers_only: Evidence::NotChecked,
            unreadable: Evidence::NotChecked,
            external_references: Evidence::NotChecked,
        },
    })))
}
