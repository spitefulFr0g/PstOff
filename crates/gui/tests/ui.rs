//! Headless GPUI tests: the wizard window, clicked through with GPUI's test
//! context, backed by the real engine on generated OSTs.
#![cfg(windows)]
mod support;

use gpui::{
    point, px, Entity, ExternalPaths, FileDropEvent, Modifiers, TestAppContext, VisualTestContext,
};
use pstoff_gui::run::Outcome;
use pstoff_gui::view::{WizardView, POLL_INTERVAL};
use pstoff_gui::{Engine, Status, Step};
use std::path::PathBuf;
use std::sync::Arc;
use support::*;

fn open(
    cx: &mut TestAppContext,
    engine: Arc<dyn Engine>,
) -> (Entity<WizardView>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| WizardView::new(engine, window, cx))
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("`{selector}` isn't on screen"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.run_until_parked();
    cx.debug_bounds(selector).is_some()
}

fn step(view: &Entity<WizardView>, cx: &mut VisualTestContext) -> Step {
    view.read_with(cx, |v, _| v.wizard.step)
}

fn add(view: &Entity<WizardView>, cx: &mut VisualTestContext, paths: Vec<PathBuf>) {
    view.update(cx, |v, cx| v.add_paths(paths, cx));
    cx.run_until_parked();
}

