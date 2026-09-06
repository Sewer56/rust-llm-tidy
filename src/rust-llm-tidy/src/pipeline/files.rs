//! File operations and project-aware visibility context.

use crate::input as paths;
use crate::input::file_io as io;
use crate::languages::registry as langs;
use crate::project::csharp as csharp_index;
use crate::reporting::change as changes;
use crate::rules::lint as check;
use crate::rules::transform::visibility::rust::{
    ModuleTree, ParsedFile, ReexportSet, build_module_tree, collect_crate_reexports,
    discover_crate_root, narrow_vis_in_tree,
};
use crate::source::preservation as safety;
use anyhow::Context;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Crate-aware context for the `vis` step: a prebuilt module tree (per-file
/// floor) + crate-wide re-export set, built ONCE before iterating files.
///
/// `None` when crate-root discovery fails (standalone file); each file is then
/// narrowed with `floor = None` and a per-file re-export guard.
pub(crate) struct VisContext {
    tree: ModuleTree,
    reexports: ReexportSet,
}

/// Check a single source file and return its lint diagnostics.
///
/// Returns `(path, diagnostics)` pairs so the caller can either print the
/// plaintext lines to stderr (default output) or project them to JSON.
///
/// The profile decides which passes run: parser-driven checks need a
/// registered backend; text lints source TEXT* per tier.
///
/// - `path`: source file to check
/// - `disabled`: diagnostic codes to suppress
/// - `index`: refreshed C# facts and cached parses for this run
///
/// # Errors
/// Returns an error when reading source or constructing its syntax tree fails.
pub(crate) fn check_file(
    path: &Path,
    disabled: &HashSet<String>,
    index: Option<&csharp_index::CSharpIndex>,
) -> anyhow::Result<Vec<(PathBuf, crate::reporting::Diagnostic)>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let profile = langs::profile_for(ext);

    let mut diagnostics = Vec::new();
    if profile.backend
        && let Some(backend) = crate::languages::backend_for(ext)
    {
        let owned;
        let parsed = if let Some(parsed) = index.and_then(|i| i.parsed(path)) {
            parsed
        } else {
            owned = backend
                .parse(&source)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            &owned
        };
        diagnostics = match index {
            Some(index) => backend.lint_indexed(parsed, &index.index),
            None => backend.lint(parsed),
        };
    }
    match profile.text_lints {
        langs::TextLints::Prose => diagnostics.extend(check::run_text_checks(&source, ext)),
        langs::TextLints::Lexicon => {
            diagnostics.extend(crate::text::comments::text_checks(&source, ext));
        }
        langs::TextLints::Ast | langs::TextLints::None => {}
    }
    diagnostics.retain(|d| !disabled.contains(d.code));

    Ok(diagnostics
        .into_iter()
        .map(|d| (path.to_path_buf(), d))
        .collect())
}

