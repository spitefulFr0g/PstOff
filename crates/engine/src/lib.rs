//! Platform-neutral public seam shared by both frontends.
//! Format readers, writers and conversion policy are private implementation details.
mod planner;
mod reader;
mod writer;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub const DEFAULT_SPLIT_BYTES: u64 = 21_474_836_480;
pub const REPORT_SCHEMA_VERSION: u32 = 3;
pub const OUTLOOK_DEFAULT_LIMIT_BYTES: u64 = 50 * 1024 * 1024 * 1024;
/// An OST records cached contents, not the server mailbox's full history.
pub const MAILBOX_COMPLETENESS_STATEMENT: &str = "Mailbox completeness: unknown. An OST holds only what Outlook synced. Items or folders that were never synced cannot be recovered from this file.";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FolderRef(pub u32);
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemRef {
    pub folder: FolderRef,
    pub node: u32,
    #[serde(default)]
    pub folder_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderPath {
    pub folder: FolderRef,
    pub path: String,
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
    #[serde(default = "completeness_statement")]
    pub mailbox_completeness_statement: String,
}
fn completeness_statement() -> String {
    MAILBOX_COMPLETENESS_STATEMENT.into()
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvertOptions {
    pub destination: PathBuf,
    pub base_name: String,
    pub exclusions: Vec<FolderRef>,
    pub split: Split,
    /// Extra destination headroom in addition to the estimated PST size.
    #[serde(default = "default_space_reserve")]
    pub space_reserve_bytes: u64,
}
fn default_space_reserve() -> u64 {
    64 * 1024 * 1024
}
impl ConvertOptions {
    pub fn new(destination: PathBuf, base_name: String) -> Self {
        Self {
            destination,
            base_name,
            exclusions: Vec::new(),
            split: Split::default(),
            space_reserve_bytes: default_space_reserve(),
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
pub struct PlannedFile {
    pub path: PathBuf,
    pub estimated_bytes: u64,
    pub part: u32,
    pub items: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputPlan {
    pub files: Vec<PlannedFile>,
    /// Absolute per-file tolerance for an unchanged source and options.
    pub size_tolerance_bytes: u64,
    pub over_outlook_limit: bool,
}
impl OutputPlan {
    pub fn new(files: Vec<PlannedFile>) -> Self {
        let over_outlook_limit = files
            .iter()
            .any(|f| f.estimated_bytes > OUTLOOK_DEFAULT_LIMIT_BYTES);
        Self {
            files,
            size_tolerance_bytes: 0,
            over_outlook_limit,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderPart {
    pub folder: FolderRef,
    #[serde(default)]
    pub folder_path: String,
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
pub enum SkipReason {
    UnreadableItem,
    UnreadableFolder,
    UnreadableAncestor,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skipped {
    pub code: SkipReason,
    pub reference: LocalRef,
    #[serde(default)]
    pub folder_path: String,
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
    /// Positive findings remain useful, but damaged Items were not checked.
    Incomplete {
        count: u64,
        references: Vec<ItemRef>,
        unchecked: Vec<ItemRef>,
    },
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
    pub mailbox_completeness_statement: String,
    pub source: PathBuf,
    pub source_sha256: String,
    pub files: Vec<OutputFile>,
    pub folders: Vec<FolderPart>,
    pub skipped: Vec<Skipped>,
    pub differences: Vec<Difference>,
    pub evidence: CompletenessEvidence,
    /// Explicit selection only; descendant exclusions are implied by policy.
    #[serde(default)]
    pub excluded_folders: Vec<FolderPath>,
    /// Paths for every local folder reference, including skipped/recovered ones.
    #[serde(default)]
    pub folder_paths: Vec<FolderPath>,
}
impl ConversionReport {
    pub fn folder_path(&self, folder: &FolderRef) -> String {
        // Engine reports are sorted by ID. Keep a fallback for callers loading
        // or constructing a report whose lookup is in a different order.
        self.folder_paths
            .binary_search_by_key(&folder.0, |p| p.folder.0)
            .ok()
            .map(|index| &self.folder_paths[index])
            .or_else(|| self.folder_paths.iter().find(|p| &p.folder == folder))
            .map(|p| p.path.clone())
            .unwrap_or_else(|| format!("Folder 0x{:x}", folder.0))
    }
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
/// Read and measure the conversion layout without creating any files or folders.
/// Payloads are decoded for layout measurement; this can take as long as conversion.
/// Recomputed at conversion time, so edits to the source invalidate an earlier plan.
pub fn plan(path: impl AsRef<Path>, options: &ConvertOptions) -> Result<OutputPlan, EngineError> {
    conversion::plan(path.as_ref(), options)
}
/// Convert a supported OST in Copy mode through the public policy boundary.
pub fn convert(
    path: impl AsRef<Path>,
    options: &ConvertOptions,
    progress: &mut dyn ProgressSink,
    cancel: &CancelToken,
) -> Result<Conversion, EngineError> {
    conversion::convert(path.as_ref(), options, progress, cancel)
}
mod conversion;
