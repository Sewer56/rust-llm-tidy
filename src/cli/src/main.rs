//! `rust-auto-reorder` - reorder and lint Rust source files.
//!
//! A unified CLI for two operations:
//!
//! - **reorder**: reorder Rust source file items into a canonical 10-phase
//!   ordering (the original behavior).
//! - **check**: lint for missing documentation and incomplete `# Errors`
//!   sections (read-only, never writes).
//!
//! Use `all` to run both in one pass: reorder (fix what is auto-fixable) then
//! check (report what remains).
//!
//! # Subcommands
//!
//! | Command   | Mutates?                 | Description                                        |
//! | --------- | ------------------------ | -------------------------------------------------- |
//! | `reorder` | yes (unless `--dry-run`) | Reorder items into canonical order                 |
//! | `check`   | no                       | Report documentation and test-naming lint findings |
//! | `all`     | yes (unless `--dry-run`) | Reorder then check                                 |
//!
//! Multiple paths are accepted; each directory is expanded recursively.

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use proc_macro2::fallback::force;
use rust_auto_reorder::graph;
use rust_auto_reorder::io;
use rust_auto_reorder::parse;
use rust_auto_reorder::reorder::Permutation;
use rust_auto_reorder::safety;
use rust_doc_check::check;
use rust_source_model::parse as model_parse;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "rust-auto-reorder",
    about = "Reorder and lint Rust source files"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Reorder items into canonical 10-phase order.
    ///
    /// Mutates files in place unless --dry-run is given.
    Reorder(PathsArgs),
    /// Check documentation coverage and error sections.
    ///
    /// Read-only: never writes files. Exits non-zero when any error-severity
    /// diagnostic is found.
    Check(PathsArgs),
    /// Reorder then check in one pass.
    ///
    /// Reorders first (fixing what is auto-fixable), then reports any
    /// remaining documentation gaps. Mutates files unless --dry-run is given.
    All(PathsArgs),
}

/// Shared path and dry-run arguments for every subcommand.
///
/// `--dry-run` is accepted by all subcommands; for `check` it is a no-op
/// (checking never writes anyway).
#[derive(Args)]
struct PathsArgs {
    /// Path(s) to the Rust source file(s) or directory(s) to process. Each
    /// directory is expanded recursively. Multiple paths are processed in the
    /// order given; duplicates are kept.
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    /// For `reorder`/`all`: print results to stdout instead of modifying
    /// files. For `check`: accepted but ignored (checking is always read-only).
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
        Command::Check(args) => run_check(args),
        Command::All(args) => run_all(args),
    }
}

// ---------------------------------------------------------------------------
// Subcommand handlers
// ---------------------------------------------------------------------------

/// `all` - reorder then check in one pass.
fn run_all(args: PathsArgs) -> anyhow::Result<()> {
    let paths = resolve_all(&args.paths)?;
    if paths.is_empty() {
        return Ok(());
    }

    let multiple_files = paths.len() > 1;
    let mut error_count = 0usize;
    let mut failed = Vec::new();

    for path in &paths {
        // Reorder first (fixes ordering). Failures abort the check for this file.
        if let Err(e) = reorder_file(path, args.dry_run, multiple_files) {
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
    let paths = resolve_all(&args.paths)?;
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

/// `reorder` - reorder items into canonical order.
fn run_reorder(args: PathsArgs) -> anyhow::Result<()> {
    let paths = resolve_all(&args.paths)?;
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
        .filter(|d| matches!(d.severity, rust_doc_check::Severity::Error))
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
    let output = rust_auto_reorder::reorder::emit(&parsed, &permutation)
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

/// Resolve a list of input paths into a flat, ordered list of `.rs` files.
fn resolve_all(inputs: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for input in inputs {
        let resolved = resolve_paths(input)
            .with_context(|| format!("failed to resolve path {}", input.display()))?;
        paths.extend(resolved);
    }
    Ok(paths)
}

/// Resolve `path` into a sorted list of `.rs` files to process.
///
/// If `path` is a file, it is returned directly. If it is a directory,
/// all `.rs` files are collected recursively and sorted for deterministic
/// ordering.
fn resolve_paths(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
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
    collect_rs_files(path, &mut files)
        .with_context(|| format!("failed to read directory {}", path.display()))?;
    files.sort();

    Ok(files)
}

/// Recursively collect all `.rs` files under `dir`.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            collect_rs_files(&path, out)?;
        } else if metadata.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }

    Ok(())
}