/// Fix table alignment, nested fence delimiters, and repeated inline links in a
/// single file.
///
/// Reads the source and applies the shared buffer transformation pipeline.
///
/// Each pass is gated by the file's [`langs::Profile`] against the active
/// rule selection: an op the profile never allows never runs, and every pass
/// strips and re-applies the profile's comment prefixes.
///
/// Writes the result back via [`io::atomic_write`] unless `--dry-run` is
/// given.
///
/// Every edit reports a [`changes::Change`] in both dry-run and in-place
/// modes:
///
/// - fences via the transformation module's anchors
/// - tables as one per-file record ([`changes::table_changes`])
/// - link hoists as one record per before/after pair
///   ([`changes::link_changes`])
///
/// A no-op pass borrows its text back and yields no record.
///
/// # Errors
/// Returns an error when reading source or atomically writing the result fails.
pub(crate) fn fix_file(
    path: &Path,
    dry_run: bool,
    profile: &langs::Profile,
    enabled: &Option<HashSet<String>>,
    disabled: &HashSet<String>,
    links_min_occurrences: usize,
) -> anyhow::Result<Vec<changes::Change>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let (out, change_records) =
        super::buffer::fix_source(&source, profile, enabled, disabled, links_min_occurrences);
    if !dry_run && out != source {
        io::atomic_write(path, &out)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(change_records)
}

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
pub(crate) fn reorder_file(
    path: &Path,
    dry_run: bool,
    disabled: &HashSet<String>,
) -> anyhow::Result<Vec<changes::Change>> {
    if disabled.contains("reorder") {
        return Ok(Vec::new());
    }
    // 1. Read source
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    // 2. Parse through the registered backend - extract items, spans,
    //    comments, members, preamble/trailer.
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let Some(backend) = crate::languages::backend_for(ext) else {
        return Ok(Vec::new());
    };
    let parsed = backend
        .parse(&source)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    // 3. Compute the item and member order; a declined source is a no-op.
    let Some(permutation) = backend
        .reorder_permutation(&parsed)
        .context("failed to compute item order")?
    else {
        return Ok(Vec::new());
    };

    // 4. Emit the reordered source.
    let output = crate::rules::transform::reorder::emit(&parsed, &permutation)
        .context("failed to emit reordered source")?;

    // 5. Safety check - verify every line is preserved (multiset equality)
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

/// Build the crate-aware [`VisContext`] from the first `.rs` input path.
///
/// Returns `None` (without warning) when there is no `.rs` input or
/// crate-root discovery fails, so standalone files keep working via
/// `narrow_vis_in_tree` with `floor = None` and a per-file re-export guard.
///
/// Vis only ever narrows `.rs` items, so non-Rust inputs (e.g. `.md` docs in
/// `.github/`) must neither select the crate-resolve candidate nor emit a
/// "narrowing standalone" warning.
pub(crate) fn resolve_vis_context(
    paths: &[PathBuf],
    warnings: &mut Vec<String>,
) -> Option<VisContext> {
    let first = paths
        .iter()
        .find(|p| crate::input::ext_in(p.extension().and_then(|e| e.to_str()), &["rs"]))?;
    match discover_crate_root(first) {
        Ok(root) => {
            // Canonicalize the crate root so it matches the canonicalized source
            // paths collected below.
            //
            // `discover_crate_root` returns the owning package's `src_path`
            // from `cargo metadata`, which is not canonicalized. On platforms
            // where the temp dir is behind a symlink (e.g. macOS `/tmp` ->
            // `/private/tmp`, or any symlinked `TMPDIR`):
            //
            // - the BFS root lookup in `build_module_tree` (`parsed.get(&path)`)
            //   would miss (root key is non-canonical, `parsed` keys are canonical)
            // - the tree ends up with only the root node: no children resolved,
            //   no warnings emitted
            // - every file silently degrades to standalone narrowing
            //
            // Canonicalizing here keeps the BFS root consistent with the
            // canonicalized source paths.
            let root = fs::canonicalize(&root).unwrap_or(root);
            // Collect every .rs file under the crate src dir, parse once, build
            // tree. Each file is parsed into a `ParsedFile` reused by both the
            // module-tree build and the crate-wide re-export scan (single parse
            // per file, vs. the prior double parse).
            let crate_dir = root.parent().unwrap_or_else(|| Path::new("."));
            let mut rs_files: Vec<PathBuf> = Vec::new();
            let _ = paths::collect_files(crate_dir, &["rs"], &mut rs_files);
            let mut files: Vec<ParsedFile> = Vec::new();
            for f in &rs_files {
                if let Ok(src) = fs::read_to_string(f) {
                    // Canonicalize so tree keys match the per-file floor_for lookup
                    // (collect_files yields absolute paths; CLI inputs may be relative).
                    let path = fs::canonicalize(f).unwrap_or_else(|_| f.clone());
                    // tree-sitter error-recovers, so a `ParsedFile` is virtually
                    // always produced; a parse failure (no tree) skips the file.
                    match ParsedFile::new(path, src) {
                        Ok(pf) => files.push(pf),
                        Err(e) => warnings.push(format!("could not parse {}: {e}", f.display())),
                    }
                }
            }
            let tree = match build_module_tree(&root, &files) {
                Ok(t) => t,
                Err(e) => {
                    warnings.push(format!("failed to build module tree ({e:?})"));
                    return None;
                }
            };
            for w in tree.warnings() {
                warnings.push(w.to_string());
            }
            let reexports = collect_crate_reexports(&files);
            Some(VisContext { tree, reexports })
        }
        Err(e) => {
            warnings.push(format!(
                "crate-aware vis unavailable ({e}); narrowing standalone"
            ));
            None
        }
    }
}

/// Narrow visibility in a single source file. With a [`VisContext`] (crate
/// root discovered) the file's tree floor + crate-wide re-export guard apply,
/// but only when the file is a node in the resolved crate module tree.
///
/// A file outside that tree (e.g. an integration test, example, bench, or a
/// fixture under `tests/`) is narrowed standalone, since the crate-wide
/// re-export set is built only from the crate `src/` dir and would miss the
/// file's own `pub use`.
///
/// Without a [`VisContext`] (no crate root) every file narrows standalone with
/// `floor = None` and a per-file re-export guard.
pub(crate) fn vis_file(
    path: &Path,
    dry_run: bool,
    ctx: Option<&VisContext>,
    disabled: &HashSet<String>,
) -> anyhow::Result<Vec<changes::Change>> {
    if disabled.contains("vis") {
        return Ok(Vec::new());
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    let output = match ctx {
        Some(VisContext { tree, reexports }) => {
            // Canonicalize the lookup key to match the tree's canonical keys.
            let canon = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if tree.contains(&canon) {
                // File is a node in the resolved crate module tree: apply the
                // tree floor + crate-wide re-export guard (built from every .rs
                // under the crate src dir, so cross-file re-exports are sound).
                let floor = tree.floor_for(&canon);
                narrow_vis_in_tree(&source, floor, reexports)
            } else {
                // File is outside the crate's src module tree (integration test,
                // example, bench, stray file under tests/). The crate-wide
                // re-export set would miss this file's own `pub use`, so narrow
                // standalone with a per-file re-export guard instead.
                let pf = ParsedFile::new(path.to_path_buf(), source.clone())?;
                let per_file = collect_crate_reexports(core::iter::once(&pf));
                narrow_vis_in_tree(&source, None, &per_file)
            }
        }
        None => {
            // Standalone: build a per-file re-export guard from this file only.
            let pf = ParsedFile::new(path.to_path_buf(), source.clone())?;
            let reexports = collect_crate_reexports(core::iter::once(&pf));
            narrow_vis_in_tree(&source, None, &reexports)
        }
    }
    .with_context(|| format!("failed to narrow {}", path.display()))?;

    // Records are reported in both modes; only the write is mode-gated.
    let change_records = changes::vis_changes(&source, &output);
    if !dry_run && output != source {
        io::atomic_write(path, &output)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(change_records)
}
