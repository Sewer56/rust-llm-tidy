//! Integration tests for the no-args git-diff fallback of
//! `rust-llm-tidy`.
//!
//! Each test builds a throwaway git repo, makes a change, and runs the
//! binary with no path args. Guarded by `git_available()` so dev machines
//! without git skip rather than fail (CI always has git).

use common::binary;
use core::sync::atomic::{AtomicU64, Ordering};
use rstest::rstest;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Output};

mod common;
mod duplication;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn all_lines_should_not_discover_unchanged_or_untracked_files() {
    let repo = init_repo().expect("Git is required for discovery acceptance");
    fs::write(repo.join(".rust-llm-tidy.yml"), "{}").unwrap();
    fs::write(repo.join("input.rs"), "fn f() { Vec::new(); }").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "baseline"]);
    fs::write(repo.join("untracked.rs"), "fn f() { Vec::new(); }").unwrap();

    let output = run(&repo, &["--all-lines", "--include", "SYM", "--json"]);
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(records.is_empty(), "{records:?}");
    cleanup(&repo);
}

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

/// No args + only a deleted file -> nothing to do, exit 0.
#[test]
fn no_args_empty_diff_succeeds() {
    let Some(repo) = init_repo() else {
        return;
    };
    fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "a.rs"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    // Delete the only file (deletions are skipped by --diff-filter=ACMR).
    fs::remove_file(repo.join("a.rs")).unwrap();
    let out = run(&repo, &["--no-config"]);
    assert!(
        out.status.success(),
        "empty diff must succeed (0 files processed): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    cleanup(&repo);
}

/// Empty diff still fails when the config is invalid.
///
/// Config validation runs up front, before the empty-list short-circuit
/// (REQ-006 half: "config still validated up front").
#[test]
fn no_args_empty_diff_with_bad_config_errors() {
    let Some(repo) = init_repo() else {
        return;
    };
    fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "a.rs"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    // Empty diff: delete the only file (deletions skipped by --diff-filter=ACMR).
    fs::remove_file(repo.join("a.rs")).unwrap();
    let cfg = repo.join(".rust-llm-tidy.yml");
    // include + exclude both present -> config-load error.
    fs::write(
        &cfg,
        "include:\n  - rules: [tables]\nexclude:\n  - rules: [reorder]\n",
    )
    .unwrap();
    let out = run(&repo, &["--config", cfg.to_str().unwrap()]);
    assert!(
        !out.status.success(),
        "empty diff must still hard-fail on an invalid config: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    cleanup(&repo);
}

/// No args from nested cwd + relative diff -> root-level tracked file is processed.
#[test]
fn no_args_nested_cwd_ignores_relative_diff_config() {
    let Some(repo) = init_repo() else {
        return;
    };
    fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "a.rs"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    fs::write(repo.join("a.rs"), "fn a() {}\nfn b() { a(); }\n").unwrap();
    git(&repo, &["config", "diff.relative", "true"]);
    let nested = repo.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let out = run(&nested, &["--no-config", "--include", "reorder"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let actual = fs::read_to_string(repo.join("a.rs")).unwrap();
    assert!(actual.find("fn b").unwrap() < actual.find("fn a").unwrap());
    cleanup(&repo);
}

/// No args outside a git repo -> non-zero exit, helpful stderr.
#[test]
fn no_args_not_in_repo_errors() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let out = run(&dir, &["--no-config"]);
    assert!(
        !out.status.success(),
        "no args + not in a repo must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.is_empty(),
        "stderr should tell the user to pass paths: {stderr}"
    );
    cleanup(&dir);
}

/// No args + uncommitted change -> file is tidied in place via the default
/// command.
#[test]
fn no_args_processes_git_diff() {
    let Some(repo) = init_repo() else {
        return;
    };
    // Commit the canonical (caller-first) state.
    let file = "a file 'quoted'.rs";
    fs::write(repo.join(file), "fn b() { a(); }\nfn a() {}\n").unwrap();
    git(&repo, &["add", file]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    // Stage an unsorted change: callee-first is non-canonical.
    fs::write(repo.join(file), "fn a() {}\nfn b() { a(); }\n").unwrap();
    git(&repo, &["add", file]);
    let out = run(&repo, &["--no-config", "--include", "reorder"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let actual = fs::read_to_string(repo.join(file)).unwrap();
    assert!(actual.find("fn b").unwrap() < actual.find("fn a").unwrap()); // caller first
    cleanup(&repo);
}

/// No-args git-diff mode selects changed files of newly allowed extensions.
///
/// Extensions: `.py`, `.cs`, `.markdown`, matched case-insensitively
/// through the same allowed list. A follow-up run finds nothing left to
/// change.
#[test]
fn no_args_selects_newly_allowed_extensions() {
    let Some(repo) = init_repo() else {
        return;
    };
    let py = repo.join("notes.py");
    let cs = repo.join("Doc.CS");
    let md = repo.join("README.MARKDOWN");
    fs::write(&py, "# | a | b |\n# | --- | --- |\n# | 1 | 22 |\n").unwrap();
    fs::write(&cs, "// | a | b |\n// | --- | --- |\n// | 1 | 22 |\n").unwrap();
    fs::write(&md, "| a | b |\n| --- | --- |\n| 1 | 22 |\n").unwrap();
    git(&repo, &["add", "."]);

    let out = run(&repo, &["--no-config", "--include", "tables"]);
    assert!(
        out.status.success(),
        "git-diff must allow .py/.CS/.MARKDOWN: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    for name in ["notes.py", "Doc.CS", "README.MARKDOWN"] {
        assert!(
            stderr.contains(name),
            "{name} must be selected and table-fixed: {stderr}"
        );
    }
    assert_eq!(
        stderr.matches("tables were aligned").count(),
        3,
        "each staged file must report one table fix: {stderr}"
    );

    // The fixes are in place, so a second no-args run finds nothing.
    let out = run(&repo, &["--no-config", "--include", "tables"]);
    assert!(
        out.status.success(),
        "second git-diff run should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("success[FIX]"),
        "second run must emit zero change records"
    );
    cleanup(&repo);
}

/// A staged `.MD`/`.RS` change is selected by the no-args git-diff path,
/// so the allowed-extension match is case-insensitive there too.
#[test]
fn no_args_selects_uppercase_extension_variants() {
    let Some(repo) = init_repo() else {
        return;
    };
    fs::write(repo.join("lib.RS"), "fn b() { a(); }\nfn a() {}\n").unwrap();
    fs::write(
        repo.join("README.MD"),
        "| Name | Value |\n| --- | --- |\n| a | 1 |\n| long | 2 |\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    // Stage an unsorted change on `.RS` (caller-before-callee is canonical).
    fs::write(repo.join("lib.RS"), "fn a() {}\nfn b() { a(); }\n").unwrap();
    git(&repo, &["add", "lib.RS"]);
    // Stage an unaligned table change on `.MD`.
    fs::write(
        repo.join("README.MD"),
        "| Name | Value |\n| --- | --- |\n| a        | 1   |\n",
    )
    .unwrap();
    git(&repo, &["add", "README.MD"]);

    let out = run(
        &repo,
        &["--no-config", "--include", "reorder", "--include", "tables"],
    );
    assert!(
        out.status.success(),
        "git-diff must allow .RS/.MD variants: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("success[REORDER]"),
        "staged .RS must be selected and reordered: {stderr}"
    );
    assert!(
        stderr.contains("success[FIX]"),
        "staged .MD must be selected and table-fixed: {stderr}"
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
