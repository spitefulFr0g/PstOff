//! Output choices and the preview of the PST files they produce.
//!
//! The engine has no plan or dry-run call yet (#35), so the preview is the
//! wizard's own estimate from the inspected folder sizes. It names the files the
//! way the spec names them and says the sizes are estimates.
use crate::format::{self, GIB};
use pstoff_engine::{ConvertOptions, FolderRef, Split, DEFAULT_SPLIT_BYTES};
use std::path::{Path, PathBuf};

/// Outlook's default maximum size for a Unicode PST (50 GB).
pub const OUTLOOK_DEFAULT_LIMIT_BYTES: u64 = 50 * GIB;
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
    /// Without Split, the PST would pass Outlook's default size limit.
    pub over_outlook_limit: bool,
    pub problem: Option<String>,
}

pub fn plan(
    settings: &OutputSettings,
    base_name: &str,
    selected_bytes: u64,
    taken: &[String],
) -> SourcePlan {
    let base = base_name.trim();
    let parts = match settings.split_limit() {
        Split::Bytes(limit) if limit > 0 => selected_bytes.div_ceil(limit).max(1),
        _ => 1,
    };
    let names: Vec<String> = if !settings.split {
        vec![format!("{base}.pst")]
    } else {
        (1..=parts)
            .map(|n| format!("{base}-part{n:03}.pst"))
            .collect()
    };
    let destination = settings.destination.as_deref();
    let files: Vec<PlannedFile> = names
        .into_iter()
        .enumerate()
        .map(|(i, name)| PlannedFile {
            exists: destination.is_some_and(|d| d.join(&name).exists()),
            name,
            estimated_bytes: share(selected_bytes, parts, i as u64),
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
    SourcePlan {
        over_outlook_limit: !settings.split && selected_bytes > OUTLOOK_DEFAULT_LIMIT_BYTES,
        estimated_bytes: selected_bytes,
        files,
        problem,
    }
}

fn share(total: u64, parts: u64, index: u64) -> u64 {
    let each = total / parts;
    if index + 1 == parts {
        total - each * (parts - 1)
    } else {
        each
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
