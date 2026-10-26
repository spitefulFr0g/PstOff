//! Wizard state. Screens read it and call its methods; the view layer runs the
//! engine work it hands back (inspections and the conversion queue) off the UI
//! thread and feeds the results in.
use crate::plan::{self, OutputSettings, SourcePlan};
use crate::run::{Job, Outcome, Run};
use crate::selection::{Selection, Totals};
use pstoff_engine::{
    CancelToken, Conversion, ConvertOptions, EngineError, ErrorKind, Inspection, ProgressSink,
    Support, UnsupportedReason,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The engine's public seam. The wizard never reaches past it.
pub trait Engine: Send + Sync + 'static {
    fn inspect(&self, path: &Path) -> Result<Inspection, EngineError>;
    fn plan(
        &self,
        path: &Path,
        options: &ConvertOptions,
    ) -> Result<pstoff_engine::OutputPlan, EngineError> {
        pstoff_engine::plan(path, options)
    }
    fn convert(
        &self,
        path: &Path,
        options: &ConvertOptions,
        progress: &mut dyn ProgressSink,
        cancel: &CancelToken,
    ) -> Result<Conversion, EngineError>;
}

pub struct RealEngine;

impl Engine for RealEngine {
    fn inspect(&self, path: &Path) -> Result<Inspection, EngineError> {
        pstoff_engine::inspect(path)
    }
    fn convert(
        &self,
        path: &Path,
        options: &ConvertOptions,
        progress: &mut dyn ProgressSink,
        cancel: &CancelToken,
    ) -> Result<Conversion, EngineError> {
        pstoff_engine::convert(path, options, progress, cancel)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Add,
    Folders,
    Output,
    Convert,
    Report,
}

impl Step {
    pub const ALL: [Step; 5] = [
        Step::Add,
        Step::Folders,
        Step::Output,
        Step::Convert,
        Step::Report,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Step::Add => "Add OST files",
            Step::Folders => "Choose folders",
            Step::Output => "Output",
            Step::Convert => "Convert",
            Step::Report => "Report",
        }
    }
    pub fn subtitle(self) -> &'static str {
        match self {
            Step::Add => "Pick what to convert",
            Step::Folders => "Untick what you don’t need",
            Step::Output => "Where PSTs go, Split",
            Step::Convert => "Progress and Cancel",
            Step::Report => "What happened",
        }
    }
    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Checking,
    Ready,
    InUse,
    Unsupported(UnsupportedReason),
    /// `inspect` failed for another reason, such as damage or an I/O error.
    Unreadable(String),
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Checking => "Checking…",
            Status::Ready => "Ready",
            Status::InUse => "In use by Outlook",
            Status::Unsupported(_) => "Not supported",
            Status::Unreadable(_) => "Can’t be read",
        }
    }

    pub fn explanation(&self) -> Option<String> {
        match self {
            Status::Checking | Status::Ready => None,
            Status::InUse => {
                Some("Outlook has this file open. Close Outlook, then select Retry.".into())
            }
            Status::Unsupported(reason) => Some(unsupported_text(reason)),
            Status::Unreadable(message) => Some(format!(
                "PstOff couldn’t read this OST, so it won’t be converted. {message}"
            )),
        }
    }
}

pub fn unsupported_text(reason: &UnsupportedReason) -> String {
    let what = match reason {
        UnsupportedReason::InvalidHeader => {
            return "This file doesn’t start with an OST header PstOff recognises, so it won’t be converted.".into()
        }
        UnsupportedReason::Ansi => "This is an older ANSI OST.".to_string(),
        UnsupportedReason::OlderUnicode(v) => format!("This OST uses an older Unicode format (version {v})."),
        UnsupportedReason::UnknownVersion(v) => format!("This OST uses a format version PstOff doesn’t know ({v})."),
        UnsupportedReason::Encoding(e) => format!("This OST is encoded or protected (method {e})."),
    };
    format!("{what} The file isn’t damaged, but this version of PstOff converts only unencoded version 36 OSTs with 4 KB pages, so it won’t be converted.")
}

#[derive(Debug, Clone)]
pub struct Source {
    pub id: SourceId,
    pub path: PathBuf,
    pub file_bytes: Option<u64>,
    pub status: Status,
    pub inspection: Option<Inspection>,
    pub selection: Selection,
    pub base_name: String,
}

impl Source {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn is_ready(&self) -> bool {
        self.status == Status::Ready
    }

    pub fn folders(&self) -> &[pstoff_engine::Folder] {
        self.inspection
            .as_ref()
            .map(|i| i.folders.as_slice())
            .unwrap_or_default()
    }

