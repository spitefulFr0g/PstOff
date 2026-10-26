use pstoff_engine::{convert, inspect, CancelToken, ConvertOptions, ErrorKind, Progress, Support};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(command) = args.next() else {
        eprintln!(
            "Usage: {} inspect <ost> | convert <ost> <destination>",
            Path::new(&program).display()
        );
        return ExitCode::from(2);
    };
    if command != "inspect" && command != "convert" {
        eprintln!("Unknown command: {}", Path::new(&command).display());
        return ExitCode::from(2);
    }
    let Some(path) = args.next() else {
        eprintln!(
            "Usage: {} inspect <ost> | convert <ost> <destination>",
            Path::new(&program).display()
        );
        return ExitCode::from(2);
    };
    let destination = args.next();
    if args.next().is_some()
        || (command == "inspect" && destination.is_some())
        || (command == "convert" && destination.is_none())
    {
        eprintln!("Usage: {} inspect <ost>", Path::new(&program).display());
        return ExitCode::from(2);
    }

    if command == "convert" {
        let destination = Path::new(destination.as_ref().unwrap());
        let base_name = Path::new(&path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let options = ConvertOptions::new(destination.to_path_buf(), base_name);
        let mut progress = |_p: Progress| {};
        return match convert(&path, &options, &mut progress, &CancelToken::default()) {
            Ok(_) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Conversion failed: {error}");
                if matches!(error.kind, ErrorKind::Unsupported(_)) {
                    ExitCode::from(3)
                } else {
                    ExitCode::FAILURE
                }
            }
        };
    }
    match inspect(&path) {
        Ok(result) => {
            println!("Source: {}", result.source.display());
            match result.support {
                Support::Supported => println!("Support: supported"),
                Support::Unsupported(reason) => println!("Support: unsupported ({reason:?})"),
                Support::InUse => println!("Support: in use"),
            }
            for folder in &result.folders {
                print_folder(folder, 0);
            }
            if result.folders.is_empty() {
                println!("Folders: none");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Inspection failed: {error}");
            if matches!(error.kind, ErrorKind::Unsupported(_)) {
                ExitCode::from(3)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

fn print_folder(folder: &pstoff_engine::Folder, depth: usize) {
    println!(
        "{}{} ({} items, {} bytes{})",
        "  ".repeat(depth),
        if folder.name.is_empty() {
            "(unnamed)"
        } else {
            &folder.name
        },
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
