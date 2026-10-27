use pstoff_engine::{
    convert, inspect, CancelToken, Conversion, ConvertOptions, ErrorKind, FolderRef, Progress,
    Split, Support,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod interrupt;

// Public scripting contract; keep these values stable.
const FAILURE: u8 = 1;
const USAGE: u8 = 2;
const UNSUPPORTED: u8 = 3;
const IN_USE: u8 = 4;
const CAPACITY: u8 = 5;
const SKIPS: u8 = 6;
const CANCELLED: u8 = 130;

const HELP: &str = "Usage: pstoff inspect <ost>
       pstoff convert <ost> --out <dir> [--base-name <name>]
              [--exclude-folder <id>]... [--split-limit <bytes|off>]

Folder IDs are printed by inspect (decimal or 0x-prefixed hexadecimal accepted).
Exclusions include descendants. Split defaults to 21474836480 bytes (20 GiB).
Progress goes to stderr. Reports go to the output folder.
Exit codes: 0 success, 1 failure, 2 usage, 3 unsupported, 4 in use,
            5 insufficient space/oversized Item, 6 success with skips, 130 cancelled.";

enum Command {
    Inspect(PathBuf),
    Convert(PathBuf, ConvertOptions),
    Help,
}

fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Command, String> {
    let command = args.next().ok_or("Missing command.")?;
    if command == "--help" || command == "-h" {
        return Ok(Command::Help);
    }
    if command != "inspect" && command != "convert" {
        return Err("Unknown command.".into());
    }
    let path = PathBuf::from(args.next().ok_or("Missing OST path.")?);
    if command == "inspect" {
        if args.next().is_some() {
            return Err("Unexpected inspect argument.".into());
        }
        return Ok(Command::Inspect(path));
    }
    let base = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut options = ConvertOptions::new(PathBuf::new(), base);
    let (mut out, mut base, mut split) = (false, false, false);
    while let Some(flag) = args.next() {
        if flag != "--out"
            && flag != "--base-name"
            && flag != "--exclude-folder"
            && flag != "--split-limit"
        {
            return Err(format!("Unknown option: {}", flag.to_string_lossy()));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {}.", flag.to_string_lossy()))?;
        match flag.to_str().unwrap_or_default() {
            "--out" if !out => {
                options.destination = PathBuf::from(value);
                out = true;
            }
            "--base-name" if !base => {
                options.base_name = value
                    .into_string()
                    .map_err(|_| "Base name must be Unicode.")?;
                base = true;
            }
            "--split-limit" if !split => {
                let value = value.to_str().ok_or("Invalid Split limit.")?;
                options.split = if value == "off" {
                    Split::Off
                } else {
                    let bytes = value
                        .parse::<u64>()
                        .map_err(|_| "Split limit must be positive bytes or off.")?;
                    if bytes == 0 {
                        return Err("Split limit must be greater than zero.".into());
                    }
                    Split::Bytes(bytes)
                };
                split = true;
            }
            "--exclude-folder" => {
                let value = value.to_str().ok_or("Invalid folder ID.")?;
                let id = if let Some(hex) = value.strip_prefix("0x") {
                    u32::from_str_radix(hex, 16)
                } else {
                    value.parse::<u32>()
                }
                .map_err(|_| "Folder ID must be decimal or 0x-prefixed hexadecimal.")?;
                options.exclusions.push(FolderRef(id));
            }
            _ => return Err(format!("Repeated option: {}", flag.to_string_lossy())),
        }
    }
    if !out || options.destination.as_os_str().is_empty() {
        return Err("An output folder (--out) is required.".into());
    }
    Ok(Command::Convert(path, options))
}

fn error_code(kind: &ErrorKind) -> u8 {
    match kind {
        ErrorKind::Unsupported(_) => UNSUPPORTED,
        ErrorKind::InUse => IN_USE,
        ErrorKind::InsufficientSpace | ErrorKind::OversizedItem(_) => CAPACITY,
        _ => FAILURE,
    }
}

fn main() -> ExitCode {
    let command = match parse(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}\n{HELP}");
            return ExitCode::from(USAGE);
        }
    };
    match command {
        Command::Help => {
            println!("{HELP}");
            ExitCode::SUCCESS
        }
        Command::Inspect(path) => run_inspect(&path),
        Command::Convert(path, options) => run_convert(&path, &options),
    }
}

