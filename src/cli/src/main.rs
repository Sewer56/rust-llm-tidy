//! `rust-llm-tidy` - fix, reorder, narrow visibility, and lint admitted
//! source files.
//!
//! A single default command that runs the full pipeline (fix -> reorder -> vis
//! -> lints) on every admitted source file.
//!
//! Admitted by default: `.rs`, the markdown family (`.md`, `.markdown`,
//! `.txt`, `.text`, `.mdx`), and the code languages the default registry
//! admits for table fixes.
//!
//! When no paths are given, the changed files from the current git diff are
//! used, filtered to the same admitted extensions.
//!
//! # Pipeline
//!
//! | Op      | Does                                               | Mutates |
//! | ------- | -------------------------------------------------- | ------- |
//! | fix     | align tables, fix fences, hoist links              | yes     |
//! | reorder | review-friendly item order                         | yes     |
//! | vis     | narrow bare `pub` in restricted-visibility modules | yes     |
//! | lints   | DOC001-DOC006 + TEST001 checks                     | no      |
//!
//! # Flags
//!
//! | Flag                | Effect                                              |
//! | ------------------- | --------------------------------------------------- |
//! | `--validate`        | Validate config and exit (no files touched)         |
//! | `--include <RULE>`  | Run only these rules (repeatable, overrides config) |
//! | `--exclude <RULE>`  | Skip these rules (repeatable, additive)             |
//! | `--extension <EXT>` | Admit an extra file extension (repeatable)          |
//! | `--dry-run`         | Preview the changes without writing them            |
//! | `--config <PATH>`   | Explicit config path                                |
//! | `--no-config`       | Disable config discovery                            |
//! | `--output-mode <M>` | Lint output format: `text` (default) or `json`      |
//! | `--json`            | Alias for `--output-mode json`                      |
//!
//! See `docs/` for per-feature documentation with before/after examples.

use anyhow::{Context, bail};
use clap::Parser;
use config::CompiledConfig;
use rust_llm_tidy_fix as fix;
use rust_llm_tidy_lint::check;
use rust_llm_tidy_model::io;
use rust_llm_tidy_model::parse as model_parse;
use rust_llm_tidy_model::parse;
use rust_llm_tidy_model::safety;
use rust_llm_tidy_reorder::graph;
use rust_llm_tidy_reorder::reorder::Permutation;
use rust_llm_tidy_vis::{
    ModuleTree, ParsedFile, ReexportSet, build_module_tree, collect_crate_reexports,
    discover_crate_root, narrow_vis_in_tree,
};
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

mod changes;
mod config;
mod diff;
// Language admission registry: the single authority behind every extension
// admission and per-file op gate.
mod langs;
mod output;
mod paths;
mod pipeline;

/// Command-line arguments for `rust-llm-tidy`, parsed via `clap`.
///
/// Collects the input paths plus flags controlling dry-run, validation, rule
/// selection, extension admission, and config discovery. See the `# Flags`
/// table in the crate docs.
#[derive(Parser)]
#[command(
    name = "rust-llm-tidy",
    about = "Fix, reorder, narrow visibility, and lint admitted source files"
)]
pub(crate) struct Cli {
    /// Path(s) to the Rust source file(s) or directory(s) to process. Each
    /// directory is expanded recursively. When omitted, the changed files in
    /// the current git diff are used, filtered to the admitted extensions.
    paths: Vec<PathBuf>,
    /// Print the changes that would be made instead of modifying files.
    #[arg(long)]
    dry_run: bool,
    /// Validate the config and exit; do not process files.
    #[arg(long)]
    validate: bool,
    /// Run only these rules/lint-codes (repeatable). Overrides config `include`.
    #[arg(long, value_name = "RULE")]
    include: Vec<String>,
    /// Skip these rules/lint-codes (repeatable). Additive to config `exclude`.
    #[arg(long, value_name = "RULE")]
    exclude: Vec<String>,
    /// Admit files with this extension in addition to the defaults
    /// (repeatable). Written without the leading dot and matched
    /// case-insensitively; additive only - default extensions cannot be
    /// removed.
    #[arg(long, value_name = "EXT")]
    extension: Vec<String>,
    /// Path to a `.rust-llm-tidy.yml` config file. Overrides auto-discovery.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Disable config discovery and loading entirely.
    #[arg(long, global = true, conflicts_with = "config")]
    no_config: bool,
    /// Lint output format: `text` (default) prints plaintext diagnostics to
    /// stderr; `json` prints a single JSON array of lint findings and dry-run
    /// change records to stdout.
    #[arg(long, value_name = "MODE", default_value = "text")]
    output_mode: output::OutputMode,
    /// Alias for `--output-mode json`.
    #[arg(long, conflicts_with = "output_mode")]
    json: bool,
}

