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
    /// Path(s) to the Rust source file(s) or directory(s) to process. Each
    /// directory is expanded recursively. When omitted, the changed files in
    /// the current git diff are used, filtered to the allowed extensions.
    pub(crate) paths: Vec<PathBuf>,
    /// Print the changes that would be made instead of modifying files.
    #[arg(long)]
    pub(crate) dry_run: bool,
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
    /// Lint output format: `text` (default) prints plaintext diagnostics to
    /// stderr; `json` prints a single JSON array of lint findings and dry-run
    /// change records to stdout.
    #[arg(long, value_name = "MODE", default_value = "text")]
    pub(crate) output_mode: output::OutputMode,
    /// Alias for `--output-mode json`.
    #[arg(long, conflicts_with = "output_mode")]
    pub(crate) json: bool,
}
