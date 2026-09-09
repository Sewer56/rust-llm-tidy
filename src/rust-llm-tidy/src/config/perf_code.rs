//! Select built-in performance policies independently of lint enablement.

use serde::Deserialize;

/// Built-in symbol-rule family selected by `perf_hints`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum PerfCode {
    /// Container capacity reminders for Rust and C#.
    #[serde(rename = "PERF001")]
    Capacity,
    /// Explicit sized C# array initialization reminder.
    #[serde(rename = "PERF002")]
    ArrayInitialization,
}

impl PerfCode {
    /// Families enabled when `perf_hints` is omitted.
    pub(crate) const ALL: &[Self] = &[Self::Capacity, Self::ArrayInitialization];
}
