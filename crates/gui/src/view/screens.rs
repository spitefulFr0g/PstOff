//! The five wizard screens.
use super::palette::*;
use super::widgets::{
    bar, button, button_with, checkbox, chip, completeness_band, heading, identifier, list_row,
    note, panel, secondary, section, toggle, ButtonKind, ChipKind, NoteKind,
};
use super::WizardView;
use crate::format;
use crate::plan::{self, SPLIT_CHOICES_GIB};
use crate::report::{self, OstReport, SHOWN};
use crate::run::{JobState, Outcome};
use crate::selection::folder_path;
use crate::wizard::Status;
use gpui::{div, prelude::*, px, rgb, Context, Div, ExternalPaths, FontWeight, SharedString};

fn status_chip(status: &Status) -> Div {
    let kind = match status {
        Status::Ready => ChipKind::Ok,
        Status::Checking => ChipKind::Running,
        Status::InUse => ChipKind::Warn,
        Status::Unsupported(_) => ChipKind::Neutral,
        Status::Unreadable(_) => ChipKind::Bad,
    };
    chip(kind, status.label())
}

fn outcome_chip(outcome: Outcome) -> Div {
    let kind = match outcome {
        Outcome::Finished => ChipKind::Ok,
        Outcome::FinishedWithSkipped => ChipKind::Warn,
        Outcome::Running | Outcome::Waiting => ChipKind::Running,
        Outcome::Cancelled | Outcome::Failed => ChipKind::Bad,
        Outcome::NotStarted => ChipKind::Neutral,
    };
    chip(kind, outcome.label())
}

fn heading_row(title: impl Into<SharedString>, right: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.))
        .child(heading(title))
        .child(right)
}

impl WizardView {
    // ---- Add OST files -------------------------------------------------

