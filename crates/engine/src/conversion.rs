use crate::*;
use std::fs;

fn failure(kind: ErrorKind, message: impl Into<String>) -> EngineError {
    EngineError {
        kind,
        message: message.into(),
        completed_files: vec![],
    }
}

pub(super) fn convert(
    path: &Path,
    options: &ConvertOptions,
    progress: &mut dyn ProgressSink,
    cancel: &CancelToken,
) -> Result<Conversion, EngineError> {
    if cancel.is_cancelled() {
        return Ok(Conversion::Cancelled {
            completed_files: vec![],
        });
    }
    let (mut source, nodes, source_sha256) = reader::conversion_source(path)?;
    if options.base_name.is_empty()
        || options.base_name.contains(['/', '\\', ':'])
        || options.base_name == "."
        || options.base_name == ".."
    {
        return Err(failure(
            ErrorKind::InvalidOptions,
            "Choose a plain output base name.",
        ));
    }
    if matches!(options.split, Split::Bytes(0)) {
        return Err(failure(
            ErrorKind::InvalidOptions,
            "Split limit must be greater than zero.",
        ));
    }
    reader::inspect_nodes(path, &mut source, &nodes)?;
    let excluded = planner::excluded(&nodes, &options.exclusions)?;
    let recovery = writer::recover(&mut source, nodes, cancel);
    if cancel.is_cancelled() {
        return Ok(Conversion::Cancelled {
            completed_files: vec![],
        });
    }
    let recovery = recovery.map_err(|error| {
        failure(
            if error.is::<std::io::Error>() {
                ErrorKind::Io
            } else {
                ErrorKind::DamagedSource
            },
            format!("Source recovery planning failed: {error}"),
        )
    })?;
    if cancel.is_cancelled() {
        return Ok(Conversion::Cancelled {
            completed_files: vec![],
        });
    }
    let selection = planner::Selection::new(recovery, excluded, cancel);
    if cancel.is_cancelled() {
        return Ok(Conversion::Cancelled {
            completed_files: vec![],
        });
    }
    let selection = selection.map_err(io_failure)?;
    let split = matches!(options.split, Split::Bytes(_));
    let output_path = |part| {
        options.destination.join(if split {
            format!("{}-part{part:03}.pst", options.base_name)
        } else {
            format!("{}.pst", options.base_name)
        })
    };
    let json = options
        .destination
        .join(format!("{}.json", options.base_name));
    let text = options
        .destination
        .join(format!("{}.txt", options.base_name));
    if [&output_path(1), &json, &text].iter().any(|p| p.exists()) {
        return Err(failure(
            ErrorKind::Collision,
            "An output or report already exists.",
        ));
    }
    fs::create_dir_all(&options.destination).map_err(io_failure)?;
    flag_staging(&options.destination)?;
    let selected = selection.part(&selection.items, split);
    let selected_folders = folder_mapping(&mut source, &selected, 1)?;
    preflight_space(
        path,
        &options.destination,
        &selected_folders,
        options.space_reserve_bytes,
    )?;
    drop(selected);
    let staging = options
        .destination
        .join(format!(".{}.pstoff-partial", options.base_name));
    fs::create_dir(&staging).map_err(io_failure)?;
    let mut cleanup = Staging {
        path: staging.clone(),
        published: vec![],
    };
    let candidate = staging.join("candidate.pst");
    let accepted = staging.join("accepted.pst");
    let display = path.file_stem().unwrap_or_default().to_string_lossy();
    let total = selection.items.len() as u64;
    let mut files = Records::default();
    let mut folders = Records::default();
    let mut differences = Records::default();
    let mut high_done = 0u64;
    // Only the current candidate and last accepted finalized part occupy staging.
    // Trial emission includes allocation-map regeneration and all final overhead.
    let result = (|| -> Result<Option<StreamedReport>, EngineError> {
        let mut start = 0;
        let mut end = if split {
            usize::from(!selection.items.is_empty())
        } else {
            selection.items.len()
        };
        let mut accepted_end = start;
        let mut accepted_differences = Records::default();
        loop {
            let part = files.len() as u32 + 1;
            let planned = selection.part(&selection.items[start..end], split);
            let mut sink = |p: Progress| {
                high_done = high_done.max(start as u64 + p.done);
                progress.update(Progress {
                    fraction: 0.9 * high_done as f64 / total.max(1) as f64,
                    current_folder: p.current_folder,
                    done: high_done,
                    total,
                    part,
                });
            };
            let Some(candidate_differences) = writer::write(
                &mut source,
                &planned,
                &candidate,
                &display,
                &mut sink,
                cancel,
            )
            .map_err(|error| match error.downcast::<EngineError>() {
                Ok(error) => *error,
                Err(error) => failure(
                    ErrorKind::Io,
                    format!(
                        "Could not finalize a PST part; unfinished output was removed: {error}"
                    ),
                ),
            })?
            else {
                return Ok(None);
            };
            let bytes = fs::metadata(&candidate).map_err(io_failure)?.len();
            let fits = !matches!(options.split, Split::Bytes(limit) if bytes > limit);
            if fits {
                if accepted.exists() {
                    fs::remove_file(&accepted).map_err(io_failure)?;
                }
                fs::rename(&candidate, &accepted).map_err(io_failure)?;
                accepted_end = end;
                accepted_differences = candidate_differences;
                if end < selection.items.len() {
                    end += 1;
                    continue;
                }
            } else {
                fs::remove_file(&candidate).map_err(io_failure)?;
                if accepted_end == start {
                    if let Some(item) = selection.items.get(start) {
                        let reference = ItemRef {
                            folder: FolderRef(item.nid_parent),
                            node: item.nid,
                        };
                        return Err(failure(ErrorKind::OversizedItem(reference.clone()), format!(
                            "Folder 0x{:x}, Item 0x{:x} needs {bytes} bytes in an otherwise empty finalized PST. Increase the Split limit or exclude this folder and retry with a new output base name. Finished parts have been kept.", reference.folder.0, reference.node)));
                    }
                    return Err(failure(ErrorKind::InvalidOptions, format!("Required empty PST system folders need {bytes} bytes; increase the Split limit.")));
                }
            }
            let final_part = fits && end == selection.items.len();
            progress.update(Progress {
                fraction: if final_part {
                    0.95
                } else {
                    0.9 * high_done as f64 / total.max(1) as f64
                },
                current_folder: None,
                done: high_done,
                total,
                part,
            });
            if cancel.is_cancelled() {
                return Ok(None);
            }
            let output = output_path(part);
            let bytes = fs::metadata(&accepted).map_err(io_failure)?.len();
            rename_new(&accepted, &output).map_err(io_failure)?;
            // An unsplit PST and its reports form one successful Conversion.
            // Finished split parts remain useful if a later part/report fails.
            if !split {
                cleanup.published.push(output.clone());
            }
            files
                .push(OutputFile {
                    path: output,
                    bytes,
                    part,
                })
                .map_err(io_failure)?;
            let planned = selection.part(&selection.items[start..accepted_end], split);
            folders
                .append_cancellable(&folder_mapping(&mut source, &planned, part)?, cancel)
                .map_err(io_failure)?;
            differences
                .append_cancellable(&accepted_differences, cancel)
                .map_err(io_failure)?;
            if final_part {
                break;
            }
            start = accepted_end;
            end = start + 1;
        }
        let report = ConversionReport {
            schema_version: REPORT_SCHEMA_VERSION,
            mailbox_completeness: MailboxCompleteness::Unknown,
            mailbox_completeness_statement: MAILBOX_COMPLETENESS_STATEMENT.into(),
            source: path.into(),
            source_sha256,
            files: files.clone(),
            folders,
            skipped: selection.recovery.skipped,
            differences,
            evidence: selection.recovery.evidence,
        };
        let saved = save_reports(&report, &staging, cancel);
        if cancel.is_cancelled() {
            return Ok(None);
        }
        saved.map_err(io_failure)?;
        for (from, to) in [
            (staging.join("report.json"), json),
            (staging.join("report.txt"), text),
        ] {
            rename_new(&from, &to).map_err(io_failure)?;
            cleanup.published.push(to);
        }
        cleanup.published.clear();
        Ok(Some(report))
    })();
    match result {
        Err(_) if cancel.is_cancelled() => Ok(Conversion::Cancelled {
            completed_files: if split {
                files.collect_records().map_err(io_failure)?
            } else {
                vec![]
            },
        }),
        Err(mut error) => {
            if !split {
                files = Records::default();
            }
            error.completed_files = files.collect_records().map_err(io_failure)?;
            Err(error)
        }
        Ok(None) => Ok(Conversion::Cancelled {
            completed_files: if split {
                files.collect_records().map_err(io_failure)?
            } else {
                vec![]
            },
        }),
        Ok(Some(report)) => {
            progress.update(Progress {
                fraction: 1.0,
                current_folder: None,
                done: total,
                total,
                part: files.len() as u32,
            });
            Ok(Conversion::Complete(Box::new(report)))
        }
    }
}

