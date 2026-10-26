//! Synthetic-only Sandbox driver. Never shipped as the PstOff executable.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
mod windows {
    use pstoff_engine::{
        CancelToken, Conversion, ConvertOptions, EngineError, Inspection, Progress, ProgressSink,
    };
    use pstoff_gui::{Engine, RealEngine};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    pub struct SmokeEngine {
        pub output: PathBuf,
        pub held: AtomicBool,
    }

    impl Engine for SmokeEngine {
        fn inspect(&self, path: &Path) -> Result<Inspection, EngineError> {
            RealEngine.inspect(path)
        }

        fn convert(
            &self,
            path: &Path,
            options: &ConvertOptions,
            progress: &mut dyn ProgressSink,
            cancel: &CancelToken,
        ) -> Result<Conversion, EngineError> {
            let mut sink = |p: Progress| {
                let capture = p.current_folder.is_some() && p.fraction > 0.0 && p.fraction < 1.0;
                let marker = format!(
                    "done={} total={} folder={:?}",
                    p.done, p.total, p.current_folder
                );
                progress.update(p);
                if capture && !self.held.swap(true, Ordering::SeqCst) {
                    // Publish actual engine progress before holding the worker.
                    // The UI's ordinary polling and rendering keep running.
                    if std::fs::write(self.output.join("progress-held.txt"), marker).is_err() {
                        cancel.cancel();
                        return;
                    }
                    let deadline = Instant::now() + Duration::from_secs(60);
                    while !self.output.join("progress-release.txt").is_file() {
                        if cancel.is_cancelled() || Instant::now() >= deadline {
                            cancel.cancel();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
            };
            RealEngine.convert(path, options, &mut sink, cancel)
        }
    }

    pub fn main() {
        // Fixed guest-local paths and generated fixture names; no user inputs.
        let output = PathBuf::from(r"C:\PstOffSmokeOutput");
        let engine = Arc::new(SmokeEngine {
            output: output.clone(),
            held: AtomicBool::new(false),
        });
        let paths = [
            "1-mailbox.ost",
            "2-archive.ost",
            "3-older-ansi.ost",
            "4-damaged.ost",
        ]
        .map(|name| Path::new(r"C:\PstOffSmoke\fixtures").join(name));
        gpui_platform::application().run(move |cx: &mut gpui::App| {
            cx.on_window_closed(|cx, _| cx.quit()).detach();
            match pstoff_gui::view::open_window(engine, cx) {
                Ok(window) => {
                    let _ = window.update(cx, |view, _, cx| {
                        view.wizard.set_destination(output.join("conversions"));
                        view.add_paths(paths.into(), cx);
                    });
                    cx.activate(true);
                }
                Err(error) => panic!("smoke window: {error}"),
            }
        });
    }
}

#[cfg(windows)]
fn main() {
    windows::main();
}

#[cfg(not(windows))]
fn main() {
    panic!("the smoke driver requires Windows Sandbox");
}

#[cfg(all(windows, test))]
#[path = "../tests/support/mod.rs"]
mod support;

#[cfg(all(windows, test))]
#[test]
fn actual_conversion_waits_for_screenshot_release() {
    use pstoff_engine::{CancelToken, ConvertOptions};
    use pstoff_gui::Engine;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let t = support::Temp::new();
    let source = t.write("generated.ost", &support::mailbox());
    let engine = Arc::new(windows::SmokeEngine {
        output: t.root.clone(),
        held: AtomicBool::new(false),
    });
    let worker_engine = engine.clone();
    let mut options = ConvertOptions::new(t.out(), "generated".into());
    options.split = pstoff_engine::Split::Off;
    let worker = std::thread::spawn(move || {
        worker_engine.convert(&source, &options, &mut |_| {}, &CancelToken::default())
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    while !t.root.join("progress-held.txt").is_file() {
        assert!(Instant::now() < deadline, "no real progress event");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !worker.is_finished(),
        "worker must remain held for the shot"
    );
    let marker = std::fs::read_to_string(t.root.join("progress-held.txt")).unwrap();
    assert!(marker.starts_with("done=1 total=6 folder=Some("));
    assert!(!t.out().join("generated.txt").exists());
    std::fs::write(t.root.join("progress-release.txt"), b"release").unwrap();
    worker.join().unwrap().unwrap();
    assert_eq!(
        t.outputs(),
        ["generated.json", "generated.pst", "generated.txt"]
    );
}
