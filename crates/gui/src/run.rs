//! The conversion queue: one `convert` call per OST, in order, on a worker
//! thread. One OST failing doesn't stop the rest. After Cancel, the OST being
//! converted ends as the engine leaves it and the waiting ones are not started.
use crate::wizard::{Engine, SourceId};
use pstoff_engine::{
    BatchResult, CancelToken, Conversion, ConvertOptions, ErrorKind, Progress, ProgressSink,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Clone)]
pub struct Job {
    pub source: SourceId,
    pub path: PathBuf,
    pub name: String,
    pub options: ConvertOptions,
    /// Relative share of the overall percentage (the selected bytes).
    pub weight: u64,
    pub state: JobState,
}

#[derive(Debug, Clone)]
pub enum JobState {
    Waiting,
    Running(Progress),
    Done(BatchResult),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Waiting,
    Running,
    Finished,
    FinishedWithSkipped,
    Cancelled,
    Failed,
    NotStarted,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Outcome::Waiting => "Waiting",
            Outcome::Running => "Converting",
            Outcome::Finished => "Finished",
            Outcome::FinishedWithSkipped => "Finished with skipped items",
            Outcome::Cancelled => "Cancelled, unfinished PST removed",
            Outcome::Failed => "Failed",
            Outcome::NotStarted => "Not started",
        }
    }
}

impl Job {
    pub fn outcome(&self) -> Outcome {
        match &self.state {
            JobState::Waiting => Outcome::Waiting,
            JobState::Running(_) => Outcome::Running,
            JobState::Done(BatchResult::Finished {
                conversion: Conversion::Complete(report),
                ..
            }) if report.skipped.is_empty() => Outcome::Finished,
            JobState::Done(BatchResult::Finished {
                conversion: Conversion::Complete(_),
                ..
            }) => Outcome::FinishedWithSkipped,
            JobState::Done(BatchResult::Finished {
                conversion: Conversion::Cancelled { .. },
                ..
            }) => Outcome::Cancelled,
            JobState::Done(BatchResult::Failed { .. }) => Outcome::Failed,
            JobState::Done(BatchResult::NotStarted { .. }) => Outcome::NotStarted,
        }
    }

    pub fn fraction(&self) -> f64 {
        match &self.state {
            JobState::Waiting => 0.0,
            JobState::Running(p) => p.fraction.clamp(0.0, 1.0),
            JobState::Done(_) => 1.0,
        }
    }
}

#[derive(Debug, Default)]
pub struct RunState {
    pub jobs: Vec<Job>,
    pub finished: bool,
}

impl RunState {
    pub fn overall(&self) -> f64 {
        let total: u64 = self.jobs.iter().map(|j| j.weight.max(1)).sum();
        if total == 0 {
            return 0.0;
        }
        let done: f64 = self
            .jobs
            .iter()
            .map(|j| j.weight.max(1) as f64 * j.fraction())
            .sum();
        done / total as f64
    }

    pub fn current(&self) -> Option<(usize, &Job)> {
        self.jobs
            .iter()
            .enumerate()
            .find(|(_, j)| matches!(j.state, JobState::Running(_)))
    }
}

/// Shared between the worker thread and the UI, which polls it.
#[derive(Debug, Clone, Default)]
pub struct Run {
    shared: Arc<Mutex<RunState>>,
    cancel: CancelToken,
}

impl Run {
    pub fn new(jobs: Vec<Job>) -> Self {
        Self {
            shared: Arc::new(Mutex::new(RunState {
                jobs,
                finished: false,
            })),
            cancel: CancelToken::default(),
        }
    }

    pub fn state(&self) -> MutexGuard<'_, RunState> {
        // A worker panic is caught per job below; the state stays usable.
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Runs every job in order. Call this on a worker thread.
    pub fn execute(&self, engine: &dyn Engine) {
        let count = self.state().jobs.len();
        for index in 0..count {
            let (path, options) = {
                let state = self.state();
                let job = &state.jobs[index];
                (job.path.clone(), job.options.clone())
            };
            let result = if self.cancel.is_cancelled() {
                BatchResult::NotStarted {
                    source: path.clone(),
                }
            } else {
                self.state().jobs[index].state = JobState::Running(Progress {
                    fraction: 0.0,
                    current_folder: None,
                    done: 0,
                    total: 0,
                    part: 1,
                });
                let mut sink = Sink { run: self, index };
                let converted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    engine.convert(&path, &options, &mut sink, &self.cancel)
                }));
                match converted {
                    Ok(Ok(conversion)) => BatchResult::Finished {
                        source: path.clone(),
                        conversion,
                    },
                    Ok(Err(error)) => BatchResult::Failed {
                        source: path.clone(),
                        error,
                    },
                    Err(_) => BatchResult::Failed {
                        source: path.clone(),
                        error: pstoff_engine::EngineError {
                            kind: ErrorKind::Io,
                            message: "The conversion stopped unexpectedly.".into(),
                            completed_files: vec![],
                        },
                    },
                }
            };
            self.state().jobs[index].state = JobState::Done(result);
        }
        self.state().finished = true;
    }
}

struct Sink<'a> {
    run: &'a Run,
    index: usize,
}

impl ProgressSink for Sink<'_> {
    fn update(&mut self, progress: Progress) {
        let mut state = self.run.state();
        if let JobState::Running(previous) = &state.jobs[self.index].state {
            // Never let the bar move backwards.
            let fraction = progress.fraction.max(previous.fraction);
            state.jobs[self.index].state = JobState::Running(Progress {
                fraction,
                ..progress
            });
        }
    }
}