fn folder_mapping(
    source: &mut pstoff_format::ndb::Pst,
    planned: &writer::Recovery,
    part: u32,
) -> Result<Records<FolderPart>, EngineError> {
    let mut folders = Records::default();
    for folder in planned
        .nodes
        .iter()
        .filter(|n| matches!(n.nid_type(), 2 | 3))
    {
        let mut count = 0;
        let mut bytes = 0u64;
        for item in planned
            .nodes
            .iter()
            .filter(|n| n.nid_parent == folder.nid && matches!(n.nid_type(), 4 | 8))
        {
            count += 1;
            // Recovery already validated nested contexts. Report mapping needs
            // only this Item's size, so do not stage its tables again here.
            let pc = writer::evidence_properties(
                source,
                item.bid_data,
                item.bid_sub,
                &[pstoff_format::ltp::PID_MESSAGE_SIZE],
                &CancelToken::default(),
            );
            if let Err(e) = &pc {
                if e.is::<std::io::Error>() {
                    return Err(failure(
                        ErrorKind::Io,
                        format!("Property staging failed: {e}"),
                    ));
                }
            }
            if let Ok(pc) = pc {
                if let Some(pstoff_format::ltp::Value::Int(size)) = pc.props.get(&0x0e08) {
                    bytes = bytes.saturating_add((*size).max(0) as u64);
                }
            }
        }
        folders
            .push(FolderPart {
                folder: FolderRef(folder.nid),
                part,
                items: count,
                bytes,
            })
            .map_err(io_failure)?;
    }
    Ok(folders)
}