/// Lets the worker thread run while the UI keeps polling it.
fn pump_until(
    view: &Entity<WizardView>,
    cx: &mut VisualTestContext,
    done: impl Fn(&WizardView) -> bool,
) {
    for _ in 0..2000 {
        cx.executor().advance_clock(POLL_INTERVAL);
        cx.run_until_parked();
        if view.read_with(cx, |v, _| done(v)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the wizard never reached the expected state");
}

/// Clicks a Browse button, then answers the folder or file prompt it opened.
fn answer_prompt(
    cx: &mut VisualTestContext,
    selector: &'static str,
    answer: impl FnOnce(&gpui::PathPromptOptions) -> Option<Vec<PathBuf>>,
) {
    click(cx, selector);
    assert!(cx.did_prompt_for_paths(), "`{selector}` opened no prompt");
    cx.simulate_path_prompt_response(answer);
    cx.run_until_parked();
}

fn choose_destination(cx: &mut VisualTestContext, out: PathBuf) {
    answer_prompt(cx, "destination", move |options| {
        assert!(options.directories && !options.files);
        Some(vec![out])
    });
}

#[gpui::test]
fn back_and_next_step_through_the_rail(cx: &mut TestAppContext) {
    let t = Temp::new();
    let (view, cx) = open(cx, real());
    assert_eq!(step(&view, cx), Step::Add);
    click(cx, "next");
    assert_eq!(step(&view, cx), Step::Add, "Next waits for a Ready OST");

    add(&view, cx, vec![t.write("m.ost", &mailbox())]);
    click(cx, "next");
    assert_eq!(step(&view, cx), Step::Folders);
    click(cx, "next");
    assert_eq!(step(&view, cx), Step::Output);
    click(cx, "next");
    assert_eq!(
        step(&view, cx),
        Step::Output,
        "Next waits for a destination"
    );
    choose_destination(cx, t.out());
    click(cx, "next");
    assert_eq!(step(&view, cx), Step::Convert);
    click(cx, "back");
    assert_eq!(step(&view, cx), Step::Output);
    click(cx, "back");
    assert_eq!(step(&view, cx), Step::Folders);
    click(cx, "step-3");
    assert_eq!(
        step(&view, cx),
        Step::Convert,
        "reachable steps are clickable"
    );
    click(cx, "step-4");
    assert_eq!(
        step(&view, cx),
        Step::Convert,
        "Report waits for a conversion"
    );
    click(cx, "step-0");
    assert_eq!(step(&view, cx), Step::Add);
}

#[gpui::test]
fn browse_drop_remove_and_each_status(cx: &mut TestAppContext) {
    let t = Temp::new();
    let ready = t.write("ready.ost", &mailbox());
    let ansi = t.write("ansi.ost", &unsupported(15, 0));
    let locked = t.write("locked.ost", &mailbox());
    let (view, cx) = open(cx, real());

    let paths = vec![ready.clone(), ansi.clone()];
    answer_prompt(cx, "browse", move |options| {
        assert!(options.files && options.multiple);
        Some(paths)
    });
    let statuses = view.read_with(cx, |v, _| {
        v.wizard
            .sources
            .iter()
            .map(|s| s.status.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(statuses[0], Status::Ready);
    assert!(matches!(statuses[1], Status::Unsupported(_)));

    let held = hold_exclusively(&locked);
    let position = point(px(600.), px(300.));
    cx.simulate_event(FileDropEvent::Entered {
        position,
        paths: ExternalPaths(vec![locked.clone(), t.root.join("readme.txt")].into()),
    });
    cx.simulate_event(FileDropEvent::Submit { position });
    cx.run_until_parked();
    let (count, status, notice) = view.read_with(cx, |v, _| {
        (
            v.wizard.sources.len(),
            v.wizard.sources[2].status.clone(),
            v.wizard.notice.clone(),
        )
    });
    assert_eq!(count, 3);
    assert_eq!(status, Status::InUse);
    assert!(notice.unwrap().contains("only .ost files"));

    click(cx, "retry-2");
    assert_eq!(
        view.read_with(cx, |v, _| v.wizard.sources[2].status.clone()),
        Status::InUse
    );
    drop(held);
    click(cx, "retry-2");
    assert_eq!(
        view.read_with(cx, |v, _| v.wizard.sources[2].status.clone()),
        Status::Ready
    );

    click(cx, "remove-1");
    let names = view.read_with(cx, |v, _| {
        v.wizard
            .sources
            .iter()
            .map(|s| s.name())
            .collect::<Vec<_>>()
    });
    assert_eq!(names, ["ready.ost", "locked.ost"]);
}

#[gpui::test]
fn ticking_folders_updates_the_total_and_pills_switch(cx: &mut TestAppContext) {
    let t = Temp::new();
    let (view, cx) = open(cx, real());
    add(
        &view,
        cx,
        vec![t.write("a.ost", &mailbox()), t.write("b.ost", &mailbox())],
    );
    click(cx, "next");
    assert!(shown(cx, "running-total"));
    let totals = |cx: &mut VisualTestContext| {
        view.read_with(cx, |v, _| {
            v.wizard
                .sources
                .iter()
                .map(|s| s.totals().items)
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(totals(cx), [6, 6]);
    // Rows run depth first: Root, Root - Mailbox, Finder, IPM_SUBTREE, Inbox, Projects,
    // Sent Items, Deleted Items.
    click(cx, "folder-4");
    assert_eq!(totals(cx), [1, 6], "Inbox and its subfolder are left out");
    click(cx, "folder-5");
    assert_eq!(totals(cx), [6, 6], "ticking Projects brings back Inbox");
    click(cx, "untick-all");
    assert_eq!(totals(cx), [0, 6]);
    click(cx, "tick-all");
    assert_eq!(totals(cx), [6, 6]);

    click(cx, "pill-1");
    click(cx, "folder-6");
    assert_eq!(totals(cx), [6, 5], "each OST keeps its own ticks");
    let pill = view.read_with(cx, |v, _| v.wizard.pill().map(|s| s.name()));
    assert_eq!(pill.as_deref(), Some("b.ost"));
}

#[gpui::test]
fn output_choices_reach_the_convert_options(cx: &mut TestAppContext) {
    let t = Temp::new();
    let (view, cx) = open(cx, real());
    add(&view, cx, vec![t.write("mailbox.ost", &mailbox())]);
    click(cx, "next");
    click(cx, "next");
    choose_destination(cx, t.out());
    assert!(shown(cx, "preview"));

    click(cx, "base-name-0");
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace backspace backspace");
    cx.simulate_keystrokes("a r c h i v e enter");
    click(cx, "split");
    click(cx, "gib-10");
    let (options, preview) = view.read_with(cx, |v, _| {
        let id = v.wizard.sources[0].id;
        (
            v.wizard.convert_options(id).unwrap(),
            v.wizard.plan_for(id).unwrap(),
        )
    });
    assert_eq!(options.base_name, "archive");
    assert_eq!(options.destination, t.out());
    assert_eq!(
        options.split,
        pstoff_engine::Split::Bytes(10 * pstoff_gui::format::GIB)
    );
    assert_eq!(preview.files[0].name, "archive-part001.pst");
    assert!(
        view.read_with(cx, |v, _| v.editing.is_none()),
        "Enter ends editing"
    );

    click(cx, "split");
    let split = view.read_with(cx, |v, _| v.wizard.output.split_limit());
    assert_eq!(split, pstoff_engine::Split::Off);
}

#[gpui::test]
fn converting_keeps_drawing_and_cancel_asks_first(cx: &mut TestAppContext) {
    let t = Temp::new();
    let gate = Gate::default();
    let (view, cx) = open(cx, Arc::new(GatedEngine::new(gate.clone(), 0)));
    add(
        &view,
        cx,
        vec![t.write("a.ost", &mailbox()), t.write("b.ost", &mailbox())],
    );
    click(cx, "step-2");
    choose_destination(cx, t.out());
    click(cx, "next");
    assert!(
        shown(cx, "completeness-before"),
        "the note is shown before converting"
    );
    click(cx, "start");
    gate.wait_until_held();

    // The engine is blocked on its worker thread; the UI keeps drawing.
    let before = view.read_with(cx, |v, _| v.frames);
    for _ in 0..5 {
        cx.executor().advance_clock(POLL_INTERVAL);
        cx.run_until_parked();
    }
    let after = view.read_with(cx, |v, _| v.frames);
    assert!(after > before + 3, "frames {before} → {after}");
    assert!(shown(cx, "overall") && shown(cx, "job-0") && shown(cx, "job-1"));

    assert!(
        !cx.simulate_close(),
        "closing mid-conversion asks to cancel"
    );
    assert!(view.read_with(cx, |v, _| v.wizard.confirming_cancel));
    click(cx, "keep-converting");
    assert!(!view.read_with(cx, |v, _| v.wizard.confirming_cancel));
    click(cx, "cancel");
    assert!(shown(cx, "confirm-cancel"));
    click(cx, "confirm-cancel");
    gate.release();
    pump_until(&view, cx, |v| v.wizard.step == Step::Report);

    let outcomes = view.read_with(cx, |v, _| {
        v.wizard
            .outcomes()
            .into_iter()
            .map(|(_, o)| o)
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes, [Outcome::Cancelled, Outcome::NotStarted]);
    assert!(
        t.outputs().is_empty(),
        "no partial PST remains: {:?}",
        t.outputs()
    );
    assert!(shown(cx, "report-1"));
}

#[gpui::test]
fn report_opens_with_the_completeness_note(cx: &mut TestAppContext) {
    let t = Temp::new();
    let (view, cx) = open(cx, real());
    add(
        &view,
        cx,
        vec![t.write("a.ost", &mailbox()), t.write("b.ost", &mailbox())],
    );
    click(cx, "step-2");
    choose_destination(cx, t.out());
    click(cx, "next");
    click(cx, "start");
    pump_until(&view, cx, |v| v.wizard.step == Step::Report);
    let note = cx.debug_bounds("completeness").expect("completeness note");
    let first = cx.debug_bounds("report-0").expect("first OST report");
    let second = cx.debug_bounds("report-1").expect("second OST report");
    assert!(note.top() < first.top() && first.top() < second.top());
    assert!(shown(cx, "open-report-0"), "a link opens the saved report");
    let outcomes = view.read_with(cx, |v, _| {
        v.wizard
            .outcomes()
            .into_iter()
            .map(|(_, o)| o)
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes, [Outcome::Finished, Outcome::Finished]);
    assert_eq!(
        t.outputs(),
        ["a.json", "a.pst", "a.txt", "b.json", "b.pst", "b.txt"]
    );
    click(cx, "start-over");
    assert_eq!(step(&view, cx), Step::Add);
}

#[gpui::test]
fn report_renders_evidence_split_skips_and_differences(cx: &mut TestAppContext) {
    let t = Temp::new();
    let engine = Arc::new(ScriptedEngine::default());
    let (view, cx) = open(cx, engine.clone());
    add(&view, cx, vec![t.write("m.ost", &mailbox())]);
    let (inbox, sent) = view.read_with(cx, |v, _| {
        let tree = v.wizard.sources[0].folders();
        (
            folder_named(tree, "Inbox").0,
            folder_named(tree, "Sent Items").0,
        )
    });
    engine
        .results
        .lock()
        .unwrap()
        .push_back(Ok(pstoff_engine::Conversion::Complete(Box::new(
            builder_report(&t, inbox, sent),
        ))));
    click(cx, "step-2");
    choose_destination(cx, t.out());
    click(cx, "next");
    click(cx, "start");
    pump_until(&view, cx, |v| v.wizard.step == Step::Report);
    for selector in [
        "evidence-0-0",
        "evidence-0-1",
        "evidence-0-2",
        "folder-part-0-0",
        "skipped-0-0",
        "difference-0-0",
    ] {
        assert!(shown(cx, selector), "missing report row: {selector}");
    }
}

#[gpui::test]
fn browse_button_accepts_keyboard_activation(cx: &mut TestAppContext) {
    let (_view, cx) = open(cx, real());
    cx.run_until_parked();
    cx.simulate_keystrokes("tab enter");
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
}
