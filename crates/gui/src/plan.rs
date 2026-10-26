//! Output choices and the preview of the PST files they produce.
//! The engine supplies whole-Item boundaries and finalized layout sizes.
use crate::format::{self, GIB};
use pstoff_engine::{ConvertOptions, FolderRef, Split, DEFAULT_SPLIT_BYTES};
use std::path::{Path, PathBuf};

/// Outlook's default maximum size for a Unicode PST (50 GB).
pub const OUTLOOK_DEFAULT_LIMIT_BYTES: u64 = pstoff_engine::OUTLOOK_DEFAULT_LIMIT_BYTES;
/// Split size choices in GiB. The middle choice is the engine's default limit.
pub const SPLIT_CHOICES_GIB: [u64; 4] = [5, 10, 20, 40];
pub const DEFAULT_SPLIT_GIB: u64 = DEFAULT_SPLIT_BYTES / GIB;

#[derive(Debug, Clone)]
pub struct OutputSettings {
    pub destination: Option<PathBuf>,
    pub split: bool,
    pub split_gib: u64,
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            destination: None,
            split: false,
            split_gib: DEFAULT_SPLIT_GIB,
        }
    }
}

impl OutputSettings {
    pub fn split_limit(&self) -> Split {
        if self.split {
            Split::Bytes(self.split_gib * GIB)
        } else {
            Split::Off
        }
    }

    pub fn options(&self, base_name: &str, exclusions: Vec<FolderRef>) -> Option<ConvertOptions> {
        let mut options =
            ConvertOptions::new(self.destination.clone()?, base_name.trim().to_string());
        options.exclusions = exclusions;
        options.split = self.split_limit();
        Some(options)
    }

    pub fn describe_split(&self) -> String {
        if self.split {
            format!("On, up to {} GB per file", self.split_gib)
        } else {
            "Off, one PST per OST".into()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub name: String,
    pub estimated_bytes: u64,
    /// A file with this name is already in the destination.
    pub exists: bool,
}

#[derive(Debug, Clone)]
pub struct SourcePlan {
    pub files: Vec<PlannedFile>,
    pub estimated_bytes: u64,
    /// At least one planned PST would pass Outlook's default size limit.
    pub over_outlook_limit: bool,
    pub problem: Option<String>,
}

pub fn plan(
    settings: &OutputSettings,
    base_name: &str,
    measured: Option<&Result<pstoff_engine::OutputPlan, pstoff_engine::EngineError>>,
    taken: &[String],
) -> SourcePlan {
    let base = base_name.trim();
    let destination = settings.destination.as_deref();
    let engine_plan = measured.and_then(|result| result.as_ref().ok());
    let files: Vec<PlannedFile> = engine_plan
        .into_iter()
        .flat_map(|p| &p.files)
        .map(|file| PlannedFile {
            name: file
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            estimated_bytes: file.estimated_bytes,
            exists: file.path.exists(),
        })
        .collect();
    let problem = base_name_problem(base)
        .or_else(|| {
            taken
                .iter()
                .any(|t| t.trim().eq_ignore_ascii_case(base))
                .then(|| format!("Another OST in the queue already uses the name “{base}”."))
        })
        .or_else(|| {
            let reports = destination.is_some_and(|d| {
                ["json", "txt"]
                    .iter()
                    .any(|ext| d.join(format!("{base}.{ext}")).exists())
            });
            (files.iter().any(|f| f.exists) || reports).then(|| {
                format!("Files named “{base}” are already in this folder. PstOff never overwrites them; choose another name or folder.")
            })
        });
    let problem = problem.or_else(|| match measured {
        None => Some("Planning output…".into()),
        Some(Err(error)) => Some(error.message.clone()),
        Some(Ok(_)) => None,
    });
    SourcePlan {
        over_outlook_limit: engine_plan.is_some_and(|p| p.over_outlook_limit),
        estimated_bytes: files.iter().map(|f| f.estimated_bytes).sum(),
        files,
        problem,
    }
}

/// Mirrors the engine's own check so the field can explain itself early.
pub fn base_name_problem(base: &str) -> Option<String> {
    const RESERVED: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    if base.is_empty() {
        Some("Enter a name for the PST file.".into())
    } else if base == "." || base == ".." || base.contains(RESERVED) {
        Some("A file name can’t contain / \\ : * ? \" < > |.".into())
    } else {
        None
    }
}

pub fn over_limit_text(estimated_bytes: u64) -> String {
    format!(
        "This PST would be about {}. Outlook’s default limit for a PST is 50 GB, so a larger file may not open or may stop accepting mail. Turn on Split to stay under it.",
        format::bytes(estimated_bytes)
    )
}

pub fn default_base_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mailbox".into())
}