// MoveFileExW without REPLACE_EXISTING atomically renames without overwriting.
// std::fs::rename permits replacement, including on Windows.
fn rename_new(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        // Both buffers are NUL-terminated and remain alive for the call. Flags 0
        // prohibit replacing a destination and moving across volumes.
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
        } == 0
        {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(from, to)?;
        fs::remove_file(from)
    }
}
fn io_failure(e: std::io::Error) -> EngineError {
    failure(
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            ErrorKind::Collision
        } else {
            ErrorKind::Io
        },
        e.to_string(),
    )
}
struct Staging {
    path: PathBuf,
    published: Vec<PathBuf>,
}
impl Drop for Staging {
    fn drop(&mut self) {
        for path in &self.published {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}
struct CancellableWriter<'a, W> {
    inner: W,
    cancel: &'a CancelToken,
}
impl<W: std::io::Write> std::io::Write for CancellableWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other("Report writing cancelled"));
        }
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
fn save_reports(
    report: &StreamedReport,
    staging: &Path,
    cancel: &CancelToken,
) -> std::io::Result<()> {
    use std::io::Write;
    for (name, json) in [("report.json", true), ("report.txt", false)] {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staging.join(name))?;
        let mut writer = CancellableWriter {
            inner: std::io::BufWriter::with_capacity(8192, file),
            cancel,
        };
        if json {
            serde_json::to_writer_pretty(&mut writer, report)
                .map_err(|_| std::io::Error::other("Could not serialize the Conversion report."))?;
        } else {
            report_text(report, &mut writer)?;
        }
        writer.flush()?;
        writer.inner.get_ref().sync_all()?;
    }
    Ok(())
}
fn report_text(report: &StreamedReport, text: &mut impl std::io::Write) -> std::io::Result<()> {
    write!(
        text,
        "{}\nSource: {}\nSHA-256: {}\n",
        report.mailbox_completeness_statement,
        report.source.display(),
        report.source_sha256
    )?;
    for file in report.files.iter() {
        let file = file?;
        writeln!(
            text,
            "File: {} ({} bytes, part {})",
            file.path.display(),
            file.bytes,
            file.part
        )?;
    }
    for folder in report.folders.iter() {
        let folder = folder?;
        writeln!(
            text,
            "Folder 0x{:x}: {} items, {} bytes, part {}",
            folder.folder.0, folder.items, folder.bytes, folder.part
        )?;
    }
    for (label, evidence) in [
        ("Headers-only Items", &report.evidence.headers_only),
        ("Unreadable content", &report.evidence.unreadable),
        (
            "External attachment references",
            &report.evidence.external_references,
        ),
    ] {
        match evidence {
            Evidence::NotChecked => {
                writeln!(text, "{label}: Not checked")?;
            }
            Evidence::Checked { count, references }
            | Evidence::Incomplete {
                count, references, ..
            } => {
                writeln!(text, "{label}: {count}")?;
                if let Evidence::Incomplete { unchecked, .. } = evidence {
                    writeln!(
                        text,
                        "  Not checked for {} Items; findings above are a lower bound.",
                        unchecked.len()
                    )?;
                    for item in unchecked.iter() {
                        let item = item?;
                        writeln!(
                            text,
                            "  Not checked: {}",
                            local_reference(&LocalRef::Item(item.clone()))
                        )?;
                    }
                }
                for item in references.iter() {
                    let item = item?;
                    writeln!(text, "  {}", local_reference(&LocalRef::Item(item.clone())))?;
                }
            }
        }
    }
    writeln!(text, "External references describe stored references, not missing content. Targets are not contacted or checked.\nSkipped: {}", report.skipped.len())?;
    for skipped in report.skipped.iter() {
        let skipped = skipped?;
        writeln!(
            text,
            "  {}: {}",
            local_reference(&skipped.reference),
            skipped.reason
        )?;
    }
    writeln!(
        text,
        "Recorded property differences: {}",
        report.differences.len()
    )?;
    for difference in report.differences.iter() {
        let difference = difference?;
        let reference = difference.item.as_ref().map_or_else(
            || "Store".into(),
            |item| local_reference(&LocalRef::Item(item.clone())),
        );
        writeln!(
            text,
            "  Property 0x{:08x}, {}\n    Source: {}\n    Reason: {}",
            difference.property, reference, difference.source, difference.reason
        )?;
    }
    Ok(())
}