    pub(super) fn add_screen(&self, cx: &Context<Self>) -> Div {
        let w = &self.wizard;
        let drop_zone = div()
            .id("drop-zone")
            .debug_selector(|| "drop-zone".into())
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .p(px(36.))
            .rounded(px(8.))
            .border_2()
            .border_dashed()
            .border_color(rgb(HAIRLINE_STRONG))
            .bg(rgb(TEAL_TINT))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_color(rgb(TEAL)))
            .child(div().text_size(px(34.)).child("⬇"))
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Drop OST files here"),
            )
            .child(secondary(
                "or browse for them. You can add several; each becomes its own PST.",
            ))
            .child(button(
                self,
                "browse",
                "Browse…",
                ButtonKind::Secondary,
                true,
                cx,
                |this, _, cx| this.browse_osts(cx),
            ));
        let files = w.sources.iter().enumerate().map(|(i, s)| {
            let id = s.id;
            let mut meta = s
                .file_bytes
                .map(format::bytes)
                .unwrap_or_else(|| "Size unknown".into());
            meta.push_str(", ");
            meta.push_str(&s.path.display().to_string());
            div()
                .flex()
                .items_center()
                .gap(px(14.))
                .px(px(16.))
                .py(px(14.))
                .bg(rgb(PAPER))
                .border_1()
                .border_color(rgb(HAIRLINE))
                .rounded(px(8.))
                .child(
                    div()
                        .w(px(38.))
                        .h(px(46.))
                        .flex_none()
                        .rounded(px(6.))
                        .bg(rgb(TEAL_TINT))
                        .text_color(rgb(TEAL))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(11.))
                        .font_weight(FontWeight::BOLD)
                        .child("OST"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(s.name()))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(PEWTER))
                                .truncate()
                                .child(meta),
                        )
                        .when_some(s.status.explanation(), |d, text| {
                            d.child(div().mt(px(6.)).child(note(
                                if s.status == Status::InUse {
                                    NoteKind::Warn
                                } else {
                                    NoteKind::Info
                                },
                                "",
                                text,
                            )))
                        }),
                )
                .child(status_chip(&s.status))
                .when(
                    matches!(s.status, Status::InUse | Status::Unreadable(_)),
                    |d| {
                        d.child(button_with(
                            self,
                            ("retry", i).into(),
                            format!("retry-{i}"),
                            "Retry",
                            ButtonKind::Secondary,
                            true,
                            cx,
                            move |this, _, cx| this.retry(id, cx),
                        ))
                    },
                )
                .child(button_with(
                    self,
                    ("remove", i).into(),
                    format!("remove-{i}"),
                    "×",
                    ButtonKind::Quiet,
                    true,
                    cx,
                    move |this, _, cx| {
                        this.wizard.remove(id);
                        cx.notify();
                    },
                ))
        });
        div()
            .flex()
            .flex_col()
            .child(drop_zone)
            .child(
                div()
                    .mt(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .children(files),
            )
            .when_some(w.notice.clone(), |d, text| {
                d.child(div().mt(px(14.)).child(note(NoteKind::Info, "", text)))
            })
            .child(div().mt(px(18.)).child(secondary(
                "Outlook usually keeps OST files in %LOCALAPPDATA%\\Microsoft\\Outlook.",
            )))
    }

    // ---- Choose folders ------------------------------------------------

    pub(super) fn folders_screen(&self, cx: &Context<Self>) -> Div {
        let w = &self.wizard;
        let Some(source) = w.pill() else {
            return div().child(secondary("Add an OST that is Ready to see its folders."));
        };
        let source_id = source.id;
        let ready: Vec<_> = w.ready().collect();
        let pills =
            (ready.len() > 1).then(|| {
                div().flex().flex_wrap().gap(px(8.)).mb(px(14.)).children(
                    ready.iter().enumerate().map(|(i, s)| {
                        let id = s.id;
                        let on = id == source_id;
                        div()
                            .id(("pill", i))
                            .debug_selector(move || format!("pill-{i}"))
                            .px(px(14.))
                            .py(px(6.))
                            .rounded_full()
                            .border_1()
                            .text_size(px(13.))
                            .cursor_pointer()
                            .map(|d| {
                                if on {
                                    d.bg(rgb(INK)).border_color(rgb(INK)).text_color(rgb(PAPER))
                                } else {
                                    d.bg(rgb(PAPER)).border_color(rgb(HAIRLINE_STRONG))
                                }
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.wizard.select_pill(id);
                                cx.notify();
                            }))
                            .child(s.name())
                    }),
                )
            });
        let tree = source.folders();
        let rows = source
            .selection
            .visible_rows(tree)
            .into_iter()
            .enumerate()
            .map(|(i, (depth, folder))| {
                let tick = source.selection.tick(folder);
                let off = tick == crate::selection::Tick::Off;
                let folder_ref = folder.id.clone();
                let expand_ref = folder.id.clone();
                let has_children = !folder.children.is_empty();
                let expanded = source.selection.is_expanded(&folder.id);
                let name = if folder.name.is_empty() {
                    "(unnamed)".to_string()
                } else {
                    folder.name.clone()
                };
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .pr(px(10.))
                    .py(px(7.))
                    .pl(px(10. + depth as f32 * 26.))
                    .rounded(px(8.))
                    .child(
                        div()
                            .id(("twisty", i))
                            .w(px(16.))
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(rgb(PEWTER))
                            .when(has_children, |d| {
                                d.cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.wizard.toggle_expanded(source_id, &expand_ref);
                                        cx.notify();
                                    }))
                                    .child(if expanded { "▾" } else { "▸" })
                            }),
                    )
                    .child(
                        div()
                            .id(("folder", i))
                            .debug_selector(move || format!("folder-{i}"))
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.wizard.toggle_folder(source_id, &folder_ref);
                                cx.notify();
                            }))
                            .child(checkbox(tick))
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .when(off, |d| d.text_color(rgb(PEWTER)).line_through())
                                    .child(name),
                            ),
                    )
                    .when(folder.unreadable.is_some(), |d| {
                        d.child(chip(ChipKind::Bad, "Can’t be read: will be skipped"))
                    })
                    .child(
                        div()
                            .min_w(px(64.))
                            .flex()
                            .justify_end()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(if off { PEWTER } else { INK }))
                            .child(if folder.item_count > 0 {
                                format::count(folder.item_count)
                            } else {
                                "—".into()
                            }),
                    )
                    .child(
                        div()
                            .min_w(px(70.))
                            .flex()
                            .justify_end()
                            .text_size(px(12.5))
                            .text_color(rgb(PEWTER))
                            .child(if folder.bytes > 0 {
                                format::bytes(folder.bytes)
                            } else {
                                String::new()
                            }),
                    )
            });
        let totals = source.totals();
        let summary = panel()
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(heading("Going into the PST"))
            .child(
                div()
                    .debug_selector(|| "running-total".into())
                    .flex()
                    .items_end()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(34.))
                            .font_weight(FontWeight::BOLD)
                            .child(format::count(totals.items)),
                    )
                    .child(
                        div()
                            .pb(px(6.))
                            .text_color(rgb(PEWTER))
                            .child(if totals.items == 1 { "item" } else { "items" }),
                    ),
            )
            .child(secondary(format!(
                "{} of {} folders, about {}",
                format::count(totals.folders),
                format::count(totals.all_folders),
                format::bytes(totals.bytes)
            )))
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .gap(px(14.))
                    .child(button(
                        self,
                        "tick-all",
                        "Tick all",
                        ButtonKind::Quiet,
                        true,
                        cx,
                        move |this, _, cx| {
                            this.wizard.set_all(source_id, true);
                            cx.notify();
                        },
                    ))
                    .child(button(
                        self,
                        "untick-all",
                        "Untick all",
                        ButtonKind::Quiet,
                        true,
                        cx,
                        move |this, _, cx| {
                            this.wizard.set_all(source_id, false);
                            cx.notify();
                        },
                    )),
            );
        let structure = panel().p(px(18.)).flex().flex_col().gap(px(8.))
            .child(heading("Folder structure is kept"))
            .child(secondary("The PST gets the same folders, in the same places. Unticked folders are left out with their subfolders."));
        div()
            .flex()
            .flex_col()
            .children(pills)
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(18.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .bg(rgb(PAPER))
                            .border_1()
                            .border_color(rgb(HAIRLINE))
                            .rounded(px(8.))
                            .p(px(6.))
                            .children(rows),
                    )
                    .child(
                        div()
                            .w(px(224.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(14.))
                            .child(summary)
                            .child(structure)
                            .when(totals.unreadable > 0, |d| {
                                d.child(note(
                                    NoteKind::Warn,
                                    "Some folders can’t be read",
                                    format!(
                                        "{} marked in the tree will become Skipped items in the report.",
                                        format::plural(totals.unreadable, "folder", "folders")
                                    ),
                                ))
                            }),
                    ),
            )
    }

    // ---- Output --------------------------------------------------------

    pub(super) fn output_screen(&self, cx: &Context<Self>) -> Div {
        let w = &self.wizard;
        let destination = w
            .output
            .destination
            .as_ref()
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "No folder chosen yet".into());
        let names = w.ready().enumerate().map(|(i, s)| {
            let id = s.id;
            let editing = self.editing == Some(id);
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .mt(px(10.))
                .child(div().w(px(220.)).truncate().child(s.name()))
                .child(
                    div()
                        .id(("base-name", i))
                        .debug_selector(move || format!("base-name-{i}"))
                        .flex_1()
                        .border_1()
                        .border_color(rgb(if editing { TEAL } else { HAIRLINE_STRONG }))
                        .rounded(px(8.))
                        .px(px(12.))
                        .py(px(9.))
                        .bg(rgb(PAPER))
                        .font_family(SharedString::from("Cascadia Mono"))
                        .text_size(px(13.))
                        .cursor(gpui::CursorStyle::IBeam)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_base_name(id, window, cx)
                        }))
                        .child(format!("{}{}", s.base_name, if editing { "|" } else { "" })),
                )
                .child(div().text_color(rgb(PEWTER)).child(".pst"))
        });
        let split = w.output.split;
        let split_card = section("", "")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex_1()
                            .child(heading("Split into smaller PSTs"))
                            .child(secondary("Each PST stays under the size you pick. Items are never cut in half, and every part keeps the folder layout.")),
                    )
                    .child(
                        div()
                            .id("split")
                            .debug_selector(|| "split".into())
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.wizard.set_split(!split);
                                cx.notify();
                            }))
                            .child(toggle(split)),
                    ),
            )
            .when(split, |d| {
                d.child(
                    div()
                        .mt(px(12.))
                        .flex()
                        .children(SPLIT_CHOICES_GIB.iter().map(|&gib| {
                            let on = w.output.split_gib == gib;
                            div()
                                .id(("gib", gib as usize))
                                .debug_selector(move || format!("gib-{gib}"))
                                .px(px(14.))
                                .py(px(7.))
                                .border_1()
                                .border_color(rgb(HAIRLINE_STRONG))
                                .cursor_pointer()
                                .map(|d| {
                                    if on {
                                        d.bg(rgb(INK)).text_color(rgb(PAPER))
                                    } else {
                                        d.bg(rgb(PAPER))
                                    }
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.wizard.set_split_gib(gib);
                                    cx.notify();
                                }))
                                .child(format!("{gib} GB"))
                        })),
                )
            });
        let plans: Vec<_> = w
            .ready()
            .filter_map(|s| w.plan_for(s.id).map(|p| (s, p)))
            .collect();
        let file_count: usize = plans.iter().map(|(_, p)| p.files.len()).sum();
        let preview = panel()
            .p(px(18.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .debug_selector(|| "preview".into())
            .child(heading(format!(
                "Estimated output: {}",
                format::plural(file_count as u64, "PST file", "PST files")
            )))
            .child(secondary(
                "Sizes include PST structure and whole Items. Planning does not create output files.",
            ))
            .children(plans.iter().map(|(s, p)| {
                div()
                    .mt(px(14.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(s.name()),
                    )
                    .child(div().mt(px(8.)).flex().flex_wrap().gap(px(10.)).children(
                        p.files.iter().map(|f| {
                            div()
                                .w(px(180.))
                                .p(px(12.))
                                .rounded(px(10.))
                                .bg(rgb(MIST))
                                .border_1()
                                .border_color(rgb(if f.exists { FAULT } else { HAIRLINE }))
                                .text_size(px(12.))
                                .child(div().truncate().child(f.name.clone()))
                                .child(
                                    div()
                                        .mt(px(4.))
                                        .text_size(px(18.))
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!("~{}", format::bytes(f.estimated_bytes))),
                                )
                                .when(f.exists, |d| {
                                    d.child(div().text_color(rgb(FAULT)).child("Already exists"))
                                })
                        }),
                    ))
                    .when(p.over_outlook_limit, |d| {
                        d.child(div().mt(px(12.)).child(note(
                            NoteKind::Warn,
                            "This PST would be over Outlook’s size limit",
                            plan::over_limit_text(p.estimated_bytes),
                        )))
                    })
            }));
        let problems = w.output_problems();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                section("Save PST files to", "Choose a folder for the new PST files and reports.").child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .mt(px(10.))
                        .child(
                            div()
                                .flex_1()
                                .border_1()
                                .border_color(rgb(HAIRLINE_STRONG))
                                .rounded(px(8.))
                                .px(px(12.))
                                .py(px(9.))
                                .bg(rgb(PAPER))
                                .font_family(SharedString::from("Cascadia Mono"))
                                .text_size(px(13.))
                                .truncate()
                                .child(destination),
                        )
                        .child(button(self,
"destination",
                            "Browse…",
                            ButtonKind::Secondary,
                            true,
                            cx,
                            |this, _, cx| this.browse_destination(cx),
                        )),
                ),
            )
            .child(
                section("PST names", "")
                    .child(secondary("Each PST is named after its OST. Click a name to change it. In Outlook, each PST is shown as “PstOff – <OST file name>”."))
                    .children(names),
            )
            .child(
                section("Copy mode", "")
                    .child(secondary("PstOff only reads each OST and writes a new PST. The OST is never changed, and PstOff never connects to or deletes from the mailbox.")),
            )
            .child(split_card)
            .child(preview)
            .when(!problems.is_empty(), |d| {
                d.child(note(
                    NoteKind::Warn,
                    "Before you go on",
                    problems.join("\n"),
                ))
            })
    }

    // ---- Convert -------------------------------------------------------

    pub(super) fn convert_screen(&self, cx: &Context<Self>) -> Div {
        let w = &self.wizard;
        let Some(run) = w.run() else {
            return self.summary(cx);
        };
        let state = run.state();
        let overall = state.overall();
        let current = state.current().map(|(_, job)| {
            let tree = w
                .source(job.source)
                .map(|s| s.folders())
                .unwrap_or_default();
            match &job.state {
                JobState::Running(p) if p.current_folder.is_some() => format!(
                    "Copying {} from {}",
                    folder_path(tree, p.current_folder.as_ref().unwrap()),
                    job.name
                ),
                JobState::Running(p) if p.fraction >= 0.95 => format!("Finishing {}", job.name),
                _ => format!("Writing {}", job.name),
            }
        });
        let rows = state.jobs.iter().enumerate().map(|(i, job)| {
            let outcome = job.outcome();
            let detail = match &job.state {
                JobState::Running(p) if p.total > 0 => format!(
                    "{} of {} items, part {}",
                    format::count(p.done),
                    format::count(p.total),
                    p.part
                ),
                JobState::Running(p) => format!("{}%", format::percent(p.fraction)),
                _ => outcome.label().to_string(),
            };
            div()
                .debug_selector(move || format!("job-{i}"))
                .bg(rgb(PAPER))
                .border_1()
                .border_color(rgb(HAIRLINE))
                .rounded(px(8.))
                .px(px(16.))
                .py(px(14.))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(px(10.))
                        .mb(px(8.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(job.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgb(PEWTER))
                                .child(detail),
                        ),
                )
                .child(bar(job.fraction(), 8.))
        });
        div()
            .flex()
            .flex_col()
            .pt(px(10.))
            .child(
                div()
                    .debug_selector(|| "overall".into())
                    .text_size(px(64.))
                    .font_weight(FontWeight::BOLD)
                    .child(format!("{}%", format::percent(overall))),
            )
            .child(
                div()
                    .text_color(rgb(PEWTER))
                    .child(current.unwrap_or_else(|| "Finishing…".into())),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(720.))
                    .mt(px(18.))
                    .child(bar(overall, 10.)),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(720.))
                    .mt(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .children(rows),
            )
    }

    fn summary(&self, _cx: &Context<Self>) -> Div {
        let w = &self.wizard;
        let items = w.ready().map(|s| {
            let parts = w.plan_for(s.id).map(|p| p.files.len()).unwrap_or(1) as u64;
            list_row()
                .child(
                    div()
                        .flex_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(s.name()),
                )
                .child(div().text_color(rgb(PEWTER)).child(format!(
                    "{} → {}",
                    format::plural(s.totals().items, "item", "items"),
                    format::plural(parts, "PST", "PSTs")
                )))
        });
        let left_out = w.sources.iter().filter(|s| !s.is_ready()).count() as u64;
        let over: Vec<_> = w
            .ready()
            .filter(|s| w.plan_for(s.id).is_some_and(|p| p.over_outlook_limit))
            .map(|s| s.name())
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                panel()
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(heading("Summary"))
                    .child(
                        div()
                            .mt(px(10.))
                            .flex()
                            .flex_col()
                            .children(items)
                            .child(
                                list_row()
                                    .child(
                                        div()
                                            .flex_1()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Destination"),
                                    )
                                    .child(
                                        div().text_color(rgb(PEWTER)).child(
                                            w.output
                                                .destination
                                                .as_ref()
                                                .map(|d| d.display().to_string())
                                                .unwrap_or_default(),
                                        ),
                                    ),
                            )
                            .child(
                                list_row()
                                    .child(
                                        div()
                                            .flex_1()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Split"),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(PEWTER))
                                            .child(w.output.describe_split()),
                                    ),
                            )
                            .child(
                                list_row()
                                    .child(
                                        div()
                                            .flex_1()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Mode"),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(PEWTER))
                                            .child("Copy: the OST is only read"),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "completeness-before".into())
                    .child(completeness_band(&report::completeness_statement(w))),
            )
            .when(!over.is_empty(), |d| {
                d.child(note(
                    NoteKind::Warn,
                    "A PST would be over Outlook’s 50 GB limit",
                    format!("{}: go back to Output and turn on Split.", over.join(", ")),
                ))
            })
            .when(left_out > 0, |d| {
                d.child(note(
                    NoteKind::Info,
                    "",
                    format!(
                        "{} left out because {} in use, not supported or can’t be read.",
                        format::plural(left_out, "file is", "files are"),
                        if left_out == 1 { "it is" } else { "they are" }
                    ),
                ))
            })
    }

    // ---- Report --------------------------------------------------------

    pub(super) fn report_screen(&self, cx: &Context<Self>) -> Div {
        let statement = report::completeness_statement(&self.wizard);
        let reports = report::build(&self.wizard);
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .debug_selector(|| "completeness".into())
                    .child(completeness_band(&statement)),
            )
            .children(
                reports
                    .into_iter()
                    .enumerate()
                    .map(|(i, r)| ost_report(self, i, r, cx)),
            )
    }
}

