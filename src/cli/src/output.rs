//! Structured (JSON) output for lint diagnostics and dry-run change records.
//!
//! The CLI can report its findings either as the human-readable plaintext
//! lines printed to stderr (the default, byte-identical to prior releases) or
//! as a single JSON array on stdout. This module owns the serializable
//! projection of lint findings and dry-run change records and the emit
//! routine, keeping the projection separate from the per-file pipeline and
//! never touching the serde-free `rust-llm-tidy-lint` crate.

use crate::changes::Change;
use rust_llm_tidy_lint::{Diagnostic, Severity};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// A serializable record that is either a lint finding or a dry-run change
/// record, matching the documented JSON schema (`{ path, line, severity, code,
/// message, item_kind, item_name }`). Lint findings use severity `error` or
/// `warning`; change records use `success`. `item_name` is `null` when the
/// item is unnamed.
#[derive(Serialize)]
pub(crate) struct JsonRecord {
    /// Path of the file the record was raised in.
    path: String,
    /// 1-based line number where the item starts.
    line: usize,
    /// Lowercase `error`, `warning`, or `success`.
    severity: &'static str,
    /// Stable rule or operation code, e.g. "DOC001", "FIX", "REORDER", "VIS".
    code: String,
    /// Human-readable description of the finding or would-be edit.
    message: String,
    /// Kind of item that produced the record, e.g. "fn".
    item_kind: String,
    /// Name of the item, or `null` when unnamed.
    item_name: Option<String>,
}

/// Selects the CLI's lint-diagnostic output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum OutputMode {
    /// Human-readable `path:line: sev[CODE]: ...` diagnostics on stderr.
    Text,
    /// A single JSON array of all lint findings and dry-run change records on
    /// stdout.
    Json,
}

/// Emit every collected lint finding and dry-run change record as one JSON
/// array on stdout.
///
/// A run with neither findings nor changes emits `[]`. The document is printed
/// before any error-count or processing-failure bail so downstream consumers
/// receive all records together with the process exit code.
pub(crate) fn emit_json(
    diagnostics: &[(PathBuf, Diagnostic)],
    changes: &[(PathBuf, Change)],
) -> anyhow::Result<()> {
    let mut records: Vec<JsonRecord> = Vec::with_capacity(diagnostics.len() + changes.len());
    records.extend(diagnostics.iter().map(|(path, d)| project_lint(path, d)));
    records.extend(changes.iter().map(|(path, c)| project_change(path, c)));
    // Serialization to a String is infallible for these types; propagate any
    // error defensively rather than silently truncating stdout ownership.
    let doc = serde_json::to_string(&records)?;
    println!("{doc}");
    Ok(())
}

/// Project a single dry-run change record into its serializable form.
fn project_change(path: &Path, c: &Change) -> JsonRecord {
    JsonRecord {
        path: path.display().to_string(),
        line: c.line,
        severity: "success",
        code: c.code.to_string(),
        message: c.message.clone(),
        item_kind: c.kind.clone(),
        item_name: c.name.clone(),
    }
}

/// Project a single lint finding into its serializable form.
fn project_lint(path: &Path, d: &Diagnostic) -> JsonRecord {
    JsonRecord {
        path: path.display().to_string(),
        line: d.line,
        severity: match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        },
        code: d.code.to_string(),
        message: d.message.clone(),
        item_kind: d.item_kind.to_string(),
        item_name: d.item_name.clone(),
    }
}
