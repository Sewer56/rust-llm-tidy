//! Command-line adapter for the rust-llm-tidy library.

use anyhow::{Context, bail};
use clap::Parser;
use rust_llm_tidy::{RunOptions, config, run};
use std::env;

mod args;
mod output;

fn main() -> anyhow::Result<()> {
    let cli = args::Cli::parse();
    let json = cli.output_mode == output::OutputMode::Json || cli.json;
    let config_path = config::discover_config_path(cli.config.as_deref(), cli.no_config);
    let compiled = config_path
        .as_deref()
        .map(config::load_and_compile)
        .transpose()?;

    if cli.validate {
        if cli.no_config {
            bail!("--no-config was passed; no config to validate");
        }
        let path = config_path
            .context("no config file found; run from a directory with .rust-llm-tidy.yml")?;
        println!("config valid: {}", path.display());
        return Ok(());
    }

    let options = RunOptions {
        paths: cli.paths,
        apply: !cli.dry_run,
        git_changed: true,
        diff_base: cli
            .diff_base
            .or_else(|| env::var("RUST_LLM_TIDY_DIFF_BASE").ok()),
        all_lines: cli.all_lines,
        cargo_discovery: true,
        post_process: true,
        include: cli.include,
        exclude: cli.exclude,
        extensions: cli.extension,
    };
    let report = run(&options, compiled.as_ref())?;

    output::emit_report(&report, json)?;
    report.ensure_success()?;
    if cli.dry_run && report.files.iter().any(|file| !file.changes.is_empty()) {
        bail!("dry-run found proposed transformations; rerun without --dry-run to apply them");
    }
    Ok(())
}