fn ost_report(view: &WizardView, index: usize, r: OstReport, cx: &Context<WizardView>) -> Div {
    let section = |title: &str| {
        div()
            .mt(px(16.))
            .text_size(px(14.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(title.to_string())
    };
    let kv = |value: String, label: &'static str| {
        div()
            .flex()
            .flex_col()
            .text_size(px(12.5))
            .text_color(rgb(PEWTER))
            .child(
                div()
                    .text_size(px(20.))
                    .text_color(rgb(INK))
                    .font_weight(FontWeight::BOLD)
                    .child(value),
            )
            .child(label)
    };
    let complete = matches!(r.outcome, Outcome::Finished | Outcome::FinishedWithSkipped);
    let more = |n: usize| {
        (n > SHOWN).then(|| {
            secondary(format!(
                "…and {} more in the saved report.",
                format::count((n - SHOWN) as u64)
            ))
        })
    };
    panel()
        .p(px(18.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .debug_selector(move || format!("report-{index}"))
        .child(heading_row(r.name.clone(), outcome_chip(r.outcome)))
        .when_some(r.message.clone(), |d, m| {
            d.child(div().mt(px(8.)).child(secondary(m)))
        })
        .when(complete, |d| {
            d.child(section("Missing-content evidence"))
                .children(r.evidence.iter().enumerate().map(|(n, e)| {
                    list_row()
                        .debug_selector(move || format!("evidence-{index}-{n}"))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(div().font_weight(FontWeight::SEMIBOLD).child(e.label))
                                .child(div().text_color(rgb(PEWTER)).child(e.note))
                                .children(
                                    e.references
                                        .iter()
                                        .take(SHOWN)
                                        .map(|r| div().text_color(rgb(PEWTER)).child(r.clone())),
                                ),
                        )
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(e.value.clone()),
                        )
                }))
                .child(
                    div()
                        .mt(px(16.))
                        .flex()
                        .gap(px(40.))
                        .child(kv(format::count(r.items), "items converted"))
                        .child(kv(format::count(r.folders), "folders"))
                        .child(kv(format::count(r.skipped.len() as u64), "skipped items")),
                )
        })
        .when(!r.files.is_empty(), |d| {
            d.child(section(if complete {
                "Files written"
            } else {
                "Finished files kept"
            }))
            .children(r.files.iter().map(|f| {
                list_row()
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .child(f.path.display().to_string()),
                    )
                    .when(r.files.len() > 1, |d| {
                        d.child(
                            div()
                                .text_color(rgb(PEWTER))
                                .child(format!("part {}", f.part)),
                        )
                    })
                    .child(div().text_color(rgb(PEWTER)).child(f.size.clone()))
            }))
        })
        .when(!r.folder_parts.is_empty(), |d| {
            d.child(section("Which part each folder went to")).children(
                r.folder_parts.iter().enumerate().map(|(n, f)| {
                    list_row()
                        .debug_selector(move || format!("folder-part-{index}-{n}"))
                        .child(div().flex_1().truncate().child(f.folder.clone()))
                        .child(div().text_color(rgb(PEWTER)).child(format!(
                            "part {}, {}",
                            f.part,
                            format::plural(f.items, "item", "items")
                        )))
                }),
            )
        })
        .when(complete && !r.excluded_folders.is_empty(), |d| {
            d.child(section("Excluded folders"))
                .child(secondary(
                    "These selections and their subfolders were left out.",
                ))
                .children(
                    r.excluded_folders
                        .iter()
                        .take(SHOWN)
                        .map(|path| list_row().child(path.clone())),
                )
                .children(more(r.excluded_folders.len()))
        })
        .when(complete && !r.skipped.is_empty(), |d| {
            d.child(section("Skipped items"))
                .child(secondary(
                    "These couldn’t be read from the OST, so they aren’t in the PST.",
                ))
                .children(
                    r.skipped
                        .iter()
                        .take(SHOWN)
                        .enumerate()
                        .map(|(n, (what, why))| {
                            list_row()
                                .debug_selector(move || format!("skipped-{index}-{n}"))
                                .child(
                                    div()
                                        .flex_1()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(what.clone()),
                                )
                                .child(
                                    div()
                                        .max_w(px(320.))
                                        .text_color(rgb(PEWTER))
                                        .child(why.clone()),
                                )
                        }),
                )
                .children(more(r.skipped.len()))
        })
        .when(complete, |d| {
            d.child(section("Explained differences"))
                .child(secondary(if r.differences.is_empty() {
                    "None recorded. See the saved report for the checks that ran."
                } else {
                    "Where the PST differs from the OST, and why."
                }))
                .children(r.differences.iter().take(SHOWN).enumerate().map(|(n, x)| {
                    list_row()
                        .debug_selector(move || format!("difference-{index}-{n}"))
                        .child(identifier(x.property.clone()).w(px(110.)))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(x.item.clone()),
                                )
                                .child(
                                    div()
                                        .text_color(rgb(PEWTER))
                                        .child(format!("{}, {}", x.source, x.reason)),
                                ),
                        )
                }))
                .children(more(r.differences.len()))
        })
        .when_some(r.saved_report.clone(), |d, path| {
            let folder = path.parent().map(|p| p.to_path_buf());
            d.child(
                div()
                    .mt(px(16.))
                    .flex()
                    .gap(px(10.))
                    .child(button_with(
                        view,
                        ("open-report", index).into(),
                        format!("open-report-{index}"),
                        "Open saved report",
                        ButtonKind::Secondary,
                        true,
                        cx,
                        move |_, _, cx| cx.open_with_system(&path),
                    ))
                    .when_some(folder, |d, folder| {
                        d.child(button_with(
                            view,
                            ("open-ost-folder", index).into(),
                            format!("open-ost-folder-{index}"),
                            "Show in folder",
                            ButtonKind::Secondary,
                            true,
                            cx,
                            move |_, _, cx| cx.open_with_system(&folder),
                        ))
                    }),
            )
        })
}
