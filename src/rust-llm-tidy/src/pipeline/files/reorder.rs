//! Reorder items in a single source file.

use crate::config::CompiledSymbolRule;
use crate::input::file_io as io;
use crate::languages::backend_for;
use crate::reporting::change as changes;
use crate::rules::lint as check;
use crate::rules::transform::reorder;
use crate::source::preservation as safety;
use anyhow::Context;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Reorder a single source file.
///
/// Returns one per-file [`changes::Change`] record per moved item (derived
/// from the reorder module's `ReorderMove` producer) in both dry-run and
/// in-place modes.
///
/// A type whose member order changed gets one record of its own: member
/// moves carry no top-level `ReorderMove`.
///
/// Parses through the file's registered backend; a parse failure is an
/// error and fails the file. A backend that parses the source but declines
/// to order it (unsupported preprocessor shapes) degrades to a no-op:
/// zero change records, no write.
///
/// Writes the reordered source only when not in dry-run and the output
/// differs from the original.
///
/// # Errors
/// Returns an error when reading, parsing, ordering, or the line-preservation
/// safety check fails, or the result cannot be written.
pub(crate) fn reorder_file(
    path: &Path,
    dry_run: bool,
    disabled: &HashSet<String>,
    rules: &[CompiledSymbolRule],
) -> anyhow::Result<Vec<changes::Change>> {
    if disabled.contains("reorder") {
        return Ok(Vec::new());
    }

    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    // Extract items, spans, comments, members, preamble/trailer.
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let Some(backend) = backend_for(ext) else {
        return Ok(Vec::new());
    };
    let parsed = backend
        .parse(&source)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    // Compute the item and member order; a declined source is a no-op.
    let ranges = check::symbols::excluded_ranges(&parsed, ext, rules)?;
    let Some(mut permutation) = backend
        .reorder_permutation(&parsed)
        .context("failed to compute item order")?
    else {
        return Ok(Vec::new());
    };
    permutation.protect(&parsed, &ranges)?;

    // Emit the reordered source and verify every line is preserved
    // (multiset equality).
    let output = reorder::emit(&parsed, &permutation).context("failed to emit reordered source")?;
    safety::verify_line_preservation(&source, &output).with_context(|| {
        format!(
            "safety check failed for {} - reordered output does not preserve lines",
            path.display()
        )
    })?;

    let change_records = changes::reorder_changes(&parsed, &permutation);
    if !dry_run && output != source {
        io::atomic_write(path, &output)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(change_records)
}
