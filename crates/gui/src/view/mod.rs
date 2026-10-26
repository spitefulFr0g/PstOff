//! GPUI rendering of the [`Wizard`]. Variant A from the #13 prototype is the
//! visual reference: a left step rail, one large question per screen and
//! Back / Next at the bottom. Engine work never runs on the UI thread.
mod screens;
mod widgets;

use crate::wizard::{Engine, InspectRequest, SourceId, Step, Wizard};
use gpui::{
    div, prelude::*, px, rgb, rgba, App, Context, ExternalPaths, FocusHandle, FontWeight,
    KeyDownEvent, PathPromptOptions, SharedString, Window,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use widgets::{button, ButtonKind};

/// How often the UI picks up progress from the conversion worker.
pub const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub mod palette {
    pub const MIST: u32 = 0xf2f4f6;
    pub const PAPER: u32 = 0xffffff;
    pub const INK: u32 = 0x1d2733;
    pub const PEWTER: u32 = 0x5a6573;
    pub const TEAL: u32 = 0x0b7a6f;
    pub const TEAL_DEEP: u32 = 0x086258;
    pub const TEAL_TEXT: u32 = 0x086258;
    pub const TEAL_TINT: u32 = 0xe5f2ef;
    pub const AMBER: u32 = 0x9a5b00;
    pub const AMBER_BG: u32 = 0xfff3dc;
    pub const FAULT: u32 = 0xb42318;
    pub const FOCUS: u32 = 0x2b63d9;
    pub const HAIRLINE: u32 = 0xdce1e6;
    pub const HAIRLINE_STRONG: u32 = 0xa4aeb9;
    pub const DISABLED_BG: u32 = 0xe3e7eb;
    pub const DISABLED_TEXT: u32 = 0x687380;
    pub const DISPLAY_FONT: &str = "Segoe UI Variable";
}
use palette::*;

pub struct WizardView {
    pub wizard: Wizard,
    focus: FocusHandle,
    controls: RefCell<HashMap<String, FocusHandle>>,
    /// The OST whose PST name field takes typing.
    pub editing: Option<SourceId>,
    /// Frames rendered so far; tests use it to see the UI keeps drawing.
    pub frames: u64,
}

impl WizardView {
    pub fn new(engine: Arc<dyn Engine>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            this.update(cx, |view, cx| view.should_close(cx))
                .unwrap_or(true)
        });
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            wizard: Wizard::new(engine),
            focus,
            controls: RefCell::new(HashMap::new()),
            editing: None,
            frames: 0,
        }
    }

    pub(super) fn focus_for(&self, name: &str, cx: &Context<Self>) -> FocusHandle {
        self.controls
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for request in self.wizard.add_paths(paths) {
            self.inspect(request, cx);
        }
        cx.notify();
    }

    fn inspect(&mut self, request: InspectRequest, cx: &mut Context<Self>) {
        let engine = self.wizard.engine();
        let path = request.path.clone();
        let task = cx
            .background_executor()
            .spawn(async move { engine.inspect(&path) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.wizard.apply_inspection(request.id, result);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn retry(&mut self, id: SourceId, cx: &mut Context<Self>) {
        if let Some(request) = self.wizard.retry(id) {
            self.inspect(request, cx);
        }
        cx.notify();
    }

    pub fn browse_osts(&mut self, cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add OST files".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = answer.await {
                let _ = this.update(cx, |this, cx| this.add_paths(paths, cx));
            }
        })
        .detach();
    }

    pub fn browse_destination(&mut self, cx: &mut Context<Self>) {
        let answer = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Save PST files here".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = answer.await {
                if let Some(path) = paths.pop() {
                    let _ = this.update(cx, |this, cx| {
                        this.wizard.set_destination(path);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    pub fn go(&mut self, step: Step, cx: &mut Context<Self>) {
        self.editing = None;
        self.wizard.go(step);
        cx.notify();
    }

    pub fn start(&mut self, cx: &mut Context<Self>) {
        self.editing = None;
        let Some(run) = self.wizard.start() else {
            return;
        };
        let engine = self.wizard.engine();
        let worker = run.clone();
        let spawned = std::thread::Builder::new()
            .name("pstoff-conversion".into())
            .spawn(move || worker.execute(engine.as_ref()));
        if spawned.is_err() {
            // Nothing was converted: every OST ends as "not started".
            run.cancel();
            run.execute(self.wizard.engine().as_ref());
            self.wizard.notice = Some("PstOff couldn’t start the conversion.".into());
        }
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| loop {
            executor.timer(POLL_INTERVAL).await;
            let running = this
                .update(cx, |this, cx| {
                    this.wizard.poll();
                    cx.notify();
                    this.wizard.is_running()
                })
                .unwrap_or(false);
            if !running {
                break;
            }
        })
        .detach();
        cx.notify();
    }

    /// Closing the window mid-conversion asks to cancel first, so the engine
    /// can remove the unfinished PST.
    pub fn should_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.wizard.is_running() {
            self.wizard.request_cancel();
            cx.notify();
            false
        } else {
            true
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(id) = self.editing else {
            return;
        };
        let Some(mut name) = self.wizard.source(id).map(|s| s.base_name.clone()) else {
            self.editing = None;
            return;
        };
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "backspace" => {
                name.pop();
            }
            "enter" | "escape" | "tab" => {
                self.editing = None;
                cx.notify();
                return;
            }
            _ => {
                let typed = keystroke.key_char.as_deref().unwrap_or_default();
                let shortcut = keystroke.modifiers.control && !event.prefer_character_input;
                if shortcut || typed.is_empty() || typed.chars().any(char::is_control) {
                    return;
                }
                name.push_str(typed);
            }
        }
        self.wizard.set_base_name(id, name);
        cx.stop_propagation();
        cx.notify();
    }

    pub fn edit_base_name(&mut self, id: SourceId, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = Some(id);
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn heading(&self) -> (&'static str, &'static str) {
        match self.wizard.step {
            Step::Add => (
                "Which OST files do you want to convert?",
                "PstOff reads each OST and writes a new PST file from it. Your OST files are never changed.",
            ),
            Step::Folders => (
                "Choose the folders to keep",
                "Everything is ticked. Untick folders you don’t want in the PST.",
            ),
            Step::Output => (
                "Where should the PST files go?",
                "One PST per OST. Turn on Split to keep each file under a size you choose.",
            ),
            Step::Convert if self.wizard.has_run() => (
                "Converting…",
                "You can keep using your computer. Cancel stops the current file.",
            ),
            Step::Convert => ("Ready to convert", "Check the summary, then start."),
            Step::Report
                if self
                    .wizard
                    .outcomes()
                    .iter()
                    .any(|(_, o)| matches!(o, crate::run::Outcome::Cancelled | crate::run::Outcome::NotStarted)) =>
            {
                (
                    "Conversion cancelled",
                    "Here’s what was finished before you stopped.",
                )
            }
            Step::Report => (
                "Conversion finished",
                "Read this before you rely on the PST files.",
            ),
        }
    }

    fn rail(&self, cx: &Context<Self>) -> impl IntoElement {
        let current = self.wizard.step;
        let steps = Step::ALL.iter().enumerate().map(|(i, &step)| {
            let reachable = self.wizard.can_enter(step);
            let done = step < current && !self.wizard.is_running();
            let is_current = step == current;
            let color = if is_current {
                TEAL
            } else if reachable || done {
                INK
            } else {
                PEWTER
            };
            let dot = div()
                .size(px(26.))
                .flex_none()
                .rounded_full()
                .border_2()
                .border_color(rgb(color))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .when(is_current, |d| d.bg(rgb(TEAL)).text_color(rgb(PAPER)))
                .when(done && !is_current, |d| {
                    d.bg(rgb(INK)).text_color(rgb(PAPER))
                })
                .child(if done && !is_current {
                    "✓".to_string()
                } else {
                    (i + 1).to_string()
                });
            div()
                .id(("step", i))
                .debug_selector(move || format!("step-{i}"))
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(10.))
                .py(px(10.))
                .rounded(px(10.))
                .text_color(rgb(color))
                .when(is_current, |d| {
                    d.bg(rgb(PAPER)).font_weight(FontWeight::SEMIBOLD)
                })
                .when(reachable && !is_current, |d| {
                    d.cursor_pointer()
                        .hover(|s| s.bg(rgb(HAIRLINE)))
                        .on_click(cx.listener(move |this, _, _, cx| this.go(step, cx)))
                })
                .child(dot)
                .child(
                    div().flex().flex_col().child(step.title()).child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgb(PEWTER))
                            .child(step.subtitle()),
                    ),
                )
        });
        div()
            .w(px(248.))
            .flex_none()
            .h_full()
            .bg(rgb(MIST))
            .border_r_1()
            .border_color(rgb(HAIRLINE))
            .px(px(18.))
            .py(px(26.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .mx(px(8.))
                    .mb(px(22.))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .child("PstOff"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(PEWTER))
                            .child("OST to PST converter"),
                    ),
            )
            .children(steps)
            .child(div().flex_1())
            .child(widgets::copy_mode_note())
    }

    fn footer(&self, cx: &Context<Self>) -> impl IntoElement {
        let w = &self.wizard;
        let bar = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(px(44.))
            .py(px(14.))
            .border_t_1()
            .border_color(rgb(HAIRLINE))
            .bg(rgb(PAPER));
        let summary = |text: String| {
            div()
                .flex_1()
                .text_size(px(13.))
                .text_color(rgb(PEWTER))
                .child(text)
        };
        match w.step {
            Step::Add | Step::Folders | Step::Output => {
                let text = if w.step == Step::Add {
                    if w.sources.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "{} of {} ready",
                            w.ready().count(),
                            crate::format::plural(w.sources.len() as u64, "file", "files")
                        )
                    }
                } else {
                    let t = w.grand_totals();
                    format!(
                        "{}, {}, about {}",
                        crate::format::plural(w.ready().count() as u64, "OST", "OSTs"),
                        crate::format::plural(t.items, "item", "items"),
                        crate::format::bytes(t.bytes)
                    )
                };
                let next_label = if w.step == Step::Output {
                    "Review and convert"
                } else {
                    "Next"
                };
                bar.child(button(
                    self,
                    "back",
                    "Back",
                    ButtonKind::Secondary,
                    w.can_go_back(),
                    cx,
                    |this, _, cx| {
                        this.editing = None;
                        this.wizard.back();
                        cx.notify();
                    },
                ))
                .child(summary(text))
                .child(button(
                    self,
                    "next",
                    next_label,
                    ButtonKind::Primary,
                    w.can_go_next(),
                    cx,
                    |this, _, cx| {
                        this.editing = None;
                        this.wizard.next();
                        cx.notify();
                    },
                ))
            }
            Step::Convert if !w.has_run() => bar
                .child(button(
                    self,
                    "back",
                    "Back",
                    ButtonKind::Secondary,
                    w.can_go_back(),
                    cx,
                    |this, _, cx| {
                        this.wizard.back();
                        cx.notify();
                    },
                ))
                .child(summary(String::new()))
                .child(button(
                    self,
                    "start",
                    "Start conversion",
                    ButtonKind::Primary,
                    w.can_enter(Step::Convert),
                    cx,
                    |this, _, cx| this.start(cx),
                )),
            Step::Convert => {
                let run = w.run().expect("a run exists while converting");
                let state = run.state();
                let position = state
                    .current()
                    .map(|(i, _)| i + 1)
                    .unwrap_or_else(|| state.jobs.iter().filter(|j| j.fraction() >= 1.0).count());
                let text = if run.is_cancelled() {
                    "Stopping: removing the unfinished PST…".to_string()
                } else {
                    format!("Converting {} of {}", position.max(1), state.jobs.len())
                };
                bar.child(summary(text)).child(button(
                    self,
                    "cancel",
                    "Cancel",
                    ButtonKind::Danger,
                    !run.is_cancelled(),
                    cx,
                    |this, _, cx| {
                        this.wizard.request_cancel();
                        cx.notify();
                    },
                ))
            }
            Step::Report => bar
                .child(button(
                    self,
                    "start-over",
                    "Start over",
                    ButtonKind::Secondary,
                    !w.is_running(),
                    cx,
                    |this, _, cx| {
                        this.editing = None;
                        this.wizard.start_over();
                        cx.notify();
                    },
                ))
                .child(summary(String::new()))
                .when_some(w.output.destination.clone(), |bar, destination| {
                    bar.child(button(
                        self,
                        "open-folder",
                        "Open output folder",
                        ButtonKind::Primary,
                        true,
                        cx,
                        move |_, _, cx| cx.open_with_system(&destination),
                    ))
                }),
        }
    }

    fn cancel_dialog(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .id("cancel-dialog")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(rgba(0x1b1b1f66))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(460.))
                    .bg(rgb(PAPER))
                    .rounded(px(14.))
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Stop converting?"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(PEWTER))
                            .child("The PST being written now is removed so it can’t be mistaken for a full copy. PST files that are already finished are kept. OSTs still waiting are not started."),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(10.))
                            .mt(px(8.))
                            .child(button(self,
"keep-converting",
                                "Keep converting",
                                ButtonKind::Secondary,
                                true,
                                cx,
                                |this, _, cx| {
                                    this.wizard.keep_converting();
                                    cx.notify();
                                },
                            ))
                            .child(button(self,
"confirm-cancel",
                                "Stop and remove unfinished PST",
                                ButtonKind::DangerSolid,
                                true,
                                cx,
                                |this, _, cx| {
                                    this.wizard.confirm_cancel();
                                    cx.notify();
                                },
                            )),
                    ),
            )
    }
}