/// Crate-aware context for the `vis` step: a prebuilt module tree (per-file
/// floor) + crate-wide re-export set, built ONCE before iterating files.
///
/// `None` when crate-root discovery fails (standalone file); each file is then
/// narrowed with `floor = None` and a per-file re-export guard.
pub(crate) struct VisContext {
    tree: ModuleTree,
    reexports: ReexportSet,
}

impl Cli {
    /// Whether the run should emit lint findings as JSON on stdout. Both
    /// `--output-mode json` and the `--json` alias select JSON mode.
    fn json_mode(&self) -> bool {
        self.output_mode == output::OutputMode::Json || self.json
    }
}

// ---------------------------------------------------------------------------
// Per-file operations
// ---------------------------------------------------------------------------

/// Check a single source file and return its lint diagnostics.
///
/// Returns `(path, diagnostics)` pairs so the caller can either print the
/// plaintext lines to stderr (default output) or project them to JSON.
///
/// The profile decides which passes run: parser-driven checks need a
/// registered backend (Rust today), and the parser-free text checks run for
/// the markdown family and Rust.
pub(crate) fn check_file(
    path: &Path,
    disabled: &HashSet<String>,
) -> anyhow::Result<Vec<(PathBuf, rust_llm_tidy_lint::Diagnostic)>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let profile = langs::profile_for(ext);

    let mut diagnostics = Vec::new();
    if profile.backend {
        let parsed = model_parse::parse_source(&source)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        diagnostics = check::run_all(&parsed);
    }
    if profile.runs_text_lints() {
        diagnostics.extend(check::run_text_checks(&source, ext));
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
/// Reads the source, runs [`fix::fix_tables_for`], [`fix::fix_fences_for`],
/// then [`fix::fix_links`] (or [`fix::fix_links_with_min`] when
/// `links_min_occurrences` exceeds 1).
///
/// Each pass is gated by the file's [`langs::Profile`] against the active
/// rule selection: an op the profile never admits never runs, and the table
/// and fence passes strip and re-apply the profile's comment prefixes.
///
/// Writes the result back via [`io::atomic_write`] unless `--dry-run` is
/// given.
///
/// Every edit reports a [`changes::Change`] in both dry-run and in-place
/// modes:
///
/// - fences via the fix crate's anchors
/// - tables as one per-file record ([`changes::table_changes`])
/// - link hoists as one record per before/after pair
///   ([`changes::link_changes`])
///
/// A no-op pass borrows its text back and yields no record.
///
/// `fix` never fails on content; it exits non-zero only on I/O errors.
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
    let mut out: String = source.clone();
    let mut change_records = Vec::new();
    if profile.op_enabled("tables", enabled, disabled) {
        let prior = std::mem::take(&mut out);
        match fix::fix_tables_for(&prior, profile.prefixes) {
            Cow::Owned(after) => {
                change_records.push(changes::table_changes());
                out = after;
            }
            Cow::Borrowed(_) => out = prior,
        }
    }
    if profile.op_enabled("fences", enabled, disabled) {
        let prior = std::mem::take(&mut out);
        let outcome = fix::fix_fences_for(&prior, profile.prefixes);
        match outcome.text {
            Cow::Owned(after) => {
                change_records.extend(changes::fence_changes(&outcome.anchors));
                out = after;
            }
            Cow::Borrowed(_) => out = prior,
        }
    }
    if profile.op_enabled("links", enabled, disabled) {
        let prior = std::mem::take(&mut out);
        // Threshold 1 is the unchanged default: reuse `fix_links`; higher
        // thresholds delegate to `fix_links_with_min`.
        let result = if links_min_occurrences <= 1 {
            fix::fix_links(&prior)
        } else {
            fix::fix_links_with_min(&prior, links_min_occurrences)
        };
        match result {
            (Cow::Owned(after), pairs) => {
                change_records.extend(changes::link_changes(&pairs));
                out = after;
            }
            (Cow::Borrowed(_), _) => out = prior,
        }
    }
    if !dry_run && out != source {
        io::atomic_write(path, &out)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(change_records)
}

/// Reorder a single source file.
///
/// Returns one per-file [`changes::Change`] record per moved item (derived from
/// the reorder crate's `ReorderMove` producer) in both dry-run and in-place
/// modes.
///
/// Writes the reordered source only when not in dry-run and the output differs
/// from the original.
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

    // 2. Parse - extract items, spans, comments, preamble/trailer
    let parsed = parse::parse_source(&source)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    // 3. Build reference graph and compute topological order (the Rust
    //    profile reproduces the engine's Rust phase order)
    let order = graph::compute_order(&parsed, &graph::RustProfile)
        .context("failed to compute item order")?;

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

    let mut change_records = Vec::new();
    for mv in rust_llm_tidy_reorder::compute_moves(&parsed.items, &permutation) {
        // `from` is the 1-based input sequence position; the anchor line is
        // the moved item's first source line.
        let item = &parsed.items[mv.from() - 1];
        change_records.push(changes::Change {
            line: std::num::NonZeroU32::new(item.start_line() as u32),
            code: "REORDER",
            message: mv.message().into_boxed_str(),
            kind: changes::ChangeKind::Item(*mv.kind()),
            name: mv.name().map(Box::from),
        });
    }
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
pub(crate) fn resolve_vis_context(paths: &[PathBuf]) -> Option<VisContext> {
    let first = paths
        .iter()
        .find(|p| crate::paths::ext_in(p.extension().and_then(|e| e.to_str()), &["rs"]))?;
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
                        Err(e) => eprintln!("warning: could not parse {}: {e}", f.display()),
                    }
                }
            }
            let tree = match build_module_tree(&root, &files) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("warning: failed to build module tree ({e:?})");
                    return None;
                }
            };
            for w in tree.warnings() {
                eprintln!("warning: {w}");
            }
            let reexports = collect_crate_reexports(&files);
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
                let per_file = collect_crate_reexports(std::iter::once(&pf));
                narrow_vis_in_tree(&source, None, &per_file)
            }
        }
        None => {
            // Standalone: build a per-file re-export guard from this file only.
            let pf = ParsedFile::new(path.to_path_buf(), source.clone())?;
            let reexports = collect_crate_reexports(std::iter::once(&pf));
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let compiled: Option<CompiledConfig> =
        config::discover_config_path(cli.config.as_deref(), cli.no_config)
            .map(|p| config::load_and_compile(&p))
            .transpose()?;
    let config_ref = compiled.as_ref();

    if cli.validate {
        if cli.no_config {
            bail!("--no-config was passed; no config to validate");
        }
        let path = config::discover_config_path(cli.config.as_deref(), false)
            .context("no config file found; run from a directory with .rust-llm-tidy.yml")?;
        config::load_and_compile(&path)?;
        println!("config valid: {}", path.display());
        return Ok(());
    }

    // Validate flag values against known_rules().
    let valid = config::known_rules();
    for op in cli.include.iter().chain(cli.exclude.iter()) {
        if !valid.contains(&op.as_str()) {
            bail!(
                "unknown op/rule `{op}` in --include/--exclude; valid: {}",
                valid.join(", ")
            );
        }
    }
    for ext in &cli.extension {
        langs::validate_extension(ext)?;
    }
    let cli_include: Option<HashSet<String>> = if cli.include.is_empty() {
        None
    } else {
        Some(cli.include.iter().cloned().collect())
    };
    let cli_disabled: HashSet<String> = cli.exclude.iter().cloned().collect();

    pipeline::run_pipeline(&cli, config_ref, cli_include.as_ref(), &cli_disabled)
}
