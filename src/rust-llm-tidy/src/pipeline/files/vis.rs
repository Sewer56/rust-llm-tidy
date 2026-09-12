//! Crate-aware visibility narrowing: the `vis` step's context and per-file pass.

use crate::config::CompiledSymbolRule;
use crate::input as paths;
use crate::input::file_io as io;
use crate::pipeline::lint_context;
use crate::reporting::change as changes;
use crate::rules::transform::visibility::rust::{
    ModuleTree, ParsedFile, ReexportSet, build_module_tree, collect_crate_reexports,
    discover_crate_root, narrow_vis_in_tree_protected,
};
use anyhow::Context;
use core::iter;
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
            //   would miss (root key unresolved, `parsed` keys resolved)
            // - the tree ends up with only the root node: no children resolved,
            //   no warnings emitted
            // - every file silently degrades to standalone narrowing
            //
            // Canonicalizing here keeps the BFS root consistent with the
            // canonicalized source paths.
            let root = fs::canonicalize(&root).unwrap_or(root);

            // Collect every .rs file under the crate src dir, parse once, build tree.
            //
            // Each file is parsed into a `ParsedFile` reused by both the
            // module-tree build and the crate-wide re-export scan (single parse
            // per file, vs. the prior double parse).
            let crate_dir = root.parent().unwrap_or_else(|| Path::new("."));
            let mut rs_files: Vec<PathBuf> = Vec::new();
            let _ = paths::collect_files(crate_dir, &["rs"], &mut rs_files, true);
            let mut files: Vec<ParsedFile> = Vec::with_capacity(rs_files.len());
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

/// Narrow visibility in a single source file.
///
/// - With a [`VisContext`] (crate root discovered), the file's tree floor and
///   crate-wide re-export guard apply only to nodes in the resolved crate
///   module tree.
/// - Files outside that tree (e.g. an integration test, example, bench, or a
///   fixture under `tests/`) are narrowed standalone. The crate-wide re-export
///   set is built only from the crate `src/` dir and would miss the file's own
///   `pub use`.
/// - Without a [`VisContext`] (no crate root), every file narrows standalone with
///   `floor = None` and a per-file re-export guard.
///
/// # Errors
/// Returns an error when reading the source, parsing for protection or the
/// standalone guard, narrowing, or writing the result fails.
pub(crate) fn vis_file(
    path: &Path,
    dry_run: bool,
    ctx: Option<&VisContext>,
    disabled: &HashSet<String>,
    rules: &[CompiledSymbolRule],
) -> anyhow::Result<Vec<changes::Change>> {
    if disabled.contains("vis") {
        return Ok(Vec::new());
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let ranges = lint_context::protected_ranges(&source, "rs", rules)?;

    let output = match ctx {
        Some(VisContext { tree, reexports }) => {
            // Resolve the lookup key to match the tree's resolved keys.
            let canon = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if tree.contains(&canon) {
                // Apply the tree floor + crate-wide re-export guard.
                //
                // The file is a node in the resolved crate module tree, and the
                // guard is built from every .rs under the crate src dir, so
                // cross-file re-exports are sound.
                let floor = tree.floor_for(&canon);
                narrow_vis_in_tree_protected(&source, floor, reexports, &ranges)
            } else {
                // Narrow standalone with a per-file re-export guard instead.
                //
                // The file is outside the crate's src module tree
                // (integration test, example, bench, stray file under
                // tests/). The crate-wide re-export set would miss this
                // file's own `pub use`.
                let pf = ParsedFile::new(path.to_path_buf(), source.clone())?;
                let per_file = collect_crate_reexports(iter::once(&pf));
                narrow_vis_in_tree_protected(&source, None, &per_file, &ranges)
            }
        }
        None => {
            // Standalone: build a per-file re-export guard from this file only.
            let pf = ParsedFile::new(path.to_path_buf(), source.clone())?;
            let reexports = collect_crate_reexports(iter::once(&pf));
            narrow_vis_in_tree_protected(&source, None, &reexports, &ranges)
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
