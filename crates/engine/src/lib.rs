//! Platform-neutral public seam shared by both frontends.
//! Format readers, writers and conversion policy are private implementation details.
pub mod platform;
mod reader;
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

fn unavailable() -> EngineError {
    EngineError {
        kind: ErrorKind::NotImplemented,
        message:
            "The OST reader and PST writer are not implemented yet; no conversion was performed."
                .into(),
        completed_files: Vec::new(),
    }
}
/// Inspect a supported v36 OST without modifying the source.
/// Unsupported formats and Windows sharing violations are explicit store outcomes.
pub fn inspect(path: impl AsRef<Path>) -> Result<Inspection, EngineError> {
    reader::inspect(path.as_ref())
}
/// Convert through the shared policy layer. Never reports success before a writer exists.
pub fn convert(
    path: impl AsRef<Path>,
    _options: &ConvertOptions,
    _progress: &mut dyn ProgressSink,
    cancel: &CancelToken,
) -> Result<Conversion, EngineError> {
    if cancel.is_cancelled() {
        return Ok(Conversion::Cancelled {
            completed_files: Vec::new(),
        });
    }
    reader::validate(path.as_ref())?;
    Err(unavailable())
}
