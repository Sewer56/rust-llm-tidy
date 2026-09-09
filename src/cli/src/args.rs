//! Command-line arguments and output selection.

use crate::output;
use clap::Parser;
use std::path::PathBuf;

/// Command-line arguments for `rust-llm-tidy`, parsed via `clap`.
///
/// Collects the input paths plus flags controlling dry-run, validation, rule
/// selection, allowed extensions, and config discovery.
#[derive(Parser)]
#[command(
    name = "rust-llm-tidy",
    about = "Fix, reorder, narrow visibility, and lint allowed source files"
)]
pub(crate) struct Cli {
    /// Paths to Rust source files or directories to process.
    ///
    /// - Directories: expanded recursively.
    /// - Omitted paths: use changed files in the current git diff,
    ///   filtered to the allowed extensions.
    pub(crate) paths: Vec<PathBuf>,
    /// Preview without modifying files; exit nonzero if transformations are needed.
    ///
    /// Processing failures and error findings also fail. Warnings, hints, and
    /// reminders alone do not. Skips external post-processing commands.
    #[arg(long)]
    pub(crate) dry_run: bool,
    /// Local baseline reference. Overrides RUST_LLM_TIDY_DIFF_BASE.
    ///
    /// Without paths, discover eligible files recursively from the current directory.
    #[arg(long, value_name = "REF")]
    pub(crate) diff_base: Option<String>,
    /// Report all lines for every severity, overriding entry and config scopes.
    /// Omitted respects configured defaults; does not enable lints or change discovery.
    #[arg(long)]
    pub(crate) all_lines: bool,
    /// Validate the config and exit; do not process files.
    #[arg(long)]
    pub(crate) validate: bool,
    /// Run only these rules/lint-codes (repeatable). Overrides config `include`.
    #[arg(long, value_name = "RULE")]
    pub(crate) include: Vec<String>,
    /// Skip these rules/lint-codes (repeatable). Additive to config `exclude`.
    #[arg(long, value_name = "RULE")]
    pub(crate) exclude: Vec<String>,
    /// Allow files with this extension in addition to the config or
    /// default allowed set (repeatable). Written without the leading dot
    /// and matched case-insensitively.
    #[arg(long, value_name = "EXT")]
    pub(crate) extension: Vec<String>,
    /// Path to a `.rust-llm-tidy.yml` config file. Overrides auto-discovery.
    #[arg(long, global = true)]
    pub(crate) config: Option<PathBuf>,
    /// Disable config discovery and loading entirely.
    #[arg(long, global = true, conflicts_with = "config")]
    pub(crate) no_config: bool,
    /// Lint output format selector.
    ///
    /// - `text` (default): prints plaintext diagnostics to stderr.
    /// - `json`: prints a single JSON array of lint findings and dry-run
    ///   change records to stdout.
    #[arg(long, value_name = "MODE", default_value = "text")]
    pub(crate) output_mode: output::OutputMode,
    /// Alias for `--output-mode json`.
    #[arg(long, conflicts_with = "output_mode")]
    pub(crate) json: bool,
}
