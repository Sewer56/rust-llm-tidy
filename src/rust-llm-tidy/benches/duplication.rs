//! Measure the production DUP001 file path with reproducible growing workloads.
//!
//! Run `cargo bench -p rust-llm-tidy --bench duplication -- WORKLOAD LINES MODE`.
//!
//! Workloads: `unique`, `groups`, `long`, `tiny`, `repeated`. MODE is `on` or
//! `off`. Timings printed here exclude fixture setup.
//!
//! Measure process peak RSS with GNU time or `wait4`. Both modes use the same
//! setup and warm filesystem caches.

use core::fmt::Write;
use rust_llm_tidy::config::DuplicationConfig;
use rust_llm_tidy::{RunOptions, run};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Generate one file, optionally capture a tiny Git query, then consume findings.
fn main() {
    let args: Vec<_> = env::args().skip(1).filter(|arg| arg != "--bench").collect();
    let workload = args.first().map_or("unique", String::as_str);
    let lines: usize = args
        .get(1)
        .map_or(Ok(1000), |arg| arg.parse())
        .expect("line count");
    let enabled = args.get(2).is_none_or(|arg| arg == "on");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.js");
    let defaults = DuplicationConfig::default();
    let source = fixture(workload, lines, defaults);
    fs::write(&path, &source).unwrap();

    if workload == "tiny" {
        git(directory.path(), &["init", "--quiet"]);
        git(directory.path(), &["add", "."]);
        git(directory.path(), &["commit", "--quiet", "-m", "baseline"]);
        let mut current = source.clone();
        current.push_str(&block(defaults.min_meaningful_lines));
        fs::write(&path, current).unwrap();
    }
    let before = fs::read_to_string(&path).unwrap();
    let options = RunOptions {
        paths: vec![path.clone()],
        include: vec!["DUP001".into()],
        exclude: if enabled {
            vec![]
        } else {
            vec!["DUP001".into()]
        },
        all_lines: workload != "tiny",
        git_changed: workload == "tiny",
        ..RunOptions::default()
    };

    let start = Instant::now();
    let report = run(&options, None).unwrap();
    let elapsed = start.elapsed();
    report.ensure_success().unwrap();
    let findings: usize = report.files.iter().map(|file| file.diagnostics.len()).sum();
    let rendered_bytes: usize = report
        .files
        .iter()
        .flat_map(|file| &file.diagnostics)
        .map(|diagnostic| diagnostic.to_string().len())
        .sum();

    assert_eq!(fs::read_to_string(path).unwrap(), before);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    println!(
        "workload={workload} lines={} bytes={} query_lines={} enabled={enabled} elapsed_us={} findings={findings} rendered_bytes={rendered_bytes}",
        before.lines().count(),
        before.len(),
        if workload == "tiny" {
            defaults.min_meaningful_lines
        } else {
            before.lines().count()
        },
        elapsed.as_micros()
    );
}

/// Build deterministic source with no dependency on historical prototype fixtures.
fn fixture(workload: &str, lines: usize, config: DuplicationConfig) -> String {
    let mut source = String::new();
    match workload {
        "unique" | "tiny" => {
            for index in 0..lines {
                writeln!(source, "classify_{index}();").unwrap();
            }
            if workload == "tiny" {
                source.push_str(
                    &block(config.min_meaningful_lines).repeat(config.min_occurrences - 1),
                );
            }
        }
        "repeated" => source = "classify();\n".repeat(lines),
        "long" => source = block(lines / config.min_occurrences).repeat(config.min_occurrences),
        "groups" => {
            let groups = lines / (config.min_meaningful_lines * config.min_occurrences);
            for group in 0..groups {
                for site in 0..config.min_occurrences {
                    for line in 0..config.min_meaningful_lines {
                        writeln!(source, "classify_{group}_{line}();").unwrap();
                    }
                    writeln!(source, "separator_{group}_{site}();").unwrap();
                }
            }
        }
        _ => panic!("unknown workload: {workload}"),
    }
    source
}

/// Run fixture-local Git without host identity, hooks, signing, or clock inputs.
fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(directory)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
}

/// Produce a distinct line sequence whose copies match only at equal offsets.
fn block(lines: usize) -> String {
    let mut source = String::new();
    for index in 0..lines {
        writeln!(source, "step_{index}();").unwrap();
    }
    source
}