fn local_reference(reference: &LocalRef) -> String {
    match reference {
        LocalRef::Folder(folder) => format!("Folder 0x{:x}", folder.0),
        LocalRef::Item(item) => format!("Folder 0x{:x}, Item 0x{:x}", item.folder.0, item.node),
    }
}

// Never delete a directory that could belong to a concurrent or crashed run.
// Flag all reserved staging names, including a different output base name.
fn flag_staging(destination: &Path) -> Result<(), EngineError> {
    for entry in fs::read_dir(destination).map_err(io_failure)? {
        let entry = entry.map_err(io_failure)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name.ends_with(".pstoff-partial") {
            return Err(failure(ErrorKind::Collision, format!(
                "Possible stale or active PstOff staging path: {}. Review it before retrying; it has been left untouched.", entry.path().display())));
        }
    }
    Ok(())
}
fn preflight_space(
    source: &Path,
    destination: &Path,
    folders: &Records<FolderPart>,
    reserve: u64,
) -> Result<(), EngineError> {
    // Source bytes account for properties/attachments not represented by message
    // sizes. Doubling plus 64 MiB reserves room for format expansion and reports.
    // This is a conservative estimate, not a guarantee against later disk changes.
    let declared = folders
        .iter()
        .try_fold(0u64, |sum, f| f.map(|f| sum.saturating_add(f.bytes)))
        .map_err(io_failure)?;
    let required = fs::metadata(source)
        .map_err(io_failure)?
        .len()
        .max(declared)
        .saturating_mul(2)
        .saturating_add(reserve);
    let available = available_space(destination).map_err(io_failure)?;
    if available < required {
        return Err(failure(ErrorKind::InsufficientSpace, format!(
            "Not enough destination space: {required} bytes estimated, {available} bytes available.")));
    }
    Ok(())
}
#[cfg(windows)]
fn available_space(path: &Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    let path = fs::canonicalize(path)?;
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    // NUL terminated directory path and valid output pointer; other outputs unused.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(available)
    }
}
#[cfg(not(windows))]
fn available_space(_: &Path) -> std::io::Result<u64> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Conversion requires Windows disk preflight.",
    ))
}