    pub fn totals(&self) -> Totals {
        self.selection.totals(self.folders())
    }
}

/// An inspection the view must run off the UI thread, then pass to
/// [`Wizard::apply_inspection`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectRequest {
    pub id: SourceId,
    pub path: PathBuf,
}

#[derive(Clone)]
pub struct PlanRequest {
    pub id: SourceId,
    pub path: PathBuf,
    pub options: ConvertOptions,
}

type MeasuredPlan = (
    ConvertOptions,
    Option<Result<pstoff_engine::OutputPlan, EngineError>>,
);

pub struct Wizard {
    engine: Arc<dyn Engine>,
    plans: std::collections::HashMap<SourceId, MeasuredPlan>,
    pub step: Step,
    pub sources: Vec<Source>,
    next_id: u64,
    pill: Option<SourceId>,
    pub output: OutputSettings,
    run: Option<Run>,
    pub confirming_cancel: bool,
    /// A short message about the last Add, such as files that weren't OSTs.
    pub notice: Option<String>,
}

impl Wizard {
    pub fn new(engine: Arc<dyn Engine>) -> Self {
        Self {
            engine,
            plans: Default::default(),
            step: Step::Add,
            sources: Vec::new(),
            next_id: 0,
            pill: None,
            output: OutputSettings::default(),
            run: None,
            confirming_cancel: false,
            notice: None,
        }
    }

    pub fn engine(&self) -> Arc<dyn Engine> {
        self.engine.clone()
    }

    // ---- Add OST files -------------------------------------------------

    /// Queues `.ost` files that aren't queued yet. Each needs an inspection.
    pub fn add_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> Vec<InspectRequest> {
        if self.run.is_some() {
            return Vec::new();
        }
        let mut requests = Vec::new();
        let mut rejected = 0u64;
        let mut duplicates = 0u64;
        for path in paths {
            let is_ost = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ost"));
            if !is_ost {
                rejected += 1;
                continue;
            }
            if self.sources.iter().any(|s| same_path(&s.path, &path)) {
                duplicates += 1;
                continue;
            }
            let id = SourceId(self.next_id);
            self.next_id += 1;
            self.sources.push(Source {
                id,
                file_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
                status: Status::Checking,
                inspection: None,
                selection: Selection::default(),
                base_name: plan::default_base_name(&path),
                path: path.clone(),
            });
            requests.push(InspectRequest { id, path });
        }
        let mut notes = Vec::new();
        if rejected > 0 {
            notes.push(format!(
                "{} left out: only .ost files can be converted.",
                crate::format::plural(rejected, "file", "files")
            ));
        }
        if duplicates > 0 {
            notes.push(format!(
                "{} already in the list.",
                crate::format::plural(duplicates, "file is", "files are")
            ));
        }
        self.notice = (!notes.is_empty()).then(|| notes.join(" "));
        requests
    }

    pub fn apply_inspection(&mut self, id: SourceId, result: Result<Inspection, EngineError>) {
        if !self
            .source(id)
            .is_some_and(|source| source.status == Status::Checking)
        {
            return;
        }
        self.plans.remove(&id);
        let Some(source) = self.source_mut(id) else {
            return;
        };
        match result {
            Ok(inspection) => {
                source.status = match &inspection.support {
                    Support::Supported => Status::Ready,
                    Support::InUse => Status::InUse,
                    Support::Unsupported(reason) => Status::Unsupported(reason.clone()),
                };
                source.selection = Selection::default();
                source.inspection = Some(inspection);
            }
            Err(error) => {
                source.status = match error.kind {
                    ErrorKind::InUse => Status::InUse,
                    ErrorKind::Unsupported(reason) => Status::Unsupported(reason),
                    _ => Status::Unreadable(error.message),
                };
                source.inspection = None;
            }
        }
    }

    /// Inspect an OST that was in use again.
    pub fn retry(&mut self, id: SourceId) -> Option<InspectRequest> {
        if self.run.is_some() {
            return None;
        }
        let source = self.source_mut(id)?;
        if !matches!(source.status, Status::InUse | Status::Unreadable(_)) {
            return None;
        }
        source.status = Status::Checking;
        Some(InspectRequest {
            id,
            path: source.path.clone(),
        })
    }

    pub fn remove(&mut self, id: SourceId) {
        if self.run.is_some() {
            return;
        }
        self.sources.retain(|s| s.id != id);
        self.plans.remove(&id);
        if self.pill == Some(id) {
            self.pill = None;
        }
        if !self.can_enter(self.step) {
            self.step = Step::Add;
        }
    }

