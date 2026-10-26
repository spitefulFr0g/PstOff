use pstoff_engine::{
    convert, inspect, CancelToken, Conversion, ConvertOptions, ErrorKind, Progress, Support,
};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_default();
    let Some(command) = args.next() else {
        return usage(&program);
    };
    if command != "inspect" && command != "convert" {
        eprintln!("Unknown command: {}", Path::new(&command).display());
        return ExitCode::from(2);
    }
    let Some(path) = args.next() else {
        return usage(&program);
    };
    let destination = if command == "convert" {
        match args.next() {
            Some(flag) if flag == "--out" => args.next(),
            _ => None,
        }
    } else {
        args.next()
    };
    if args.next().is_some()
        || (command == "inspect" && destination.is_some())
        || (command == "convert" && destination.is_none())
    {
        return usage(&program);
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
            Ok(Conversion::Complete(report)) => {
                for file in &report.files {
                    println!("PST: {} ({} bytes)", file.path.display(), file.bytes);
                    println!(
                        "Reports: {}, {}",
                        file.path.with_extension("json").display(),
                        file.path.with_extension("txt").display()
                    );
                }
                println!("{}", report.mailbox_completeness_statement);
                ExitCode::SUCCESS
            }
            Ok(Conversion::Cancelled { .. }) => {
                eprintln!("Conversion cancelled.");
                ExitCode::from(130)
            }
            Err(error) => {
                eprintln!("Conversion failed: {error}");
                error_exit(&error.kind)
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
            error_exit(&error.kind)
        }
    }
}

fn usage(program: &std::ffi::OsStr) -> ExitCode {
    eprintln!(
        "Usage: {} inspect <ost> | convert <ost> --out <dir>",
        Path::new(program).display()
    );
    ExitCode::from(2)
}

fn error_exit(kind: &ErrorKind) -> ExitCode {
    if matches!(kind, ErrorKind::Unsupported(_)) {
        ExitCode::from(3)
    } else {
        ExitCode::FAILURE
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
