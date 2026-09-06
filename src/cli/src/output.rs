//! Structured (JSON) output for lint diagnostics and dry-run change records.
//!
//! The CLI can report its findings either as human-readable plaintext lines
//! on stderr (the default) or as a single JSON array on stdout.
//!
//! This module owns the serializable projection of lint findings and dry-run
//! change records and the emit routine, keeping presentation separate from
//! library execution and its structured results.

use core::num::NonZeroU32;
use rust_llm_tidy::reporting::{Change, RunReport};
use rust_llm_tidy::reporting::{Diagnostic, Severity};
use serde::Serialize;
use std::borrow::Cow;
use std::io::Write;
use std::path::Path;

/// A serializable record that is either a lint finding or a dry-run change
/// record, matching the documented JSON schema (`{ path, line, severity, code,
/// message, item_kind, item_name, title }`).
///
/// Lint findings use severity `error`, `warning`, or `hint`; change
/// records use `success`. `item_name` is `null` when the item is unnamed, and
/// `title` is `null` for change records.
///
/// Text fields borrow from the projected record as [`Cow`], so a JSON run
/// allocates nothing per record besides the one `path` string.
#[derive(Serialize)]
pub(crate) struct JsonRecord<'a> {
    /// Path of the file the record was raised in.
    path: Cow<'a, str>,
    /// Optional 1-based line number where the item starts; `null` when the
    /// record has no specific line (e.g. link/table fixes).
    line: Option<NonZeroU32>,
    /// Lowercase `error`, `warning`, `hint`, or `success`.
    severity: &'static str,
    /// Stable rule or operation code, e.g. "DOC001", "FIX", "REORDER", "VIS".
    code: &'static str,
    /// Human-readable description of the finding or would-be edit.
    message: Cow<'a, str>,
    /// Kind of item that produced the record, e.g. "fn".
    item_kind: Cow<'a, str>,
    /// Name of the item, or `null` when unnamed.
    item_name: Option<Cow<'a, str>>,
    /// Friendly title for the finding's rule code, e.g. "missing
    /// documentation" for `DOC001`; `null` for change records.
    title: Option<&'static str>,
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

/// Render processing results before the entry point selects a failure exit code.
pub(crate) fn emit_report(report: &RunReport, json: bool) -> anyhow::Result<()> {
    let mut stderr = std::io::stderr().lock();
    for warning in &report.warnings {
        writeln!(stderr, "warning: {warning}")?;
    }

    if json {
        for file in &report.files {
            if let Some(error) = &file.failure {
                writeln!(stderr, "error processing {}: {error}", file.path.display())?;
            }
        }
        emit_json(report)?;
    } else {
        write_text(&mut stderr, report)?;
    }

    for failure in &report.post_process_failures {
        let action = if failure.spawn_failed {
            "failed to spawn"
        } else {
            "failed"
        };
        writeln!(
            stderr,
            "post_process `{}` {action} on {}: {}",
            failure.command,
            failure.path.display(),
            failure.message
        )?;
    }
    Ok(())
}

/// Emit every collected lint finding and dry-run change record as one JSON
/// array on stdout.
///
/// A run with neither findings nor changes emits `[]`. The document is printed
/// before any error-count or processing-failure bail so downstream consumers
/// receive all records together with the process exit code.
pub(crate) fn emit_json(report: &RunReport) -> anyhow::Result<()> {
    let count = report
        .files
        .iter()
        .map(|file| file.diagnostics.len() + file.changes.len())
        .sum();
    let mut records: Vec<JsonRecord<'_>> = Vec::with_capacity(count);
    records.extend(
        report
            .files
            .iter()
            .flat_map(|file| file.diagnostics.iter().map(|d| project_lint(&file.path, d))),
    );
    records.extend(
        report
            .files
            .iter()
            .flat_map(|file| file.changes.iter().map(|c| project_change(&file.path, c))),
    );
    // Serialization to a String is infallible for these types; propagate any
    // error defensively rather than silently truncating stdout ownership.
    let doc = serde_json::to_string(&records)?;
    // Lock once and write the document plus a trailing newline through the
    // handle so an I/O error is reported instead of silently swallowed.
    let mut out = std::io::stdout().lock();
    out.write_all(doc.as_bytes())?;
    out.write_all(b"\n")?;
    Ok(())
}

/// Project a single dry-run change record into its serializable form.
fn project_change<'a>(path: &Path, c: &'a Change) -> JsonRecord<'a> {
    JsonRecord {
        path: Cow::Owned(path.display().to_string()),
        line: c.line,
        severity: "success",
        code: c.code,
        message: Cow::Borrowed(c.message.as_ref()),
        item_kind: Cow::Borrowed(c.kind.as_str()),
        item_name: c.name.as_deref().map(Cow::Borrowed),
        title: None,
    }
}

