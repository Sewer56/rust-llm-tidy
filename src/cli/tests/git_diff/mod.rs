//! Integration tests for the no-args git-diff fallback of
//! `rust-llm-tidy`.
//!
//! Each test builds a throwaway git repo, makes a change, and runs the
//! binary with no path args. Guarded by `git_available()` so dev machines
//! without git skip rather than fail (CI always has git).
//!
//! No-args discovery cases live in the `no_args` child module.

use common::binary;
use core::sync::atomic::{AtomicU64, Ordering};
use rstest::rstest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Output};

mod no_args;
// The folder root sits inside `tests/git_diff/`, so the sibling modules
// resolve at their `tests/` paths, not under this folder.
#[path = "../common/mod.rs"]
mod common;
#[path = "../documentation_context/mod.rs"]
mod documentation_context;
#[path = "../duplication/mod.rs"]
mod duplication;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

#[rstest]
#[case::new_token("long", "3", "PERF002", false, 2)]
#[case::size_only("int", "4", "PERF002", false, 0)]
#[case::all_override("int", "4", "PERF002", true, 2)]
#[case::independent_perf001("long", "3", "PERF001", true, 2)]
#[case::both_codes("long", "3", "PERF001,PERF002", true, 3)]
#[case::custom_array("long", "3", "", true, 1)]
fn array_reminders_should_respect_code_selection_and_changed_anchor(
    #[case] element: &str,
    #[case] size: &str,
    #[case] codes: &str,
    #[case] all_lines: bool,
    #[case] count: usize,
) {
    let repo = init_repo().expect("Git is required for array reminder acceptance");
    fs::write(
        repo.join(".rust-llm-tidy.yml"),
        format!(
            concat!(
                "perf_hints: [{}]\n",
                "symbol_rules:\n",
                "  - symbol: new[]\n    extensions: [cS]\n    array_kind: any\n",
                "    title: Custom array\n    message: custom array reminder\n"
            ),
            codes
        ),
    )
    .unwrap();
    let source = |element, size| {
        format!(
            "class C {{ void M() {{\nvar a = new {element}[\n{size}];\nvar b = new List<int>();\n}} }}\n"
        )
    };
    fs::write(repo.join("input.CS"), source("int", "3")).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(repo.join("input.CS"), source(element, size)).unwrap();
    let mut args = vec!["--json", "--diff-base", "HEAD", "--include", "SYM"];
    if all_lines {
        args.push("--all-lines");
    }

    let output = run(&repo, &args);
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(records.len(), count, "{records:?}");
    assert!(
        records
            .iter()
            .all(|record| record["severity"] == "reminder")
    );
    assert_eq!(
        fs::read_to_string(repo.join("input.CS")).unwrap(),
        source(element, size)
    );
    cleanup(&repo);
}

#[test]
fn no_paths_should_reject_bad_explicit_baseline_without_selected_files() {
    let repo = init_repo().expect("Git is required for baseline acceptance");
    fs::write(repo.join(".rust-llm-tidy.yml"), "extensions: [rs]").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);

    let output = run(
        &repo,
        &["--diff-base", "missing-reference", "--include", "links"],
    );

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("explicit baseline"));
    cleanup(&repo);
}

#[rstest]
#[case::flag(Some("HEAD~1"), None)]
#[case::environment(None, Some("HEAD~1"))]
#[case::flag_wins(Some("HEAD~1"), Some("missing-reference"))]
fn no_paths_should_report_committed_reminders_against_explicit_baseline(
    #[case] flag: Option<&str>,
    #[case] environment: Option<&str>,
) {
    let repo = init_repo().expect("Git is required for baseline acceptance");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}").unwrap();
    fs::write(repo.join("input.rs"), "fn f() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(repo.join("input.rs"), "fn f() { Vec::new(); }\n").unwrap();
    git(&repo, &["add", "input.rs"]);
    git(&repo, &["commit", "--quiet", "-m", "change"]);
    assert!(git(&repo, &["status", "--porcelain"]).is_empty());

    let mut command = Command::new(binary());
    command
        .current_dir(&repo)
        .args(["--include", "SYM", "--json"])
        .env_remove("RUST_LLM_TIDY_DIFF_BASE");
    if let Some(flag) = flag {
        command.args(["--diff-base", flag]);
    }
    if let Some(environment) = environment {
        command.env("RUST_LLM_TIDY_DIFF_BASE", environment);
    }

    let output = command.output().unwrap();
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["code"], "SYM");
    assert_eq!(records[0]["title"], "PERF001: API performance reminder");
    assert_eq!(records[0]["severity"], "reminder");
    cleanup(&repo);
}

