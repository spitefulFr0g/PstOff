// No console window behind the wizard in release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    use std::sync::Arc;

    // OST paths on the command line are added straight away, as when files
    // are dropped onto the .exe in Explorer. `--out <folder>` picks the
    // destination, as the CLI's flag does.
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    let mut destination = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--out" {
            destination = args.next().map(Into::into);
        } else {
            paths.push(arg.into());
        }
    }
    gpui_platform::application().run(move |cx: &mut gpui::App| {
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        match pstoff_gui::view::open_window(Arc::new(pstoff_gui::RealEngine), cx) {
            Ok(window) => {
                let _ = window.update(cx, |view, _, cx| {
                    if let Some(destination) = destination {
                        view.wizard.set_destination(destination);
                    }
                    view.add_paths(paths, cx);
                });
                cx.activate(true);
            }
            Err(error) => {
                eprintln!("PstOff couldn’t open its window: {error}");
                cx.quit();
            }
        }
    });
}

#[cfg(not(windows))]
fn main() {
    eprintln!("PstOff GUI requires Windows.");
    std::process::exit(1);
}
