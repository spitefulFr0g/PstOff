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
    if !options.exclusions.is_empty()
        || matches!(options.split, Split::Bytes(n) if n != DEFAULT_SPLIT_BYTES)
    {
        return Err(failure(
            ErrorKind::NotImplemented,
            "Split and exclusions require the conversion planner.",
        ));
    }
    let inspection = reader::inspect(path)?;
    let mut folders = vec![];
    let mut pending: Vec<_> = inspection.folders.iter().collect();
    while let Some(folder) = pending.pop() {
        if folder.unreadable.is_some() {
            return Err(failure(
                ErrorKind::DamagedSource,
                "A source folder or Item is unreadable.",
            ));
        }
        folders.push(FolderPart {
            folder: folder.id.clone(),
            part: 1,
            items: folder.item_count,
            bytes: folder.bytes,
        });
        pending.extend(&folder.children);
    }
    let output = options
        .destination
        .join(format!("{}.pst", options.base_name));
    let json = options
        .destination
        .join(format!("{}.json", options.base_name));
    let text = options
        .destination
        .join(format!("{}.txt", options.base_name));
    if [&output, &json, &text].iter().any(|p| p.exists()) {
        return Err(failure(
            ErrorKind::Collision,
            "An output or report already exists.",
        ));
    }
    fs::create_dir_all(&options.destination).map_err(|e| failure(ErrorKind::Io, e.to_string()))?;
    let staging = options
        .destination
        .join(format!(".{}.pstoff-partial", options.base_name));
    fs::create_dir(&staging).map_err(io_failure)?;
    let mut cleanup = Staging {
        path: staging.clone(),
        published: vec![],
    };
    let temporary = staging.join("output.pst");
    let display = path.file_stem().unwrap_or_default().to_string_lossy();
    let total = nodes
        .iter()
        .filter(|n| matches!(n.nid_type(), 4 | 8))
        .count() as u64;
    let Some(differences) =
        writer::write(&mut source, &nodes, &temporary, &display, progress, cancel)
            .map_err(|e| failure(ErrorKind::Io, e.to_string()))?
    else {
        return Ok(Conversion::Cancelled {
            completed_files: vec![],
        });
    };
    // The writer has closed and flushed the PST before it can receive its final name.
    let bytes = fs::metadata(&temporary)
        .map_err(|e| failure(ErrorKind::Io, e.to_string()))?
        .len();
    if matches!(options.split, Split::Bytes(n) if bytes > n) {
        let _ = fs::remove_file(&temporary);
        return Err(failure(
            ErrorKind::NotImplemented,
            "This conversion requires splitting, which is not implemented yet.",
        ));
    }
    progress.update(Progress {
        fraction: 0.95,
        current_folder: None,
        done: total,
        total,
        part: 1,
    });
    if cancel.is_cancelled() {
        let _ = fs::remove_file(&temporary);
        return Ok(Conversion::Cancelled {
            completed_files: vec![],
        });
    }
    let report = ConversionReport {
        schema_version: REPORT_SCHEMA_VERSION,
        mailbox_completeness: MailboxCompleteness::Unknown,
        mailbox_completeness_statement: MAILBOX_COMPLETENESS_STATEMENT.into(),
        source: path.into(),
        source_sha256,
        files: vec![OutputFile {
            path: output,
            bytes,
            part: 1,
        }],
        folders,
        skipped: vec![],
        differences,
        evidence: CompletenessEvidence {
            headers_only: Evidence::NotChecked,
            unreadable: Evidence::NotChecked,
            external_references: Evidence::NotChecked,
        },
    };
    let json_bytes =
        serde_json::to_vec_pretty(&report).map_err(|e| failure(ErrorKind::Io, e.to_string()))?;
    write_synced(&staging.join("report.json"), &json_bytes)?;
    write_synced(&staging.join("report.txt"), report_text(&report).as_bytes())?;
    // Publish the PST last. A failure removes only reports published by this call.
    for (from, to) in [
        (staging.join("report.json"), json),
        (staging.join("report.txt"), text),
        (temporary, report.files[0].path.clone()),
    ] {
        rename_new(&from, &to).map_err(io_failure)?;
        cleanup.published.push(to);
    }
    cleanup.published.clear();
    progress.update(Progress {
        fraction: 1.0,
        current_folder: None,
        done: total,
        total,
        part: 1,
    });
    Ok(Conversion::Complete(Box::new(report)))
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
            Evidence::Checked { count, references } => {
                let _ = writeln!(text, "{label}: {count}");
                for item in references {
                    let _ = writeln!(text, "  {}", local_reference(&LocalRef::Item(item.clone())));
                }
            }
        }
    }
    let _ = writeln!(text, "Skipped: {}", report.skipped.len());
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