/// Sorted paths from a successful `--json` run.
fn sorted_paths(output: &Output) -> Vec<String> {
    assert!(output.status.success(), "{output:?}");
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut paths: Vec<String> = findings
        .iter()
        .map(|finding| {
            finding["path"]
                .as_str()
                .expect("path is a string")
                .to_owned()
        })
        .collect();
    paths.sort();
    paths
}

/// TEST002 reports only when a test's declaration line is in the diff.
///
/// A body-only edit leaves the declaration line unchanged and stays silent;
/// an added test produces a finding; `--all-lines` audits unchanged tests too.
#[rstest]
#[case::body_only("#[test]\nfn summarised() {\n    assert!(false);\n}\n", false, vec![])]
#[case::added_test(
    "#[test]\nfn summarised() {\n    assert!(false);\n}\n\n#[test]\nfn added() {}\n",
    false,
    vec![6]
)]
#[case::all_lines(
    "#[test]\nfn summarised() {\n    assert!(false);\n}\n\n#[test]\nfn added() {}\n",
    true,
    vec![1, 6]
)]
fn test002_should_gate_on_the_declaration_line(
    #[case] current: &str,
    #[case] all_lines: bool,
    #[case] expected_lines: Vec<usize>,
) {
    let repo = init_repo().expect("Git is required for TEST002 acceptance");
    let path = repo.join("input.rs");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}").unwrap();
    fs::write(&path, "#[test]\nfn summarised() {\n    assert!(true);\n}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(&path, current).unwrap();
    git(&repo, &["add", "input.rs"]);
    git(&repo, &["commit", "--quiet", "-m", "change"]);

    let mut args = vec!["--diff-base", "HEAD~1", "--include", "TEST002", "--json"];
    if all_lines {
        args.push("--all-lines");
    }
    let output = run(&repo, &args);
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<usize> = findings
        .iter()
        .map(|finding| finding["line"].as_u64().unwrap() as usize)
        .collect();
    assert_eq!(lines, expected_lines, "{findings:?}");
    assert!(
        findings
            .iter()
            .all(|finding| finding["code"] == "TEST002" && finding["severity"] == "reminder"),
        "{findings:?}"
    );
    cleanup(&repo);
}

/// Remove a throwaway temp dir created by `temp_dir`/`init_repo`.
fn cleanup(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

// -- Helpers (mirrors integration.rs) --------------------------------
//
// Note: `temp_dir` and `TEST_COUNTER` are still duplicated from
// integration.rs; `binary` now lives in the shared `tests/common/mod.rs`
// module.
//
// In a future cleanup, extract the rest likewise so git_diff.rs only owns
// `git`/`git_available`.

/// Spawn a fresh git repo in a temp dir, or return `None` when git is
/// unavailable so the test skips (dev machines without git).
fn init_repo() -> Option<PathBuf> {
    if !git_available() {
        return None;
    }
    let repo = temp_dir();
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    Some(repo)
}

/// Run the binary with `args` in `current_dir`, returning the raw `Output`.
fn run(current_dir: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .current_dir(current_dir)
        .args(args)
        .output()
        .expect("failed to spawn")
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run git {}: {e}", args.join(" ")));
    if !out.status.success() {
        panic!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn temp_dir() -> PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-git-{}-{}", pid, seq))
}
