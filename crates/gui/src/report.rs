//! What the Report screen says about each OST, built from the engine's
//! Conversion reports. The completeness statement is the engine's own text.
use crate::format;
use crate::run::{JobState, Outcome};
use crate::selection::folder_path;
use crate::wizard::Wizard;
use pstoff_engine::{
    BatchResult, Conversion, ErrorKind, Evidence, Folder, ItemRef, LocalRef, OutputFile,
    StreamedReport, MAILBOX_COMPLETENESS_STATEMENT,
};
use std::path::PathBuf;

/// Lists longer than this are cut short on screen; the saved report has all of them.
pub const SHOWN: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRow {
    pub label: &'static str,
    pub value: String,
    pub note: &'static str,
    pub references: Vec<String>,
    pub reference_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub path: PathBuf,
    pub size: String,
    pub part: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPartRow {
    pub folder: String,
    pub part: u32,
    pub items: u64,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DifferenceRow {
    pub property: String,
    pub item: String,
    pub source: String,
    pub reason: String,
}

impl std::fmt::Debug for DifferenceRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DifferenceRow")
            .field("property", &self.property)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OstReport {
    pub name: String,
    pub outcome: Outcome,
    /// Why it didn't finish, for cancelled, failed and not-started OSTs.
    pub message: Option<String>,
    pub evidence: Vec<EvidenceRow>,
    pub items: u64,
    pub folders: u64,
    pub files: Vec<FileRow>,
    pub file_count: usize,
    pub folder_part_count: usize,
    /// Shown when the Items went into more than one part.
    pub folder_parts: Vec<FolderPartRow>,
    pub skipped: Vec<(String, String)>,
    pub differences: Vec<DifferenceRow>,
    pub skipped_count: usize,
    pub difference_count: usize,
    pub saved_report: Option<PathBuf>,
}

/// The note that opens the Report and the Convert summary: the engine's text.
pub fn completeness_statement(wizard: &Wizard) -> String {
    wizard
        .run()
        .and_then(|run| {
            run.state().jobs.iter().find_map(|j| match &j.state {
                JobState::Done(BatchResult::Finished {
                    conversion: Conversion::Complete(report),
                    ..
                }) => Some(report.mailbox_completeness_statement.clone()),
                _ => None,
            })
        })
        .unwrap_or_else(|| MAILBOX_COMPLETENESS_STATEMENT.to_string())
}

pub fn build(wizard: &Wizard) -> Vec<OstReport> {
    let Some(run) = wizard.run() else {
        return Vec::new();
    };
    let state = run.state();
    state
        .jobs
        .iter()
        .map(|job| {
            let tree: &[Folder] = wizard
                .source(job.source)
                .map(|s| s.folders())
                .unwrap_or_default();
            let mut out = OstReport {
                name: job.name.clone(),
                outcome: job.outcome(),
                message: None,
                evidence: Vec::new(),
                items: 0,
                folders: 0,
                files: Vec::new(),
                file_count: 0,
                folder_part_count: 0,
                folder_parts: Vec::new(),
                skipped: Vec::new(),
                differences: Vec::new(),
                skipped_count: 0,
                difference_count: 0,
                saved_report: None,
            };
            match &job.state {
                JobState::Done(BatchResult::Finished {
                    conversion: Conversion::Complete(report),
                    ..
                }) => {
                    if fill_complete(&mut out, report, tree).is_err() {
                        out.message = Some("Could not read report records; see the saved report.".into());
                    }
                    let saved = job
                        .options
                        .destination
                        .join(format!("{}.txt", job.options.base_name));
                    out.saved_report = saved.exists().then_some(saved);
                }
                JobState::Done(BatchResult::Finished {
                    conversion: Conversion::Cancelled { completed_files },
                    ..
                }) => {
                    out.message = Some(
                        "Stopped before this PST was finished. The unfinished PST was removed so it can’t be mistaken for a full copy."
                            .into(),
                    );
                    out.files = file_rows(completed_files).unwrap_or_else(|_| {
                        out.message = Some("Could not read finished-part records; inspect the destination folder.".into());
                        Vec::new()
                    });
                    out.file_count = completed_files.len();
                }
                JobState::Done(BatchResult::Failed { error, .. }) => {
                    let hint = match &error.kind {
                        ErrorKind::InUse => " Close Outlook, then convert this OST again.".to_string(),
                        ErrorKind::InsufficientSpace => " Free some space or choose another folder.".to_string(),
                        ErrorKind::Collision => " Nothing was overwritten.".to_string(),
                        ErrorKind::OversizedItem(item) => format!(
                            " {} is larger than the Split limit. Finished parts were kept.",
                            item_label(tree, item)
                        ),
                        ErrorKind::NotImplemented => {
                            " This version of the engine can’t do this yet.".to_string()
                        }
                        _ => String::new(),
                    };
                    out.message = Some(format!("{}{hint}", error.message));
                    out.files = file_rows(&error.completed_files).unwrap_or_else(|_| {
                        out.message = Some(format!("{} Finished-part records could not be read; inspect the destination folder.", error.message));
                        Vec::new()
                    });
                    out.file_count = error.completed_files.len();
                }
                JobState::Done(BatchResult::NotStarted { .. }) => {
                    out.message =
                        Some("This file was waiting in the queue when you cancelled.".into());
                }
                JobState::Waiting | JobState::Running(_) => {}
            }
            out
        })
        .collect()
}

fn fill_complete(
    out: &mut OstReport,
    report: &StreamedReport,
    tree: &[Folder],
) -> std::io::Result<()> {
    out.evidence = vec![
        evidence_row(
            "Headers-only items",
            "Only the header was downloaded; the body and attachments are missing.",
            &report.evidence.headers_only,
            tree,
        )?,
        evidence_row(
            "Unreadable content",
            "Content PstOff couldn’t read from the OST.",
            &report.evidence.unreadable,
            tree,
        )?,
        evidence_row(
            "External attachment references",
            "Links to files kept outside the mailbox. These aren’t missing content.",
            &report.evidence.external_references,
            tree,
        )?,
    ];
    // A disk index counts distinct folders without retaining one owner per folder.
    let (items, folders) = report.summary_counts()?;
    out.items = items;
    out.folders = folders;
    out.files = report
        .files
        .iter()
        .take(SHOWN)
        .map(|f| {
            let f = f?;
            Ok(FileRow {
                path: f.path,
                size: format::bytes(f.bytes),
                part: f.part,
            })
        })
        .collect::<std::io::Result<_>>()?;
    if report.files.len() > 1 {
        out.folder_parts = report
            .folders
            .iter()
            .take(SHOWN)
            .map(|f| {
                let f = f?;
                Ok(FolderPartRow {
                    folder: folder_path(tree, &f.folder),
                    part: f.part,
                    items: f.items,
                })
            })
            .collect::<std::io::Result<_>>()?;
    }
    out.file_count = report.files.len();
    out.folder_part_count = report.folders.len();
    out.skipped_count = report.skipped.len();
    out.difference_count = report.differences.len();
    out.skipped = report
        .skipped
        .iter()
        .take(SHOWN)
        .map(|s| {
            let s = s?;
            let what = match &s.reference {
                LocalRef::Folder(folder) => format!("Folder {}", folder_path(tree, folder)),
                LocalRef::Item(item) => item_label(tree, item),
            };
            Ok((what, s.reason.clone()))
        })
        .collect::<std::io::Result<_>>()?;
    out.differences = report
        .differences
        .iter()
        .take(SHOWN)
        .map(|d| {
            let d = d?;
            Ok(DifferenceRow {
                property: format!("0x{:08x}", d.property),
                item: d
                    .item
                    .as_ref()
                    .map(|i| item_label(tree, i))
                    .unwrap_or_else(|| "Store".into()),
                source: d.source.clone(),
                reason: d.reason.clone(),
            })
        })
        .collect::<std::io::Result<_>>()?;
    Ok(())
}

fn evidence_row(
    label: &'static str,
    note: &'static str,
    evidence: &Evidence<pstoff_engine::Records<ItemRef>>,
    tree: &[Folder],
) -> std::io::Result<EvidenceRow> {
    Ok(match evidence {
        Evidence::NotChecked => EvidenceRow {
            label,
            value: "Not checked".into(),
            note,
            references: Vec::new(),
            reference_count: 0,
        },
        Evidence::Incomplete {
            count,
            references,
            unchecked,
        } => EvidenceRow {
            label,
            value: format!("{} found; {} not checked", count, unchecked.len()),
            note,
            reference_count: references.len() + unchecked.len(),
            references: references
                .iter()
                .map(|r| r.map(|r| item_label(tree, &r)))
                .chain(
                    unchecked
                        .iter()
                        .map(|r| r.map(|r| format!("Not checked: {}", item_label(tree, &r)))),
                )
                .take(SHOWN)
                .collect::<std::io::Result<_>>()?,
        },
        Evidence::Checked { count, references } => EvidenceRow {
            label,
            value: if *count == 0 {
                "None found".into()
            } else {
                format::plural(*count, "item", "items")
            },
            note,
            reference_count: references.len(),
            references: references
                .iter()
                .take(SHOWN)
                .map(|r| r.map(|r| item_label(tree, &r)))
                .collect::<std::io::Result<_>>()?,
        },
    })
}

/// Local references only: node ID and folder path, never subjects or addresses.
pub fn item_label(tree: &[Folder], item: &ItemRef) -> String {
    format!(
        "Item 0x{:x} in {}",
        item.node,
        folder_path(tree, &item.folder)
    )
}

fn file_rows(files: &pstoff_engine::Records<OutputFile>) -> std::io::Result<Vec<FileRow>> {
    files
        .iter()
        .take(SHOWN)
        .map(|f| {
            let f = f?;
            Ok(FileRow {
                path: f.path,
                size: format::bytes(f.bytes),
                part: f.part,
            })
        })
        .collect()
}
