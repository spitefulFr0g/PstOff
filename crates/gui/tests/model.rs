//! Wizard behaviour without a window, through the real engine on generated OSTs.
mod support;

use pstoff_engine::{
    BatchResult, Conversion, Evidence, OutputFile, Split, UnsupportedReason,
    MAILBOX_COMPLETENESS_STATEMENT,
};
use pstoff_gui::format::GIB;
use pstoff_gui::plan::{self, OutputSettings, DEFAULT_SPLIT_GIB};
use pstoff_gui::run::Outcome;
use pstoff_gui::selection::Tick;
use pstoff_gui::{report, SourceId, Status, Step, Wizard};
use std::path::PathBuf;
use std::sync::Arc;
use support::*;

fn add(w: &mut Wizard, paths: Vec<PathBuf>) -> Vec<SourceId> {
    let requests = w.add_paths(paths);
    let engine = w.engine();
    requests
        .into_iter()
        .map(|r| {
            w.apply_inspection(r.id, engine.inspect(&r.path));
            r.id
        })
        .collect()
}

fn measure(w: &mut Wizard) {
    let engine = w.engine();
    for request in w.plan_requests() {
        let result = engine.plan(&request.path, &request.options);
        w.apply_plan(&request, result);
    }
}

fn to_convert(w: &mut Wizard, destination: PathBuf) {
    w.next();
    w.next();
    w.set_destination(destination);
    measure(w);
    w.next();
    assert_eq!(w.step, Step::Convert, "{:?}", w.output_problems());
}

#[test]
fn add_keeps_only_new_ost_files_and_inspects_each() {
    let t = Temp::new();
    let a = t.write("first.ost", &mailbox());
    let upper = t.write("SECOND.OST", &mailbox());
    let text = t.write("notes.txt", b"not mail");
    let mut w = Wizard::new(real());
    let requests = w.add_paths(vec![a.clone(), a.clone(), text, upper]);
    assert_eq!(requests.len(), 2);
    assert!(w.sources.iter().all(|s| s.status == Status::Checking));
    assert!(!w.can_go_next(), "Next waits for the checks");
    let notice = w.notice.clone().unwrap();
    assert!(notice.contains("only .ost files"), "{notice}");
    assert!(notice.contains("already in the list"), "{notice}");
    for r in requests {
        let result = w.engine().inspect(&r.path);
        w.apply_inspection(r.id, result);
    }
    assert!(w.sources.iter().all(|s| s.status == Status::Ready));
    assert_eq!(w.sources[0].base_name, "first");
    assert_eq!(w.sources[1].base_name, "SECOND");
    assert!(w.can_go_next());
    // Adding the same file again leaves the queue alone.
    assert!(w.add_paths(vec![a]).is_empty());
    assert_eq!(w.sources.len(), 2);
}

#[test]
fn each_status_comes_from_the_engine() {
    let t = Temp::new();
    let ready = t.write("ready.ost", &mailbox());
    let ansi = t.write("ansi.ost", &unsupported(14, 0));
    let older = t.write("older.ost", &unsupported(23, 0));
    let encoded = t.write("encoded.ost", &unsupported(36, 1));
    let damaged = t.write("damaged.ost", &[0; 512]);
    let mut w = Wizard::new(real());
    add(&mut w, vec![ready, ansi, older, encoded, damaged]);
    let statuses: Vec<_> = w.sources.iter().map(|s| s.status.clone()).collect();
    assert_eq!(statuses[0], Status::Ready);
    assert_eq!(statuses[1], Status::Unsupported(UnsupportedReason::Ansi));
    assert_eq!(
        statuses[2],
        Status::Unsupported(UnsupportedReason::OlderUnicode(23))
    );
    assert_eq!(
        statuses[3],
        Status::Unsupported(UnsupportedReason::Encoding(1))
    );
    assert!(
        matches!(statuses[4], Status::Unreadable(_)),
        "{:?}",
        statuses[4]
    );
    for s in &w.sources[1..4] {
        let text = s.status.explanation().unwrap();
        assert!(text.contains("isn’t damaged"), "{text}");
        assert!(text.contains("won’t be converted"), "{text}");
    }
    assert_eq!(w.sources[1].status.label(), "Not supported");
    // Only the ready OST is converted.
    assert_eq!(w.ready().count(), 1);
}

