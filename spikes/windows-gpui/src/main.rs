//! Synthetic Windows feasibility probe. This does not inspect or convert mail.
use gpui::{
    App, Context, Div, ExternalPaths, PathPromptOptions, Stateful, Window, WindowOptions, div,
    prelude::*, rgb,
};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

#[derive(Default)]
struct Work {
    percent: AtomicU64,
    cancel: AtomicBool,
    finished: AtomicBool,
}

struct Probe {
    status: String,
    percent: u64,
    work: Option<Arc<Work>>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        if let Some(work) = &self.work {
            work.cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl Probe {
    fn select(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let accepted = paths
            .iter()
            .filter(|path| {
                path.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case("ost"))
            })
            .count();
        self.status = format!(
            "Selected {accepted} synthetic OST paths; rejected {}. No files opened.",
            paths.len() - accepted
        );
        cx.notify();
    }

    fn browse(&mut self, directory: bool, cx: &mut Context<Self>) {
        let result = cx.prompt_for_paths(PathPromptOptions {
            files: !directory,
            directories: directory,
            multiple: !directory,
            prompt: Some("Choose synthetic test inputs only".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = result.await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(Some(paths))) if !directory => this.select(paths, cx),
                    Ok(Ok(Some(_))) => {
                        this.status = "Destination selected; no files written.".into()
                    }
                    Ok(Ok(None)) => this.status = "Browse cancelled.".into(),
                    _ => this.status = "Native dialog failed.".into(),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        // Wait for the UI observer to finish before allowing another worker.
        // Otherwise its final update could overwrite a newly started run.
        if self.work.is_some() {
            return;
        }
        let work = Arc::new(Work::default());
        self.work = Some(work.clone());
        self.percent = 0;
        self.status = "Synthetic background work running; resize or cancel.".into();
        let worker = work.clone();
        std::thread::spawn(move || {
            for percent in 1..=100 {
                if worker.cancel.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
                worker.percent.store(percent, Ordering::Relaxed);
            }
            worker.finished.store(true, Ordering::Release);
        });
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                executor.timer(Duration::from_millis(50)).await;
                let finished = work.finished.load(Ordering::Acquire);
                if this
                    .update(cx, |this, cx| {
                        this.percent = work.percent.load(Ordering::Relaxed);
                        if finished {
                            this.status = if work.cancel.load(Ordering::Relaxed) {
                                "Synthetic work cancelled."
                            } else {
                                "Synthetic work finished."
                            }
                            .into();
                            this.work = None;
                        }
                        cx.notify();
                    })
                    .is_err()
                    || finished
                {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn cancel(&mut self) {
        if let Some(work) = &self.work {
            work.cancel.store(true, Ordering::Relaxed);
        }
    }
}

fn button(
    id: &'static str,
    label: &'static str,
    cx: &Context<Probe>,
    activate: impl Fn(&mut Probe, &mut Context<Probe>) + Copy + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .p_3()
        .rounded_md()
        .bg(rgb(0x305f9e))
        .text_color(rgb(0xffffff))
        .border_2()
        .border_color(rgb(0x305f9e))
        .on_click(cx.listener(move |this, _, _, cx| activate(this, cx)))
        .child(label)
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().id("probe").size_full()
            .flex().flex_col().gap_4().p_6().bg(rgb(0x18212b)).text_color(rgb(0xffffff))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.select(paths.0.to_vec(), cx)))
            .child(div().text_xl().child("PstOff — GPUI Windows feasibility"))
            .child("Synthetic inputs only. This probe never reads or converts mail.")
            .child("Drop synthetic .ost paths here or click Browse. Use the buttons to choose a destination, start or cancel work.")
            .child(div().flex().flex_wrap().gap_3()
                .child(button("browse", "Browse synthetic OSTs", cx, |this, cx| this.browse(false, cx)))
                .child(button("destination", "Choose destination", cx, |this, cx| this.browse(true, cx)))
                .child(button("start", "Start synthetic work", cx, |this, cx| this.start(cx)))
                .child(button("cancel", "Cancel work", cx, |this, _| this.cancel())))
            .child(div().id("progress")
                .child(format!("{}% — {}", self.percent, self.status)))
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.on_window_closed(|cx, _| {
            // This probe owns one GPUI window; closing it always ends the app.
            cx.quit();
        })
        .detach();
        cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| Probe {
                status: "Ready for synthetic checks.".into(),
                percent: 0,
                work: None,
            })
        })
        .expect("open GPUI probe window");
        cx.activate(true);
    });
}
