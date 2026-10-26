use pstoff_engine::{inspect, Support};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(command) = args.next() else {
        eprintln!("Usage: {} inspect <ost>", Path::new(&program).display());
        return ExitCode::from(2);
    };
    if command != "inspect" {
        eprintln!("Unknown command: {}", Path::new(&command).display());
        return ExitCode::from(2);
    }
    let Some(path) = args.next() else {
        eprintln!("Usage: {} inspect <ost>", Path::new(&program).display());
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("Usage: {} inspect <ost>", Path::new(&program).display());
        return ExitCode::from(2);
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
            ExitCode::FAILURE
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
