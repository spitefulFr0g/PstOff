//! What the Report screen says about each OST, built from the engine's
//! Conversion reports. The completeness statement is the engine's own text.
use crate::format;
use crate::run::{JobState, Outcome};
use crate::selection::folder_path;
use crate::wizard::Wizard;
use pstoff_engine::{
    BatchResult, Conversion, ConversionReport, ErrorKind, Evidence, Folder, ItemRef, LocalRef,
    OutputFile, MAILBOX_COMPLETENESS_STATEMENT,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifferenceRow {
    pub property: String,
    pub item: String,
    pub source: String,
    pub reason: String,
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
    /// Shown when the Items went into more than one part.
    pub folder_parts: Vec<FolderPartRow>,
    pub skipped: Vec<(String, String)>,
    pub excluded_folders: Vec<String>,
    pub differences: Vec<DifferenceRow>,
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
        .or_else(|| {
            wizard.ready().find_map(|s| {
                s.inspection
                    .as_ref()
                    .map(|i| i.mailbox_completeness_statement.clone())
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
                folder_parts: Vec::new(),
                skipped: Vec::new(),
                excluded_folders: Vec::new(),
                differences: Vec::new(),
                saved_report: None,
            };
            match &job.state {
                JobState::Done(BatchResult::Finished {
                    conversion: Conversion::Complete(report),
                    ..
                }) => {
                    fill_complete(&mut out, report, tree);
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
                    out.files = file_rows(completed_files);
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
                    out.files = file_rows(&error.completed_files);
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

fn fill_complete(out: &mut OstReport, report: &ConversionReport, tree: &[Folder]) {
    out.excluded_folders = report
        .excluded_folders
        .iter()
        .map(|f| f.path.clone())
        .collect();
    out.evidence = vec![
        evidence_row(
            "Headers-only items",
            "Only the header was downloaded; the body and attachments are missing.",
            &report.evidence.headers_only,
            tree,
        ),
        evidence_row(
            "Unreadable content",
            "Content PstOff couldn’t read from the OST.",
            &report.evidence.unreadable,
            tree,
        ),
        evidence_row(
            "External attachment references",
            "Links to files kept outside the mailbox. These aren’t missing content.",
            &report.evidence.external_references,
            tree,
        ),
    ];
    out.items = report.folders.iter().map(|f| f.items).sum();
    let mut folders: Vec<_> = report.folders.iter().map(|f| &f.folder).collect();
    folders.sort_by_key(|f| f.0);
    folders.dedup();
    out.folders = folders.len() as u64;
    out.files = file_rows(&report.files);
    let parts: std::collections::BTreeSet<u32> = report.files.iter().map(|f| f.part).collect();
    if parts.len() > 1 {
        out.folder_parts = report
            .folders
            .iter()
            .map(|f| FolderPartRow {
                folder: if f.folder_path.is_empty() {
                    folder_path(tree, &f.folder)
                } else {
                    f.folder_path.clone()
                },
                part: f.part,
                items: f.items,
            })
            .collect();
    }
    out.skipped = report
        .skipped
        .iter()
        .map(|s| {
            let what = match &s.reference {
                LocalRef::Folder(folder) => format!(
                    "Folder {}",
                    if s.folder_path.is_empty() {
                        folder_path(tree, folder)
                    } else {
                        s.folder_path.clone()
                    }
                ),
                LocalRef::Item(item) => item_label(tree, item),
            };
            (what, s.reason.clone())
        })
        .collect();
    out.differences = report
        .differences
        .iter()
        .map(|d| DifferenceRow {
            property: format!("0x{:08x}", d.property),
            item: d
                .item
                .as_ref()
                .map(|i| item_label(tree, i))
                .unwrap_or_else(|| "Store".into()),
            source: d.source.clone(),
            reason: d.reason.clone(),
        })
        .collect();
}

fn evidence_row(
    label: &'static str,
    note: &'static str,
    evidence: &Evidence,
    tree: &[Folder],
) -> EvidenceRow {
    match evidence {
        Evidence::NotChecked => EvidenceRow {
            label,
            value: "Not checked".into(),
            note,
            references: Vec::new(),
        },
        Evidence::Incomplete {
            count,
            references,
            unchecked,
        } => EvidenceRow {
            label,
            value: format!("{} found; {} not checked", count, unchecked.len()),
            note,
            references: references
                .iter()
                .map(|r| item_label(tree, r))
                .chain(
                    unchecked
                        .iter()
                        .map(|r| format!("Not checked: {}", item_label(tree, r))),
                )
                .collect(),
        },
        Evidence::Checked { count, references } => EvidenceRow {
            label,
            value: if *count == 0 {
                "None found".into()
            } else {
                format::plural(*count, "item", "items")
            },
            note,
            references: references.iter().map(|r| item_label(tree, r)).collect(),
        },
    }
}

/// Local references only: node ID and folder path, never subjects or addresses.
pub fn item_label(tree: &[Folder], item: &ItemRef) -> String {
    format!(
        "Item 0x{:x} in {}",
        item.node,
        if item.folder_path.is_empty() {
            folder_path(tree, &item.folder)
        } else {
            item.folder_path.clone()
        }
    )
}

fn file_rows(files: &[OutputFile]) -> Vec<FileRow> {
    files
        .iter()
        .map(|f| FileRow {
            path: f.path.clone(),
            size: format::bytes(f.bytes),
            part: f.part,
        })
        .collect()
}
