//! Structured (JSON) output for lint diagnostics.
//!
//! The CLI can report its lint findings either as the human-readable plaintext
//! lines printed to stderr (the default, byte-identical to prior releases) or
//! as a single JSON array on stdout. This module owns the serializable
//! projection of a lint finding and the emit routine, so the projection stays
//! separate from the per-file pipeline and never touches the serde-free
//! `rust-llm-tidy-lint` crate.

use rust_llm_tidy_lint::{Diagnostic, Severity};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// A serializable projection of a `(path, Diagnostic)` pair matching the
/// documented JSON schema (`{ path, line, severity, code, message, item_kind,
/// item_name }`). `severity` is the lowercase string "error" or "warning" and
/// `item_name` is `null` when the item is unnamed.
#[derive(Serialize)]
pub(crate) struct JsonDiagnostic<'a> {
    /// Path of the file the finding was raised in.
    path: String,
    /// 1-based line number where the item starts.
    line: usize,
    /// Lowercase "error" or "warning".
    severity: &'a str,
    /// Stable rule code, e.g. "DOC001".
    code: &'a str,
    /// Human-readable description of the problem.
    message: &'a str,
    /// Kind of item that produced the finding, e.g. "fn".
    item_kind: &'a str,
    /// Name of the item, or `null` when unnamed.
    item_name: Option<&'a str>,
}

/// Selects the CLI's lint-diagnostic output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum OutputMode {
    /// Human-readable `path:line: sev[CODE]: ...` diagnostics on stderr.
    Text,
    /// A single JSON array of all findings on stdout.
    Json,
}

/// Emit every collected `(path, Diagnostic)` pair as one JSON array on stdout.
///
/// A run with no findings emits `[]`. The document is printed before any
/// error-count or processing-failure bail so downstream consumers receive all
/// findings together with the process exit code.
pub(crate) fn emit_diagnostics(diagnostics: &[(PathBuf, Diagnostic)]) -> anyhow::Result<()> {
    let projected: Vec<JsonDiagnostic<'_>> = diagnostics
        .iter()
        .map(|(path, d)| project(path, d))
        .collect();
    // Serialization to a String is infallible for these types; propagate any
    // error defensively rather than silently truncating stdout ownership.
    let doc = serde_json::to_string(&projected)?;
    println!("{doc}");
    Ok(())
}

/// Project a single finding into its serializable form.
fn project<'a>(path: &Path, d: &'a Diagnostic) -> JsonDiagnostic<'a> {
    JsonDiagnostic {
        path: path.display().to_string(),
        line: d.line,
        severity: match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        },
        code: d.code,
        message: &d.message,
        item_kind: &d.item_kind,
        item_name: d.item_name.as_deref(),
    }
}