#[cfg(windows)]
#[test]
fn an_ost_held_open_is_in_use_until_retry_after_release() {
    let t = Temp::new();
    let path = t.write("locked.ost", &mailbox());
    let held = hold_exclusively(&path);
    let mut w = Wizard::new(real());
    let id = add(&mut w, vec![path.clone()])[0];
    assert_eq!(w.source(id).unwrap().status, Status::InUse);
    assert_eq!(w.source(id).unwrap().status.label(), "In use by Outlook");
    assert!(!w.can_go_next());
    let again = w.retry(id).unwrap();
    w.apply_inspection(id, w.engine().inspect(&again.path));
    assert_eq!(w.source(id).unwrap().status, Status::InUse);
    drop(held);
    let again = w.retry(id).unwrap();
    w.apply_inspection(id, w.engine().inspect(&again.path));
    assert_eq!(w.source(id).unwrap().status, Status::Ready);
    assert!(w.can_go_next());
}

#[test]
fn removing_an_ost_takes_it_out_of_the_queue() {
    let t = Temp::new();
    let ids = {
        let mut w = Wizard::new(real());
        let ids = add(
            &mut w,
            vec![t.write("a.ost", &mailbox()), t.write("b.ost", &mailbox())],
        );
        w.next();
        assert_eq!(w.step, Step::Folders);
        w.remove(ids[0]);
        assert_eq!(w.sources.len(), 1);
        assert_eq!(w.pill().unwrap().id, ids[1]);
        w.remove(ids[1]);
        assert!(w.sources.is_empty());
        assert_eq!(w.step, Step::Add, "nothing left to choose folders from");
        ids
    };
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn unticking_follows_subfolders_and_updates_the_running_total() {
    let t = Temp::new();
    let mut w = Wizard::new(real());
    let id = add(&mut w, vec![t.write("m.ost", &mailbox())])[0];
    let tree = w.source(id).unwrap().folders().to_vec();
    let inbox = folder_named(&tree, "Inbox");
    let projects = folder_named(&tree, "Projects");
    let subtree = folder_named(&tree, "IPM_SUBTREE");
    let all = w.source(id).unwrap().totals();
    assert_eq!(all.items, 6);
    assert_eq!(all.folders, all.all_folders, "everything starts ticked");

    w.toggle_folder(id, &inbox);
    let s = w.source(id).unwrap();
    assert!(!s.selection.is_ticked(&projects), "subfolders follow");
    let after = s.totals();
    assert_eq!(after.items, 1);
    assert_eq!(after.folders, all.folders - 2);
    assert_eq!(after.bytes, all.bytes - 3 * 1000 - 2 * 500);
    let subtree_folder = pstoff_gui::selection::path_to(&tree, &subtree).unwrap();
    assert_eq!(
        s.selection.tick(subtree_folder.last().unwrap()),
        Tick::Mixed
    );
    assert_eq!(s.selection.exclusions(&tree), vec![inbox.clone()]);

    // Ticking a subfolder brings back the folders it lives in.
    w.toggle_folder(id, &projects);
    let s = w.source(id).unwrap();
    assert!(s.selection.is_ticked(&inbox));
    assert!(s.selection.exclusions(&tree).is_empty());

    w.set_all(id, false);
    let s = w.source(id).unwrap();
    assert_eq!(s.totals().items, 0);
    assert_eq!(s.selection.exclusions(&tree).len(), tree.len());
    w.set_all(id, true);
    assert_eq!(w.source(id).unwrap().totals(), all);
}

#[test]
fn pills_switch_between_queued_osts_with_separate_selections() {
    let t = Temp::new();
    let mut w = Wizard::new(real());
    let ids = add(
        &mut w,
        vec![
            t.write("a.ost", &mailbox()),
            t.write("bad.ost", &unsupported(99, 0)),
            t.write("b.ost", &mailbox()),
        ],
    );
    assert_eq!(w.pill().unwrap().id, ids[0]);
    w.select_pill(ids[1]);
    assert_eq!(
        w.pill().unwrap().id,
        ids[0],
        "unsupported OSTs have no tree"
    );
    w.select_pill(ids[2]);
    assert_eq!(w.pill().unwrap().id, ids[2]);
    let tree = w.source(ids[2]).unwrap().folders().to_vec();
    w.toggle_folder(ids[2], &folder_named(&tree, "Sent Items"));
    assert_eq!(w.source(ids[2]).unwrap().totals().items, 5);
    assert_eq!(w.source(ids[0]).unwrap().totals().items, 6);
    assert_eq!(w.grand_totals().items, 11);
}

#[test]
fn options_reaching_convert_match_the_screens() {
    let t = Temp::new();
    let engine = Arc::new(RecordingEngine::default());
    let mut w = Wizard::new(engine.clone());
    let ids = add(
        &mut w,
        vec![t.write("a.ost", &mailbox()), t.write("b.ost", &mailbox())],
    );
    let tree = w.source(ids[0]).unwrap().folders().to_vec();
    let sent = folder_named(&tree, "Sent Items");
    let inbox = folder_named(&tree, "Inbox");
    w.toggle_folder(ids[0], &sent);
    w.toggle_folder(ids[0], &inbox);
    w.set_base_name(ids[1], "renamed".into());
    w.set_split(true);
    w.set_split_gib(10);
    to_convert(&mut w, t.out());
    let run = w.start().unwrap();
    run.execute(engine.as_ref());
    let calls = engine.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    let (path, first) = &calls[0];
    assert!(path.ends_with("a.ost"));
    assert_eq!(first.destination, t.out());
    assert_eq!(first.base_name, "a");
    let mut excluded = first.exclusions.clone();
    excluded.sort_by_key(|f| f.0);
    let mut expected = vec![sent, inbox];
    expected.sort_by_key(|f| f.0);
    assert_eq!(excluded, expected);
    assert_eq!(first.split, Split::Bytes(10 * GIB));
    assert_eq!(calls[1].1.base_name, "renamed");
    assert!(calls[1].1.exclusions.is_empty());
}

#[test]
fn split_off_sends_no_limit_and_default_choice_is_the_engine_default() {
    assert_eq!(DEFAULT_SPLIT_GIB * GIB, pstoff_engine::DEFAULT_SPLIT_BYTES);
    let settings = OutputSettings::default();
    assert!(!settings.split);
    assert_eq!(settings.split_limit(), Split::Off);
    assert_eq!(settings.split_gib, 20);
}

#[test]
fn preview_uses_engine_sizes_and_warning_without_repacking_items() {
    let t = Temp::new();
    let settings = OutputSettings {
        destination: Some(t.root.clone()),
        ..Default::default()
    };
    let measured = Ok(pstoff_engine::OutputPlan::new(vec![
        pstoff_engine::PlannedFile {
            path: t.root.join("mailbox.pst"),
            estimated_bytes: 51 * GIB,
            part: 1,
            items: 1,
        },
    ]));
    let preview = plan::plan(&settings, "mailbox", Some(&measured), &[]);
    assert_eq!(preview.files.len(), 1);
    assert_eq!(preview.files[0].name, "mailbox.pst");
    assert_eq!(preview.estimated_bytes, 51 * GIB);
    assert!(preview.over_outlook_limit);
    assert!(plan::over_limit_text(preview.estimated_bytes).contains("50 GB"));
}

#[test]
fn existing_files_and_repeated_names_stop_the_output_step() {
    let t = Temp::new();
    let mut w = Wizard::new(real());
    let ids = add(
        &mut w,
        vec![t.write("a.ost", &mailbox()), t.write("b.ost", &mailbox())],
    );
    w.next();
    w.next();
    assert!(!w.can_go_next(), "no destination yet");
    std::fs::create_dir_all(t.out()).unwrap();
    std::fs::write(t.out().join("a.pst"), b"earlier output").unwrap();
    w.set_destination(t.out());
    measure(&mut w);
    let problems = w.output_problems();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("never overwrites"), "{problems:?}");
    assert!(w.plan_for(ids[0]).unwrap().files[0].exists);
    assert!(!w.can_go_next());

    w.set_base_name(ids[0], "B".into());
    measure(&mut w);
    let problems = w.output_problems();
    assert!(
        problems[0].contains("already uses the name"),
        "{problems:?}"
    );
    w.set_base_name(ids[0], "a:b".into());
    assert!(w.output_problems()[0].contains("can’t contain"));
    w.set_base_name(ids[0], "  ".into());
    assert!(w.output_problems()[0].contains("Enter a name"));
    w.set_base_name(ids[0], "fresh".into());
    measure(&mut w);
    assert!(w.output_problems().is_empty());
    assert!(w.can_go_next());
    assert_eq!(
        std::fs::read(t.out().join("a.pst")).unwrap(),
        b"earlier output"
    );
}