    pub fn source(&self, id: SourceId) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    fn source_mut(&mut self, id: SourceId) -> Option<&mut Source> {
        self.sources.iter_mut().find(|s| s.id == id)
    }

    pub fn ready(&self) -> impl Iterator<Item = &Source> {
        self.sources.iter().filter(|s| s.is_ready())
    }

    pub fn any_checking(&self) -> bool {
        self.sources.iter().any(|s| s.status == Status::Checking)
    }

    // ---- Choose folders ------------------------------------------------

    /// The OST whose folder tree is showing.
    pub fn pill(&self) -> Option<&Source> {
        self.pill
            .and_then(|id| self.source(id))
            .filter(|s| s.is_ready())
            .or_else(|| self.ready().next())
    }

    pub fn select_pill(&mut self, id: SourceId) {
        if self.source(id).is_some_and(|s| s.is_ready()) {
            self.pill = Some(id);
        }
    }

    pub fn toggle_folder(&mut self, id: SourceId, folder: &pstoff_engine::FolderRef) {
        if self.run.is_some() {
            return;
        }
        if let Some(source) = self.source_mut(id) {
            let tree = source.inspection.as_ref().map(|i| &i.folders);
            if let Some(tree) = tree {
                source.selection.toggle(tree, folder);
            }
        }
    }

    pub fn toggle_expanded(&mut self, id: SourceId, folder: &pstoff_engine::FolderRef) {
        if let Some(source) = self.source_mut(id) {
            source.selection.toggle_expanded(folder);
        }
    }

    pub fn set_all(&mut self, id: SourceId, ticked: bool) {
        if self.run.is_some() {
            return;
        }
        if let Some(source) = self.source_mut(id) {
            if let Some(inspection) = &source.inspection {
                source.selection.set_all(&inspection.folders, ticked);
            }
        }
    }

    /// Items, size and folders going into the PSTs, across all ready OSTs.
    pub fn grand_totals(&self) -> Totals {
        self.ready().fold(Totals::default(), |mut a, s| {
            let t = s.totals();
            a.items += t.items;
            a.bytes += t.bytes;
            a.folders += t.folders;
            a.all_folders += t.all_folders;
            a.unreadable += t.unreadable;
            a
        })
    }

    // ---- Output --------------------------------------------------------

    pub fn set_destination(&mut self, path: PathBuf) {
        if self.run.is_none() {
            self.output.destination = Some(path);
        }
    }

    pub fn set_split(&mut self, on: bool) {
        if self.run.is_none() {
            self.output.split = on;
        }
    }

    pub fn set_split_gib(&mut self, gib: u64) {
        if self.run.is_none() && plan::SPLIT_CHOICES_GIB.contains(&gib) {
            self.output.split_gib = gib;
        }
    }

    pub fn set_base_name(&mut self, id: SourceId, name: String) {
        if self.run.is_some() {
            return;
        }
        if let Some(source) = self.source_mut(id) {
            source.base_name = name;
        }
    }

    pub fn plan_for(&self, id: SourceId) -> Option<SourcePlan> {
        let source = self.source(id).filter(|s| s.is_ready())?;
        let taken: Vec<String> = self
            .ready()
            .take_while(|s| s.id != id)
            .map(|s| s.base_name.clone())
            .collect();
        Some(plan::plan(
            &self.output,
            &source.base_name,
            self.plans
                .get(&id)
                .filter(|(options, _)| Some(options) == self.convert_options(id).as_ref())
                .and_then(|(_, result)| result.as_ref()),
            &taken,
        ))
    }

    /// View workers measure layouts off the UI thread. Only one request per
    /// source is in flight; changed options schedule a fresh pass when it ends.
    pub fn plan_requests(&mut self) -> Vec<PlanRequest> {
        if self.run.is_some() || self.output.destination.is_none() {
            return vec![];
        }
        let mut requests = vec![];
        let ids: Vec<_> = self.ready().map(|s| s.id).collect();
        for id in ids {
            let options = self.convert_options(id).unwrap();
            let needed = match self.plans.get(&id) {
                Some((_, None)) => false,
                Some((previous, Some(_))) => previous != &options,
                None => true,
            };
            if needed {
                requests.push(PlanRequest {
                    id,
                    path: self.source(id).unwrap().path.clone(),
                    options: options.clone(),
                });
                self.plans.insert(id, (options, None));
            }
        }
        requests
    }

