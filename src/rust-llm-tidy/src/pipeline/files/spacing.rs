//! Insert blank lines between documented members in one source file.

use super::super::changes;
use super::file_io as io;
use crate::config::CompiledSymbolRule;
use crate::languages::backend_for;
use crate::reporting::Change;
use crate::rules::lint as check;
use crate::rules::transform::spacing;
use anyhow::Context;
use std::fs;
use std::path::Path;

/// Space documented members of one Rust or C# file.
///
/// Uses the file's backend to find gaps. A syntax-error tree produces
/// no changes unless `exclude_edits` requires a complete parse.
/// In that case the file fails. Dry-run reports edits without writing.
///
/// # Arguments
///
/// - `path` - Rust or C# file to read and, unless previewing, update.
/// - `dry_run` - If true, report changes without writing.
/// - `rules` - Declaration rules that may protect members from edits.
///
/// # Returns
///
/// One [`Change`] per inserted or proposed blank line.
///
/// # Errors
/// Returns an error if the file cannot be read, parsed or written, or
/// if a damaged tree prevents identifying a protected declaration.
pub(crate) fn spacing_file(
    path: &Path,
    dry_run: bool,
    rules: &[CompiledSymbolRule],
) -> anyhow::Result<Vec<Change>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let Some(backend) = backend_for(ext) else {
        return Ok(Vec::new());
    };
    let parsed = backend
        .parse(&source)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    // Use the same parse for protected ranges and spacing decisions.
    let ranges = check::symbols::excluded_ranges(&parsed, ext, rules)?;
    let (out, edits) = match ext.to_ascii_lowercase().as_str() {
        "rs" => spacing::rust::fix_rust(&source, &parsed, &ranges),
        "cs" => spacing::csharp::fix_csharp(&source, &parsed, &ranges),
        _ => return Ok(Vec::new()),
    };

    let change_records = changes::spacing_changes(&edits);
    if !dry_run && out != source {
        io::atomic_write(path, &out)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(change_records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    /// A gap between documented members gains one blank line and one
    /// change record; a second run reports nothing.
    #[test]
    fn spacing_file_should_insert_one_blank_line_and_stay_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spacing_file.rs");
        fs::write(
            &path,
            indoc! {"
                /// An edge.
                pub struct Edge {
                    /// The source.
                    pub source: u32,
                    /// The target.
                    pub target: u32,
                }
            "},
        )
        .unwrap();

        let first = spacing_file(&path, true, &[]).unwrap();

        assert_eq!(first.len(), 1);
        assert_eq!(
            first[0].message.as_ref(),
            "insert blank line between `source` and `target`"
        );
        // Dry-run leaves the file untouched.
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("pub source: u32,\n    ///")
        );

        let applied = spacing_file(&path, false, &[]).unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            indoc! {"
                /// An edge.
                pub struct Edge {
                    /// The source.
                    pub source: u32,

                    /// The target.
                    pub target: u32,
                }
            "}
        );

        let second = spacing_file(&path, true, &[]).unwrap();
        assert!(second.is_empty(), "already spaced: no further records");
    }

    /// A syntax-error tree is a no-op, not a file failure.
    #[test]
    fn spacing_file_should_noop_for_recovered_trees() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.rs");
        let source = indoc! {"
            struct S {
                /// d a.
                a: u32,
                /// d b.
                b: u32
            "};
        fs::write(&path, source).unwrap();

        let records = spacing_file(&path, false, &[]).unwrap();

        assert!(records.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }
}