#[test]
fn one_failure_does_not_stop_the_queue() {
    let t = Temp::new();
    let a = t.write("a.ost", &mailbox());
    let b = t.write("b.ost", &mailbox());
    let source_bytes = std::fs::read(&b).unwrap();
    let mut w = Wizard::new(real());
    add(&mut w, vec![a, b.clone()]);
    to_convert(&mut w, t.out());
    let run = w.start().unwrap();
    // The first OST's name is taken after the check, so its conversion fails.
    std::fs::create_dir_all(t.out()).unwrap();
    std::fs::write(t.out().join("a.pst"), b"someone else's").unwrap();
    run.execute(w.engine().as_ref());
    w.poll();
    assert_eq!(w.step, Step::Report);
    let outcomes: Vec<_> = w.outcomes().into_iter().map(|(_, o)| o).collect();
    assert_eq!(
        outcomes,
        [Outcome::Failed, Outcome::Finished],
        "{:?}",
        report::build(&w)
            .iter()
            .map(|r| r.message.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        std::fs::read(t.out().join("a.pst")).unwrap(),
        b"someone else's"
    );
    assert!(t.out().join("b.pst").exists());
    assert_eq!(
        std::fs::read(&b).unwrap(),
        source_bytes,
        "the OST is only read"
    );

    let reports = report::build(&w);
    assert!(reports[0]
        .message
        .as_deref()
        .unwrap()
        .contains("Nothing was overwritten"));
    assert_eq!(reports[1].files.len(), 1);
    assert_eq!(reports[1].items, 6);
    assert!(reports[1].saved_report.is_some());
}

#[test]
fn cancel_asks_first_then_keeps_finished_psts_and_marks_the_rest_not_started() {
    let t = Temp::new();
    let gate = Gate::default();
    let engine = Arc::new(GatedEngine::new(gate.clone(), 1));
    let mut w = Wizard::new(engine.clone());
    add(
        &mut w,
        vec![
            t.write("a.ost", &mailbox()),
            t.write("b.ost", &mailbox()),
            t.write("c.ost", &mailbox()),
        ],
    );
    to_convert(&mut w, t.out());
    let run = w.start().unwrap();
    let worker = {
        let run = run.clone();
        let engine = engine.clone();
        std::thread::spawn(move || run.execute(engine.as_ref()))
    };
    gate.wait_until_held();
    assert!(w.is_running());
    assert!(!w.can_go_back(), "no going back while converting");

    w.request_cancel();
    assert!(w.confirming_cancel);
    w.keep_converting();
    assert!(!w.confirming_cancel);
    assert!(!run.is_cancelled(), "keeping on doesn't cancel");

    w.request_cancel();
    w.confirm_cancel();
    assert!(run.is_cancelled());
    gate.release();
    worker.join().unwrap();
    w.poll();
    assert_eq!(w.step, Step::Report);
    let outcomes: Vec<_> = w.outcomes().into_iter().map(|(_, o)| o).collect();
    assert_eq!(
        outcomes,
        [Outcome::Finished, Outcome::Cancelled, Outcome::NotStarted]
    );
    assert_eq!(
        t.outputs(),
        ["a.json", "a.pst", "a.txt"],
        "no partial PST is left"
    );
    let reports = report::build(&w);
    assert!(reports[1]
        .message
        .as_deref()
        .unwrap()
        .contains("unfinished PST was removed"));
    assert!(reports[2]
        .message
        .as_deref()
        .unwrap()
        .contains("waiting in the queue"));
}

#[test]
fn report_from_a_real_conversion_opens_with_the_engines_completeness_note() {
    let t = Temp::new();
    let mut w = Wizard::new(real());
    add(&mut w, vec![t.write("m.ost", &mailbox())]);
    assert_eq!(
        report::completeness_statement(&w),
        MAILBOX_COMPLETENESS_STATEMENT,
        "shown before converting"
    );
    to_convert(&mut w, t.out());
    let run = w.start().unwrap();
    run.execute(w.engine().as_ref());
    w.poll();
    let r = &report::build(&w)[0];
    assert_eq!(r.outcome, Outcome::Finished, "{:?}", r.message);
    assert!(report::completeness_statement(&w).starts_with("Mailbox completeness: unknown"));
    assert_eq!(r.items, 6);
    assert_eq!(r.files[0].path, t.out().join("m.pst"));
    assert_eq!(
        r.saved_report.as_deref(),
        Some(t.out().join("m.txt").as_path())
    );
    assert!(r.folder_parts.is_empty(), "one part needs no mapping");
    let labels: Vec<_> = r.evidence.iter().map(|e| e.label).collect();
    assert_eq!(
        labels,
        [
            "Headers-only items",
            "Unreadable content",
            "External attachment references"
        ]
    );
}

#[test]
fn report_rows_cover_split_mapping_skipped_items_evidence_and_differences() {
    let t = Temp::new();
    let engine = Arc::new(ScriptedEngine::default());
    let mut w = Wizard::new(engine.clone());
    let id = add(&mut w, vec![t.write("m.ost", &mailbox())])[0];
    let tree = w.source(id).unwrap().folders().to_vec();
    let inbox = folder_named(&tree, "Inbox").0;
    let sent = folder_named(&tree, "Sent Items").0;
    engine
        .results
        .lock()
        .unwrap()
        .push_back(Ok(Conversion::Complete(Box::new(builder_report(
            &t, inbox, sent,
        )))));
    to_convert(&mut w, t.out());
    w.start().unwrap().execute(engine.as_ref());
    w.poll();
    let r = &report::build(&w)[0];
    assert_eq!(r.outcome, Outcome::FinishedWithSkipped);
    assert_eq!(r.items, 4);
    assert_eq!(r.folders, 2);
    assert_eq!(r.files.iter().map(|f| f.part).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(r.folder_parts.len(), 3);
    assert!(r.folder_parts[0].folder.ends_with("IPM_SUBTREE / Inbox"));
    assert_eq!((r.folder_parts[1].part, r.folder_parts[1].items), (2, 1));
    assert_eq!(r.skipped.len(), 2);
    assert!(r.skipped[0].0.starts_with("Item 0x200024 in "));
    assert!(r.skipped[1].0.starts_with("Folder "));
    assert_eq!(r.skipped[1].1, "The folder table could not be read.");
    assert_eq!(r.evidence[0].value, "2 items");
    assert_eq!(r.evidence[0].references.len(), 2);
    assert!(r.evidence[0].references[1].ends_with("Sent Items"));
    assert_eq!(r.evidence[1].value, "None found");
    assert_eq!(r.evidence[2].value, "Not checked");
    assert!(r.evidence[2].note.contains("aren’t missing content"));
    assert_eq!(r.differences[0].property, "0x3001001f");
    assert_eq!(r.differences[0].item, "Store");
    assert!(r.differences[1].item.starts_with("Item 0x200044 in "));
    assert_eq!(r.differences[1].source, "contents table cache");
}

#[test]
fn failed_conversion_keeps_the_engines_message_and_finished_parts() {
    let t = Temp::new();
    let engine = Arc::new(ScriptedEngine::default());
    let mut w = Wizard::new(engine.clone());
    let id = add(&mut w, vec![t.write("m.ost", &mailbox())])[0];
    let inbox = folder_named(w.source(id).unwrap().folders(), "Inbox").0;
    engine
        .results
        .lock()
        .unwrap()
        .push_back(Err(pstoff_engine::EngineError {
            kind: pstoff_engine::ErrorKind::OversizedItem(item(inbox, 0x2000a4)),
            message: "An Item is larger than the Split limit.".into(),
            completed_files: vec![OutputFile {
                path: t.out().join("m-part001.pst"),
                bytes: 10,
                part: 1,
            }],
        }));
    to_convert(&mut w, t.out());
    w.start().unwrap().execute(engine.as_ref());
    w.poll();
    let r = &report::build(&w)[0];
    assert_eq!(r.outcome, Outcome::Failed);
    let message = r.message.as_deref().unwrap();
    assert!(message.starts_with("An Item is larger than the Split limit."));
    assert!(message.contains("Item 0x2000a4 in "), "{message}");
    assert_eq!(r.files.len(), 1);
    assert!(r.evidence.is_empty());
    assert!(matches!(
        w.run().unwrap().state().jobs[0].state,
        pstoff_gui::run::JobState::Done(BatchResult::Failed { .. })
    ));
}

#[test]
fn unreadable_folders_are_marked_before_converting() {
    let t = Temp::new();
    let mut w = Wizard::new(real());
    let id = add(&mut w, vec![t.write("d.ost", &damaged_folder())])[0];
    let s = w.source(id).unwrap();
    assert_eq!(s.status, Status::Ready);
    assert_eq!(s.totals().unreadable, 1);
}

#[test]
fn navigation_follows_the_rail() {
    let t = Temp::new();
    let mut w = Wizard::new(real());
    assert!(!w.can_enter(Step::Folders));
    assert!(!w.can_go_back());
    add(&mut w, vec![t.write("m.ost", &mailbox())]);
    w.go(Step::Convert);
    assert_eq!(w.step, Step::Add, "Convert needs a destination first");
    w.go(Step::Output);
    assert_eq!(w.step, Step::Output);
    w.back();
    assert_eq!(w.step, Step::Folders);
    w.set_destination(t.out());
    measure(&mut w);
    w.go(Step::Convert);
    assert_eq!(w.step, Step::Convert);
    assert!(!w.can_go_next(), "Convert starts with its own button");
    assert!(!w.can_enter(Step::Report));
    let run = w.start().unwrap();
    assert!(
        !w.can_enter(Step::Add),
        "the rail is locked while converting"
    );
    run.execute(w.engine().as_ref());
    w.poll();
    assert_eq!(w.step, Step::Report);
    assert!(!w.can_enter(Step::Convert));
    w.start_over();
    assert_eq!(w.step, Step::Add);
    assert!(w.sources.is_empty());
}

#[test]
fn split_preview_numbers_even_a_single_part_and_detects_its_collision() {
    let t = Temp::new();
    let settings = OutputSettings {
        destination: Some(t.root.clone()),
        split: true,
        ..Default::default()
    };
    std::fs::write(t.root.join("mailbox-part001.pst"), b"keep").unwrap();
    let measured = Ok(pstoff_engine::OutputPlan::new(vec![
        pstoff_engine::PlannedFile {
            path: t.root.join("mailbox-part001.pst"),
            estimated_bytes: 1,
            part: 1,
            items: 1,
        },
    ]));
    let preview = plan::plan(&settings, "mailbox", Some(&measured), &[]);
    assert_eq!(preview.files[0].name, "mailbox-part001.pst");
    assert!(preview.files[0].exists);
    assert!(preview.problem.is_some());
}

#[test]
fn preview_waits_for_current_options_and_matches_real_conversion() {
    let t = Temp::new();
    let mut w = Wizard::new(real());
    let id = add(&mut w, vec![t.write("m.ost", &mailbox())])[0];
    w.set_destination(t.out());
    let stale = w.plan_requests().pop().unwrap();
    assert!(w
        .plan_for(id)
        .unwrap()
        .problem
        .unwrap()
        .contains("Planning"));
    let inbox = folder_named(w.source(id).unwrap().folders(), "Inbox");
    w.toggle_folder(id, &inbox);
    w.set_split(true);
    assert!(w.plan_requests().is_empty(), "one worker per source");
    let engine = w.engine();
    w.apply_plan(&stale, engine.plan(&stale.path, &stale.options));
    assert!(
        w.plan_for(id).unwrap().files.is_empty(),
        "stale boundaries cannot be shown"
    );
    measure(&mut w);
    let preview = w.plan_for(id).unwrap();
    assert!(preview.problem.is_none());
    assert!(!t.out().exists());
    w.go(Step::Convert);
    w.start().unwrap().execute(engine.as_ref());
    w.poll();
    let reports = report::build(&w);
    assert_eq!(reports[0].excluded_folders.len(), 1);
    assert!(reports[0].excluded_folders[0].ends_with(" / Inbox"));
    assert_eq!(preview.files.len(), reports[0].files.len());
    for (planned, actual) in preview.files.iter().zip(&reports[0].files) {
        assert_eq!(
            planned.name,
            actual.path.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(
            planned.estimated_bytes,
            std::fs::metadata(&actual.path).unwrap().len()
        );
    }
}

#[test]
fn incomplete_evidence_retains_findings_and_names_unchecked_items() {
    let t = Temp::new();
    let engine = Arc::new(ScriptedEngine::default());
    let mut w = Wizard::new(engine.clone());
    let id = add(&mut w, vec![t.write("m.ost", &mailbox())])[0];
    let tree = w.source(id).unwrap().folders();
    let inbox = folder_named(tree, "Inbox").0;
    let sent = folder_named(tree, "Sent Items").0;
    let mut report = builder_report(&t, inbox, sent);
    report.evidence.headers_only = Evidence::Incomplete {
        count: 1,
        references: vec![item(inbox, 0x200024)],
        unchecked: vec![item(sent, 0x200044)],
    };
    engine
        .results
        .lock()
        .unwrap()
        .push_back(Ok(Conversion::Complete(Box::new(report))));
    to_convert(&mut w, t.out());
    w.start().unwrap().execute(engine.as_ref());
    w.poll();
    let reports = report::build(&w);
    let evidence = &reports[0].evidence[0];
    assert_eq!(evidence.value, "1 found; 1 not checked");
    assert!(evidence.references[0].contains("0x200024"));
    assert!(evidence.references[1].starts_with("Not checked: Item 0x200044"));
}

#[test]
fn report_uses_saved_item_paths_and_inspection_explanation() {
    let reference = pstoff_engine::ItemRef {
        folder: pstoff_engine::FolderRef(0x182),
        node: 0x200024,
        folder_path: "Root / Recovered".into(),
    };
    assert_eq!(
        report::item_label(&[], &reference),
        "Item 0x200024 in Root / Recovered"
    );
    let t = Temp::new();
    let mut w = Wizard::new(real());
    add(&mut w, vec![t.write("m.ost", &mailbox())]);
    w.sources[0]
        .inspection
        .as_mut()
        .unwrap()
        .mailbox_completeness_statement = "Explanation returned by Inspection".into();
    assert_eq!(
        report::completeness_statement(&w),
        "Explanation returned by Inspection"
    );
}
