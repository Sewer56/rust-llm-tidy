//! `rust-llm-tidy` - fix, reorder, narrow visibility, and lint Rust and
//! Markdown source files.
//!
//! A unified CLI for four operations:
//!
//! - **fix**: realign misaligned GFM markdown tables in `.rs` doc comments and
//!   `.md` files (auto-fixable).
//! - **reorder**: reorder Rust source file items into a canonical 10-phase
//!   ordering (the original behavior).
//! - **vis**: narrow bare `pub` items nested inside restricted-visibility
//!   inline modules to the module's visibility (crate-aware by default).
//! - **check**: lint for missing documentation and incomplete `# Errors`
//!   sections (read-only, never writes).
//!
//! Use `all` to run all four in one pass: fix (table alignment) -> reorder
//! (item ordering) -> vis (narrow visibility) -> check (report what remains).
//!
//! # Subcommands
//!
//! | Command   | Mutates?                 | Description                                                                 |
//! | --------- | ------------------------ | --------------------------------------------------------------------------- |
//! | `fix`     | yes (unless `--dry-run`) | Realign GFM markdown tables                                                 |
//! | `reorder` | yes (unless `--dry-run`) | Reorder items into canonical order                                          |
//! | `vis`     | yes (unless `--dry-run`) | Narrow bare `pub` in restricted-visibility modules (crate-aware by default) |
//! | `check`   | no                       | Report documentation and test-naming lint findings                          |
//! | `all`     | yes (unless `--dry-run`) | Fix, reorder, vis, then check                                               |
//!
//! Multiple paths are accepted; each directory is expanded recursively.

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use proc_macro2::fallback::force;
use rust_llm_tidy_fix as fix;
use rust_llm_tidy_lint::check;
use rust_llm_tidy_model::io;
use rust_llm_tidy_model::parse as model_parse;
use rust_llm_tidy_model::parse;
use rust_llm_tidy_model::safety;
use rust_llm_tidy_reorder::graph;
use rust_llm_tidy_reorder::reorder::Permutation;
use rust_llm_tidy_vis::{
    ModuleTree, ReexportSet, build_module_tree, collect_crate_reexports, discover_crate_root,
    narrow_vis_in_tree,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "rust-llm-tidy", about = "Reorder and lint Rust source files")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Crate-aware context for the `vis` step: a prebuilt module tree (per-file
/// floor) + crate-wide re-export set, built ONCE before iterating files.
/// `None` when crate-root discovery fails (standalone file); each file is then
/// narrowed with `floor = None` and a per-file re-export guard.
struct VisContext {
    tree: ModuleTree,
    reexports: ReexportSet,
}

#[derive(Subcommand)]
enum Command {
    /// Reorder items into canonical 10-phase order.
    ///
    /// Mutates files in place unless --dry-run is given.
    Reorder(PathsArgs),
    /// Narrow bare `pub` items nested inside restricted-visibility inline
    /// modules to the module's visibility (crate-aware by default).
    ///
    /// Mutates files in place unless --dry-run is given.
    Vis(PathsArgs),
    /// Check documentation coverage and error sections.
    ///
    /// Read-only: never writes files. Exits non-zero when any error-severity
    /// diagnostic is found.
    Check(PathsArgs),
    /// Fix table alignment, reorder, narrow visibility, then check in one pass.
    ///
    /// Collects `.rs` and `.md` files. Markdown files are fixed (table
    /// alignment); Rust files are fixed, reordered, visibility-narrowed, and
    /// checked. Mutates files unless --dry-run is given.
    All(PathsArgs),
    /// Fix auto-fixable style issues (markdown table alignment).
    ///
    /// Mutates files in place unless --dry-run is given.
    Fix(PathsArgs),
}

/// Shared path and dry-run arguments for every subcommand.
///
/// `--dry-run` is accepted by all subcommands; for `check` it is a no-op
/// (checking is always read-only).
#[derive(Args)]
struct PathsArgs {
    /// Path(s) to the Rust source file(s) or directory(s) to process. Each
    /// directory is expanded recursively. Multiple paths are processed in the
    /// order given; duplicates are kept.
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// For `reorder`/`fix`/`vis`/`all`: print results to stdout instead of
    /// modifying files. For `check`: accepted but ignored (checking is always
    /// read-only).
    #[arg(long)]
    dry_run: bool,
}

fn main() -> anyhow::Result<()> {
    // Span-location support: proc_macro2 needs this for accurate span
    // byte ranges when parsing with syn.
    force();

    let cli = Cli::parse();

    match cli.command {
        Command::Reorder(args) => run_reorder(args),
        Command::Vis(args) => run_vis(args),
        Command::Check(args) => run_check(args),
        Command::All(args) => run_all(args),
        Command::Fix(args) => run_fix(args),
    }
}

// ---------------------------------------------------------------------------
// Subcommand handlers
// ---------------------------------------------------------------------------

/// `all` - fix, reorder, then check in one pass.
///
/// Collects both `.rs` and `.md` files. Markdown files are only fixed (table
/// alignment); reordering and checking apply only to Rust source files.
fn run_all(args: PathsArgs) -> anyhow::Result<()> {
    let paths = resolve_all(&args.paths, &["rs", "md"])?;
    if paths.is_empty() {
        return Ok(());
    }

    let multiple_files = paths.len() > 1;
    let mut error_count = 0usize;
    let mut failed = Vec::new();

    // Build VisContext once for the crate-aware default in the vis step.
    let ctx = resolve_vis_context(&paths);

    for path in &paths {
        // Fix table alignment first (auto-fixable formatting).
        if let Err(e) = fix_file(path, args.dry_run, multiple_files) {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
            continue;
        }

        // Reorder/check are Rust-only operations.
        let is_rust = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "rs");
        if !is_rust {
            continue;
        }

        // Reorder next (fixes ordering). Failures abort the check for this file.
        if let Err(e) = reorder_file(path, args.dry_run, multiple_files) {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
            continue;
        }
        // Narrow visibility next (fixes misleading bare `pub` inside
        // restricted-visibility inline modules). Runs after reorder (canonical
        // item layout) and before check (narrowing can flip VisibilityTier and
        // newly suppress/trigger missing-docs diagnostics).
        if let Err(e) = vis_file(path, args.dry_run, multiple_files, ctx.as_ref()) {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
            continue;
        }
        // Then check (reports remaining doc gaps).
        match check_file(path) {
            Ok(errs) => error_count += errs,
            Err(e) => {
                eprintln!("error processing {}: {e:?}", path.display());
                failed.push(path);
            }
        }
    }

    if !failed.is_empty() {
        bail!("failed to process {} file(s)", failed.len());
    }

    if error_count > 0 {
        bail!("found {} error(s)", error_count);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Per-file operations
// ---------------------------------------------------------------------------

/// `check` - report documentation diagnostics.
fn run_check(args: PathsArgs) -> anyhow::Result<()> {
    let paths = resolve_all(&args.paths, &["rs"])?;
    if paths.is_empty() {
        return Ok(());
    }

    let mut error_count = 0usize;
    let mut failed = Vec::new();

    for path in &paths {
        match check_file(path) {
            Ok(errs) => error_count += errs,
            Err(e) => {
                eprintln!("error processing {}: {e:?}", path.display());
                failed.push(path);
            }
        }
    }

    if !failed.is_empty() {
        bail!("failed to process {} file(s)", failed.len());
    }

    if error_count > 0 {
        bail!("found {} error(s)", error_count);
    }

    Ok(())
}

/// `fix` - realign GFM markdown tables in place.
fn run_fix(args: PathsArgs) -> anyhow::Result<()> {
    let paths = resolve_all(&args.paths, &["rs", "md"])?;
    if paths.is_empty() {
        return Ok(());
    }

    let multiple_files = paths.len() > 1;
    let mut failed = Vec::new();

    for path in &paths {
        if let Err(e) = fix_file(path, args.dry_run, multiple_files) {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
        }
    }

    if !failed.is_empty() {
        bail!("failed to process {} file(s)", failed.len());
    }

    Ok(())
}

/// `reorder` - reorder items into canonical order.
fn run_reorder(args: PathsArgs) -> anyhow::Result<()> {
    let paths = resolve_all(&args.paths, &["rs"])?;
    if paths.is_empty() {
        return Ok(());
    }

    let multiple_files = paths.len() > 1;
    let mut failed = Vec::new();

    for path in &paths {
        if let Err(e) = reorder_file(path, args.dry_run, multiple_files) {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
        }
    }

    if !failed.is_empty() {
        bail!("failed to process {} file(s)", failed.len());
    }

    Ok(())
}

/// `vis` - crate-aware when a crate root is discovered, else standalone.
fn run_vis(args: PathsArgs) -> anyhow::Result<()> {
    let paths = resolve_all(&args.paths, &["rs"])?;
    if paths.is_empty() {
        return Ok(());
    }
    let ctx = resolve_vis_context(&paths);
    let multiple_files = paths.len() > 1;
    let mut failed = Vec::new();
    for path in &paths {
        if let Err(e) = vis_file(path, args.dry_run, multiple_files, ctx.as_ref()) {
            eprintln!("error processing {}: {e:?}", path.display());
            failed.push(path);
        }
    }

    if !failed.is_empty() {
        bail!("failed to process {} file(s)", failed.len());
    }

    Ok(())
}

/// Check a single source file and print any diagnostics to stderr.
///
/// Returns the number of error-severity diagnostics found.
fn check_file(path: &Path) -> anyhow::Result<usize> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    let parsed = model_parse::parse_source(&source)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    let diagnostics = check::run_all(&parsed);

    let error_count = diagnostics
        .iter()
        .filter(|d| matches!(d.severity, rust_llm_tidy_lint::Severity::Error))
        .count();

    if !diagnostics.is_empty() {
        for diag in &diagnostics {
            eprintln!("{}:{}", path.display(), diag);
        }
    }

    Ok(error_count)
}

// ---------------------------------------------------------------------------
// Shared path resolution
// ---------------------------------------------------------------------------

/// Fix table alignment in a single file.
///
/// Reads the source, calls [`fix::fix_tables`], and writes the result back
/// via [`io::atomic_write`] unless `--dry-run` is given. On dry-run with
/// multiple files, a neutral `<!-- {path} -->` HTML-comment header is emitted
/// (valid in both markdown and harmless in stdout).
///
/// `fix` never fails on content; it exits non-zero only on I/O errors.
fn fix_file(path: &Path, dry_run: bool, multiple_files: bool) -> anyhow::Result<()> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let out = fix::fix_tables(&source);
    if dry_run {
        if multiple_files {
            print!("<!-- {} -->\n{}", path.display(), out);
        } else {
            print!("{out}");
        }
    } else if out != source {
        io::atomic_write(path, &out)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

/// Reorder a single source file.
///
/// When processing multiple files in dry-run mode, a comment header with the
/// file path is emitted before each file's output so the results can be
/// distinguished.
fn reorder_file(path: &Path, dry_run: bool, multiple_files: bool) -> anyhow::Result<()> {
    // 1. Read source
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    // 2. Parse - extract items, spans, comments, preamble/trailer
    let parsed = parse::parse_source(&source)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    // 3. Build reference graph and compute topological order
    let order = graph::compute_order(&parsed).context("failed to compute item order")?;

    // 4. Build permutation and emit reordered source
    let permutation =
        Permutation::new(parsed.items.len(), order).context("failed to build permutation")?;
    let output = rust_llm_tidy_reorder::reorder::emit(&parsed, &permutation)
        .context("failed to emit reordered source")?;

    // 5. Safety check - verify every line is preserved (multiset equality)
    safety::verify_line_preservation(&source, &output).with_context(|| {
        format!(
            "safety check failed for {} - reordered output does not preserve lines",
            path.display()
        )
    })?;

    // 6. Write output
    if dry_run {
        if multiple_files {
            print!("// {}\n{}", path.display(), output);
        } else {
            print!("{output}");
        }
    } else {
        io::atomic_write(path, &output)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(())
}

/// Resolve a list of input paths into a flat, ordered list of files with matching extensions.
fn resolve_all(inputs: &[PathBuf], exts: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for input in inputs {
        let resolved = resolve_paths(input, exts)
            .with_context(|| format!("failed to resolve path {}", input.display()))?;
        paths.extend(resolved);
    }
    Ok(paths)
}

/// Build the crate-aware [`VisContext`] from the first input path. Returns
/// `None` (with a printed warning) when crate-root discovery fails, so
/// standalone files keep working via `narrow_vis_in_tree` with `floor = None`
/// and a per-file re-export guard.
fn resolve_vis_context(paths: &[PathBuf]) -> Option<VisContext> {
    let first = paths.first()?;
    match discover_crate_root(first) {
        Ok(root) => {
            // Collect every .rs file under the crate src dir, parse, build tree.
            let crate_dir = root.parent().unwrap_or_else(|| Path::new("."));
            let mut rs_files: Vec<PathBuf> = Vec::new();
            let _ = collect_files(crate_dir, &["rs"], &mut rs_files);
            let mut sources: Vec<(PathBuf, String)> = Vec::new();
            for f in &rs_files {
                if let Ok(src) = fs::read_to_string(f) {
                    // Canonicalize so tree keys match the per-file floor_for lookup
                    // (collect_files yields absolute paths; CLI inputs may be relative).
                    sources.push((fs::canonicalize(f).unwrap_or_else(|_| f.clone()), src));
                }
            }
            let parsed: Vec<syn::File> = sources
                .iter()
                .filter_map(|(_, s)| syn::parse_str(s).ok())
                .collect();
            let tree = match build_module_tree(&root, &sources) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("warning: failed to build module tree ({e:?})");
                    return None;
                }
            };
            for w in tree.warnings() {
                eprintln!("warning: {w}");
            }
            let reexports = collect_crate_reexports(parsed.iter());
            Some(VisContext { tree, reexports })
        }
        Err(e) => {
            eprintln!("warning: crate-aware vis unavailable ({e}); narrowing standalone");
            None
        }
    }
}

/// Narrow visibility in a single source file. With a [`VisContext`] (crate
/// root discovered) the file's tree floor + crate-wide re-export guard apply,
/// but only when the file is a node in the resolved crate module tree; a file
/// outside that tree (e.g. an integration test, example, bench, or a fixture
/// under `tests/`) is narrowed standalone, since the crate-wide re-export set
/// is built only from the crate `src/` dir and would miss the file's own
/// `pub use`. Without a [`VisContext`] (no crate root) every file narrows
/// standalone with `floor = None` and a per-file re-export guard.
fn vis_file(
    path: &Path,
    dry_run: bool,
    multiple_files: bool,
    ctx: Option<&VisContext>,
) -> anyhow::Result<()> {
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
                let parsed: syn::File = syn::parse_str(&source)?;
                let per_file = collect_crate_reexports(std::iter::once(&parsed));
                narrow_vis_in_tree(&source, None, &per_file)
            }
        }
        None => {
            // Standalone: build a per-file re-export guard from this file only.
            let parsed: syn::File = syn::parse_str(&source)?;
            let reexports = collect_crate_reexports(std::iter::once(&parsed));
            narrow_vis_in_tree(&source, None, &reexports)
        }
    }
    .with_context(|| format!("failed to narrow {}", path.display()))?;

    if dry_run {
        if multiple_files {
            print!("// {}\n{}", path.display(), output);
        } else {
            print!("{output}");
        }
    } else if output != source {
        io::atomic_write(path, &output)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(())
}

/// Resolve `path` into a sorted list of files with matching extensions.
///
/// If `path` is a file, it is returned directly. If it is a directory,
/// all files with extensions in `exts` are collected recursively and sorted
/// for deterministic ordering.
fn resolve_paths(path: &Path, exts: &[&str]) -> anyhow::Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }

    if !path.is_dir() {
        bail!("path is neither a file nor a directory: {}", path.display());
    }

    let mut files = Vec::new();
    collect_files(path, exts, &mut files)
        .with_context(|| format!("failed to read directory {}", path.display()))?;
    files.sort();

    Ok(files)
}

/// Recursively collect all files under `dir` whose extension is in `exts`.
fn collect_files(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            collect_files(&path, exts, out)?;
        } else if metadata.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e))
        {
            out.push(path);
        }
    }

    Ok(())
}