fn run_convert(path: &Path, options: &ConvertOptions) -> ExitCode {
    let cancel = CancelToken::default();
    if let Err(error) = interrupt::install(cancel.clone()) {
        eprintln!("Could not install cancellation handler: {error}");
        return ExitCode::from(FAILURE);
    }
    let mut progress = |p: Progress| {
        eprintln!(
            "Progress: {:.1}% items {}/{} part {} folder {}",
            p.fraction * 100.0,
            p.done,
            p.total,
            p.part,
            p.current_folder
                .map(|id| format!("0x{:x}", id.0))
                .unwrap_or_else(|| "-".into())
        );
    };
    match convert(path, options, &mut progress, &cancel) {
        Ok(Conversion::Complete(report)) => {
            for file in report.files.iter() {
                let file = match file {
                    Ok(file) => file,
                    Err(_) => {
                        eprintln!("Could not read report records.");
                        return ExitCode::FAILURE;
                    }
                };
                println!("PST: {} ({} bytes)", file.path.display(), file.bytes);
            }
            println!(
                "Reports: {}, {}",
                options
                    .destination
                    .join(format!("{}.json", options.base_name))
                    .display(),
                options
                    .destination
                    .join(format!("{}.txt", options.base_name))
                    .display()
            );
            println!("{}", report.mailbox_completeness_statement);
            println!("Skipped: {}", report.skipped.len());
            ExitCode::from(if report.skipped.is_empty() { 0 } else { SKIPS })
        }
        Ok(Conversion::Cancelled { completed_files }) => {
            eprintln!(
                "Conversion cancelled; unfinished output removed. {} finished parts kept.",
                completed_files.len()
            );
            ExitCode::from(CANCELLED)
        }
        Err(error) => {
            eprintln!("Conversion failed: {error}");
            for file in error.completed_files.iter() {
                match file {
                    Ok(file) => eprintln!("Finished part kept: {}", file.path.display()),
                    Err(e) => {
                        eprintln!("Could not read finished-part records: {e}");
                        break;
                    }
                }
            }
            ExitCode::from(error_code(&error.kind))
        }
    }
}

fn run_inspect(path: &Path) -> ExitCode {
    match inspect(path) {
        Ok(result) => {
            println!("Source: {}", result.source.display());
            let code = match result.support {
                Support::Supported => {
                    println!("Support: supported");
                    0
                }
                Support::Unsupported(reason) => {
                    println!("Support: unsupported ({reason:?})");
                    UNSUPPORTED
                }
                Support::InUse => {
                    println!("Support: in use");
                    IN_USE
                }
            };
            for folder in &result.folders {
                print_folder(folder, 0);
            }
            if result.folders.is_empty() {
                println!("Folders: none");
            }
            ExitCode::from(code)
        }
        Err(error) => {
            eprintln!("Inspection failed: {error}");
            ExitCode::from(error_code(&error.kind))
        }
    }
}

fn print_folder(folder: &pstoff_engine::Folder, depth: usize) {
    println!(
        "{}{} [0x{:x}] ({} items, {} bytes{})",
        "  ".repeat(depth),
        if folder.name.is_empty() {
            "(unnamed)"
        } else {
            &folder.name
        },
        folder.id.0,
        folder.item_count,
        folder.bytes,
        folder
            .unreadable
            .as_deref()
            .map(|reason| format!(", unreadable: {reason}"))
            .unwrap_or_default()
    );
    for child in &folder.children {
        print_folder(child, depth + 1);
    }
}
