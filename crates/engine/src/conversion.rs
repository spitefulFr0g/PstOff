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
    let selection = planner::Selection::new(recovery, excluded);
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
    let mut files = vec![];
    let mut folders = vec![];
    let mut differences = vec![];
    let mut high_done = 0u64;
    // Only the current candidate and last accepted finalized part occupy staging.
    // Trial emission includes allocation-map regeneration and all final overhead.
    let result = (|| -> Result<Option<ConversionReport>, EngineError> {
        let mut start = 0;
        let mut end = if split {
            usize::from(!selection.items.is_empty())
        } else {
            selection.items.len()
        };
        let mut accepted_end = start;
        let mut accepted_differences = vec![];
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
            files.push(OutputFile {
                path: output,
                bytes,
                part,
            });
            let planned = selection.part(&selection.items[start..accepted_end], split);
            folders.extend(folder_mapping(&mut source, &planned, part)?);
            differences.append(&mut accepted_differences);
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
        let json_bytes = serde_json::to_vec_pretty(&report)
            .map_err(|_| failure(ErrorKind::Io, "Could not serialize the Conversion report."))?;
        write_synced(&staging.join("report.json"), &json_bytes)?;
        write_synced(&staging.join("report.txt"), report_text(&report).as_bytes())?;
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
        Err(mut error) => {
            if !split {
                files.clear();
            }
            error.completed_files = files;
            Err(error)
        }
        Ok(None) => Ok(Conversion::Cancelled {
            completed_files: files,
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
) -> Result<Vec<FolderPart>, EngineError> {
    let mut folders = vec![];
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
        folders.push(FolderPart {
            folder: FolderRef(folder.nid),
            part,
            items: count,
            bytes,
        });
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
fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_failure)?;
    file.write_all(bytes).map_err(io_failure)?;
    file.sync_all().map_err(io_failure)
}
fn report_text(report: &ConversionReport) -> String {
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
    folders: &[FolderPart],
    reserve: u64,
) -> Result<(), EngineError> {
    // Source bytes account for properties/attachments not represented by message
    // sizes. Doubling plus 64 MiB reserves room for format expansion and reports.
    // This is a conservative estimate, not a guarantee against later disk changes.
    let declared = folders
        .iter()
        .fold(0u64, |sum, f| sum.saturating_add(f.bytes));
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