#[cfg(test)]
fn materialized_text_reference(report: &ConversionReport) -> String {
    use std::fmt::Write;
    let mut text = format!(
        "{}\nSource: {}\nSHA-256: {}\n",
        report.mailbox_completeness_statement,
        report.source.display(),
        report.source_sha256
    );
    for file in &report.files {
        let _ = writeln!(
            text,
            "File: {} ({} bytes, part {})",
            file.path.display(),
            file.bytes,
            file.part
        );
    }
    for folder in &report.folders {
        let _ = writeln!(
            text,
            "Folder 0x{:x}: {} items, {} bytes, part {}",
            folder.folder.0, folder.items, folder.bytes, folder.part
        );
    }
    for (label, evidence) in [
        ("Headers-only Items", &report.evidence.headers_only),
        ("Unreadable content", &report.evidence.unreadable),
        (
            "External attachment references",
            &report.evidence.external_references,
        ),
    ] {
        match evidence {
            Evidence::NotChecked => {
                let _ = writeln!(text, "{label}: Not checked");
            }
            Evidence::Checked { count, references }
            | Evidence::Incomplete {
                count, references, ..
            } => {
                let _ = writeln!(text, "{label}: {count}");
                if let Evidence::Incomplete { unchecked, .. } = evidence {
                    let _ = writeln!(
                        text,
                        "  Not checked for {} Items; findings above are a lower bound.",
                        unchecked.len()
                    );
                    for item in unchecked {
                        let _ = writeln!(
                            text,
                            "  Not checked: {}",
                            local_reference(&LocalRef::Item(item.clone()))
                        );
                    }
                }
                for item in references {
                    let _ = writeln!(text, "  {}", local_reference(&LocalRef::Item(item.clone())));
                }
            }
        }
    }
    let _ = writeln!(text, "External references describe stored references, not missing content. Targets are not contacted or checked.\nSkipped: {}", report.skipped.len());
    for skipped in &report.skipped {
        let _ = writeln!(
            text,
            "  {}: {}",
            local_reference(&skipped.reference),
            skipped.reason
        );
    }
    let _ = writeln!(
        text,
        "Recorded property differences: {}",
        report.differences.len()
    );
    for difference in &report.differences {
        let reference = difference.item.as_ref().map_or_else(
            || "Store".into(),
            |item| local_reference(&LocalRef::Item(item.clone())),
        );
        let _ = writeln!(
            text,
            "  Property 0x{:08x}, {}\n    Source: {}\n    Reason: {}",
            difference.property, reference, difference.source, difference.reason
        );
    }
    text
}

