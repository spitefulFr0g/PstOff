//! PstOff's guided wizard: Add OST files → Choose folders → Output → Convert → Report.
//!
//! [`wizard`] holds every decision the screens make and talks to the engine only
//! through `pstoff_engine`'s public `inspect`, `plan` and `convert`. The GPUI views in
//! [`view`] render that state and move engine work off the UI thread.
pub mod format;
pub mod plan;
pub mod report;
pub mod run;
pub mod selection;
pub mod wizard;

#[cfg(windows)]
pub mod view;

pub use wizard::{Engine, RealEngine, SourceId, Status, Step, Wizard};