impl Render for WizardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        for request in self.wizard.plan_requests() {
            let engine = self.wizard.engine();
            let worker = request.clone();
            let task = cx
                .background_executor()
                .spawn(async move { engine.plan(&worker.path, &worker.options) });
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.wizard.apply_plan(&request, result);
                    cx.notify();
                });
            })
            .detach();
        }
        self.frames += 1;
        let (title, subtitle) = self.heading();
        let content = match self.wizard.step {
            Step::Add => self.add_screen(cx).into_any_element(),
            Step::Folders => self.folders_screen(cx).into_any_element(),
            Step::Output => self.output_screen(cx).into_any_element(),
            Step::Convert => self.convert_screen(cx).into_any_element(),
            Step::Report => self.report_screen(cx).into_any_element(),
        };
        div()
            .id("pstoff")
            .track_focus(&self.focus)
            .tab_group()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "tab" {
                    this.editing = None;
                    if event.keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                } else {
                    this.key_down(event, cx);
                }
            }))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                if this.wizard.step == Step::Add {
                    this.add_paths(paths.paths().to_vec(), cx);
                }
            }))
            .relative()
            .size_full()
            .flex()
            .bg(rgb(MIST))
            .text_color(rgb(INK))
            .text_size(px(14.))
            .font_family(SharedString::from(DISPLAY_FONT))
            .child(self.rail(cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex_none()
                            .px(px(44.))
                            .pt(px(34.))
                            .pb(px(10.))
                            .child(
                                div()
                                    .text_size(px(28.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .mt(px(6.))
                                    .text_size(px(15.))
                                    .text_color(rgb(PEWTER))
                                    .child(subtitle),
                            ),
                    )
                    .child(
                        div()
                            .id("content")
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_y_scroll()
                            .px(px(44.))
                            .pt(px(18.))
                            .pb(px(24.))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(760.))
                                    .child(div().w_full().max_w(px(760.)).child(content)),
                            ),
                    )
                    .child(self.footer(cx)),
            )
            .when(self.wizard.confirming_cancel, |d| {
                d.child(self.cancel_dialog(cx))
            })
    }
}

/// Opens the wizard window. Used by `main` and by the Sandbox smoke run.
pub fn open_window(
    engine: Arc<dyn Engine>,
    cx: &mut App,
) -> Result<gpui::WindowHandle<WizardView>, String> {
    let bounds = gpui::Bounds::centered(None, gpui::size(px(1120.), px(760.)), cx);
    cx.open_window(
        gpui::WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("PstOff".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| cx.new(|cx| WizardView::new(engine, window, cx)),
    )
    .map_err(|e| e.to_string())
}