    pub fn apply_plan(
        &mut self,
        request: &PlanRequest,
        result: Result<pstoff_engine::OutputPlan, EngineError>,
    ) {
        if let Some((options, measured)) = self.plans.get_mut(&request.id) {
            if options == &request.options {
                *measured = Some(result);
            }
        }
    }

    /// Everything that stops the Output step from moving on.
    pub fn output_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.output.destination.is_none() {
            problems.push("Choose a folder for the PST files.".into());
        }
        for source in self.ready() {
            if let Some(problem) = self.plan_for(source.id).and_then(|p| p.problem) {
                problems.push(format!("{}: {problem}", source.name()));
            }
        }
        problems
    }

    pub fn convert_options(&self, id: SourceId) -> Option<ConvertOptions> {
        let source = self.source(id).filter(|s| s.is_ready())?;
        self.output.options(
            &source.base_name,
            source.selection.exclusions(source.folders()),
        )
    }

    // ---- Navigation ----------------------------------------------------

    pub fn is_running(&self) -> bool {
        self.run.as_ref().is_some_and(|r| !r.state().finished)
    }

    pub fn has_run(&self) -> bool {
        self.run.is_some()
    }

    pub fn can_enter(&self, step: Step) -> bool {
        if let Some(run) = &self.run {
            let finished = run.state().finished;
            return match step {
                Step::Convert => !finished,
                Step::Report => finished,
                _ => false,
            };
        }
        let ready = self.ready().next().is_some() && !self.any_checking();
        match step {
            Step::Add => true,
            Step::Folders | Step::Output => ready,
            Step::Convert => ready && self.output_problems().is_empty(),
            Step::Report => false,
        }
    }

    pub fn go(&mut self, step: Step) {
        if self.can_enter(step) {
            self.step = step;
        }
    }

    pub fn can_go_next(&self) -> bool {
        self.step < Step::Convert
            && Step::ALL
                .get(self.step.index() + 1)
                .is_some_and(|&n| self.can_enter(n))
    }

    pub fn next(&mut self) {
        if self.can_go_next() {
            self.step = Step::ALL[self.step.index() + 1];
        }
    }

    pub fn can_go_back(&self) -> bool {
        self.run.is_none() && self.step > Step::Add && self.step < Step::Report
    }

    pub fn back(&mut self) {
        if self.can_go_back() {
            self.step = Step::ALL[self.step.index() - 1];
        }
    }

    pub fn start_over(&mut self) {
        if self.is_running() {
            return;
        }
        let engine = self.engine.clone();
        *self = Wizard::new(engine);
    }

    // ---- Convert -------------------------------------------------------

    /// Builds the queue and moves to the running Convert screen. The caller
    /// runs [`Run::execute`] on a worker thread.
    pub fn start(&mut self) -> Option<Run> {
        if self.run.is_some() || self.step != Step::Convert || !self.can_enter(Step::Convert) {
            return None;
        }
        let jobs: Vec<Job> = self
            .ready()
            .filter_map(|s| {
                Some(Job {
                    source: s.id,
                    path: s.path.clone(),
                    name: s.name(),
                    options: self.convert_options(s.id)?,
                    weight: s.totals().bytes,
                    state: crate::run::JobState::Waiting,
                })
            })
            .collect();
        if jobs.is_empty() {
            return None;
        }
        let run = Run::new(jobs);
        self.run = Some(run.clone());
        Some(run)
    }

    pub fn run(&self) -> Option<&Run> {
        self.run.as_ref()
    }

    pub fn request_cancel(&mut self) {
        if self.is_running() && !self.run.as_ref().is_some_and(Run::is_cancelled) {
            self.confirming_cancel = true;
        }
    }

    pub fn keep_converting(&mut self) {
        self.confirming_cancel = false;
    }

    pub fn confirm_cancel(&mut self) {
        self.confirming_cancel = false;
        if let Some(run) = &self.run {
            run.cancel();
        }
    }

    /// Picks up worker progress. Moves to the Report once the queue is done.
    pub fn poll(&mut self) {
        let finished = self.run.as_ref().is_some_and(|r| r.state().finished);
        if finished && self.step == Step::Convert {
            self.confirming_cancel = false;
            self.step = Step::Report;
        }
    }

    pub fn outcomes(&self) -> Vec<(SourceId, Outcome)> {
        self.run
            .as_ref()
            .map(|r| {
                r.state()
                    .jobs
                    .iter()
                    .map(|j| (j.source, j.outcome()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.as_os_str().eq_ignore_ascii_case(b.as_os_str()),
    }
}