#[cfg(test)]
mod report_tests {
    use super::*;
    #[test]
    fn high_record_count_json_and_text_match_materialized_schema() {
        let mut report = ConversionReport {
            schema_version: REPORT_SCHEMA_VERSION,
            mailbox_completeness: MailboxCompleteness::Unknown,
            mailbox_completeness_statement: MAILBOX_COMPLETENESS_STATEMENT.into(),
            source: PathBuf::from("generated.ost"),
            source_sha256: "00".repeat(32),
            files: vec![],
            folders: vec![],
            skipped: vec![],
            differences: vec![],
            evidence: CompletenessEvidence {
                headers_only: Evidence::NotChecked,
                unreadable: Evidence::NotChecked,
                external_references: Evidence::NotChecked,
            },
        }
        .into_streamed()
        .unwrap();
        let mut references = Records::default();
        for n in 0..25_000 {
            let item = ItemRef {
                folder: FolderRef(2),
                node: n * 32 + 4,
            };
            references.push(item.clone()).unwrap();
            report
                .skipped
                .push(Skipped {
                    code: SkipReason::UnreadableItem,
                    reference: LocalRef::Item(item.clone()),
                    reason: "Synthetic unreadable context".into(),
                })
                .unwrap();
            report
                .differences
                .push(Difference {
                    property: 0x30080040,
                    item: Some(item),
                    source: "Synthetic contents table".into(),
                    reason: "Generated regression diagnostic. ".repeat(32),
                })
                .unwrap();
            report
                .folders
                .push(FolderPart {
                    folder: FolderRef(2 + (n % 64) * 32),
                    part: 1,
                    items: 1,
                    bytes: 1024,
                })
                .unwrap();
        }
        report.evidence.headers_only = Evidence::Incomplete {
            count: 25_000,
            references: references.clone(),
            unchecked: references,
        };
        assert_eq!(report.summary_counts().unwrap(), (25_000, 64));
        // Materialization is deliberately confined to compatibility assertions.
        let collected = report.collect_report().unwrap();
        let mut json = Vec::new();
        serde_json::to_writer_pretty(&mut json, &report).unwrap();
        assert!(
            json == serde_json::to_vec_pretty(&collected).unwrap(),
            "Streamed JSON differs from schema-2 serialization"
        );
        let mut text = Vec::new();
        report_text(&report, &mut text).unwrap();
        assert!(
            text == materialized_text_reference(&collected).into_bytes(),
            "Streamed text differs from the legacy formatter"
        );
        let decoded: StreamedReport = serde_json::from_slice(&json).unwrap();
        assert_eq!(decoded.differences.len(), 25_000);
        assert_eq!(decoded.summary_counts().unwrap(), (25_000, 64));
    }
    #[test]
    fn report_writes_honor_cancellation_and_io_errors() {
        struct CancelAfterWrite {
            token: CancelToken,
            written: usize,
            threshold: usize,
        }
        impl std::io::Write for CancelAfterWrite {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.written += b.len();
                if self.written >= self.threshold {
                    self.token.cancel();
                }
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut records = Records::default();
        records
            .extend((0..25_000).map(|node| ItemRef {
                folder: FolderRef(2),
                node,
            }))
            .unwrap();
        struct Count(usize);
        impl std::io::Write for Count {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0 += b.len();
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        serde_json::to_writer(&mut count, &records).unwrap();
        for threshold in [0, count.0 / 2, count.0 - 2] {
            let token = CancelToken::default();
            let mut writer = CancellableWriter {
                inner: CancelAfterWrite {
                    token: token.clone(),
                    written: 0,
                    threshold,
                },
                cancel: &token,
            };
            assert!(serde_json::to_writer(&mut writer, &records).is_err());
            assert!(token.is_cancelled());
        }
        struct Failing;
        impl std::io::Write for Failing {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("Synthetic disk full"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(serde_json::to_writer(Failing, &records).is_err());
    }
}

#[cfg(test)]
mod report_cleanup_tests {
    use super::*;
    #[test]
    fn cancelled_report_staging_and_unsplit_output_are_removed() {
        let root =
            std::env::temp_dir().join(format!("pstoff-report-cleanup-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let staging = root.join("partial");
        fs::create_dir(&staging).unwrap();
        let output = root.join("output.pst");
        fs::write(&output, b"synthetic").unwrap();
        let report = ConversionReport {
            schema_version: REPORT_SCHEMA_VERSION,
            mailbox_completeness: MailboxCompleteness::Unknown,
            mailbox_completeness_statement: MAILBOX_COMPLETENESS_STATEMENT.into(),
            source: PathBuf::from("generated.ost"),
            source_sha256: String::new(),
            files: vec![],
            folders: vec![],
            skipped: vec![],
            differences: vec![],
            evidence: CompletenessEvidence {
                headers_only: Evidence::NotChecked,
                unreadable: Evidence::NotChecked,
                external_references: Evidence::NotChecked,
            },
        }
        .into_streamed()
        .unwrap();
        let token = CancelToken::default();
        token.cancel();
        {
            let _cleanup = Staging {
                path: staging.clone(),
                published: vec![output.clone()],
            };
            assert!(save_reports(&report, &staging, &token).is_err());
        }
        assert!(!staging.exists());
        assert!(!output.exists());
        fs::remove_dir(&root).unwrap();
    }
}