/// Project a single lint finding into its serializable form.
fn project_lint<'a>(path: &Path, d: &'a Diagnostic) -> JsonRecord<'a> {
    JsonRecord {
        path: Cow::Owned(path.display().to_string()),
        line: NonZeroU32::new(d.line as u32),
        severity: match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Hint => "hint",
        },
        code: d.code,
        message: Cow::Borrowed(d.message.as_ref()),
        item_kind: Cow::Borrowed(d.item_kind.as_ref()),
        item_name: d.item_name.as_deref().map(Cow::Borrowed),
        title: Some(d.title()),
    }
}

/// Write changes and findings in file order, followed by the separate hint group.
fn write_text(output: &mut impl Write, report: &RunReport) -> std::io::Result<()> {
    for file in &report.files {
        for change in &file.changes {
            writeln!(output, "{}:{change}", file.path.display())?;
        }
        for diagnostic in &file.diagnostics {
            if diagnostic.severity != Severity::Hint {
                writeln!(output, "{}:{diagnostic}", file.path.display())?;
            }
        }
        if let Some(error) = &file.failure {
            writeln!(output, "error processing {}: {error}", file.path.display())?;
        }
    }

    for file in &report.files {
        for diagnostic in &file.diagnostics {
            if diagnostic.severity == Severity::Hint {
                writeln!(output, "{}:{diagnostic}", file.path.display())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::project_lint;
    use rust_llm_tidy::reporting::{Diagnostic, Severity};
    use std::path::Path;

    /// Rendering groups hints last while error counts remain severity-specific.
    #[test]
    fn report_should_render_hints_last_and_count_only_errors() {
        use rust_llm_tidy::reporting::{FileReport, RunReport};

        for (name, severity, errors) in [
            ("hint_only", Severity::Hint, 0),
            ("hint_and_warning", Severity::Warning, 0),
            ("hint_and_error", Severity::Error, 1),
        ] {
            let diagnostic = |severity, line| Diagnostic {
                severity,
                code: "DOC999",
                message: "finding".into(),
                line,
                item_kind: "fn".into(),
                item_name: None,
            };
            let report = RunReport {
                files: vec![
                    FileReport {
                        path: "a.rs".into(),
                        diagnostics: vec![diagnostic(Severity::Hint, 1)],
                        ..FileReport::default()
                    },
                    FileReport {
                        path: "b.rs".into(),
                        diagnostics: vec![diagnostic(severity, 2)],
                        ..FileReport::default()
                    },
                ],
                ..RunReport::default()
            };
            let mut rendered = Vec::new();

            super::write_text(&mut rendered, &report).unwrap();
            let text = String::from_utf8(rendered).unwrap();
            let lines: Vec<_> = text.lines().collect();
            let json: Vec<_> = report
                .files
                .iter()
                .flat_map(|file| {
                    file.diagnostics
                        .iter()
                        .map(|d| serde_json::to_value(project_lint(&file.path, d)).unwrap())
                })
                .collect();

            assert_eq!(report.error_count(), errors, "{name}");
            assert_eq!(report.ensure_success().is_err(), errors > 0, "{name}");
            assert_eq!(lines.len(), 2, "{name}");
            if name == "hint_only" {
                assert!(lines[0].starts_with("a.rs:1: hint["));
                assert!(lines[1].starts_with("b.rs:2: hint["));
            } else {
                assert!(lines[0].starts_with("b.rs:2:"));
                assert!(lines[1].starts_with("a.rs:1: hint["));
            }
            assert_eq!(json.len(), 2);
            assert_eq!(json[0]["severity"], "hint");
        }
    }

    /// A hint finding serializes with `severity: "hint"` and the
    /// unchanged lint-record field set.
    #[test]
    fn project_lint_serializes_hint_severity() {
        let finding = Diagnostic {
            severity: Severity::Hint,
            code: "DOC999",
            message: String::from("consider pre-allocating the buffer"),
            line: 3,
            item_kind: String::from("fn"),
            item_name: Some(String::from("load")),
        };

        let json = serde_json::to_string(&project_lint(Path::new("src/lib.rs"), &finding)).unwrap();

        assert_eq!(
            json,
            "{\"path\":\"src/lib.rs\",\"line\":3,\"severity\":\"hint\",\
             \"code\":\"DOC999\",\"message\":\"consider pre-allocating the buffer\",\
             \"item_kind\":\"fn\",\"item_name\":\"load\",\"title\":\"DOC999\"}"
        );
    }
}
