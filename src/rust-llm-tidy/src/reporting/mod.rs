//! Structured findings and changes returned by processing operations.

pub use change::{Change, ChangeKind};
pub use diagnostic::{Diagnostic, Severity};
pub use file_report::FileReport;
pub use run_report::{PostProcessFailure, RunReport};
pub use source_report::SourceReport;

pub(crate) mod change;
pub mod diagnostic;
mod file_report;
mod run_report;
mod source_report;
