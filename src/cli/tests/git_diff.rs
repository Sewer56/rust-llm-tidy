//! Integration tests for the no-args git-diff fallback of
//! `rust-llm-tidy`.
//!
//! Each test builds a throwaway git repo, makes a change, and runs the
//! binary with no path args. Guarded by `git_available()` so dev machines
//! without git skip rather than fail (CI always has git).

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

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

/// Empty diff still fails when the config is invalid: config validation runs
/// up front, before the empty-list short-circuit (REQ-006 half:
/// "config still validated up front").
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

/// A staged `.MD`/`.RS` change is selected by the no-args git-diff path,
/// so extension admission is case-insensitive there too.
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
        "git-diff must admit .RS/.MD variants: {}",
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

/// Remove a throwaway temp dir created by `temp_dir`/`init_repo`.
fn cleanup(dir: &std::path::Path) {
    let _ = fs::remove_dir_all(dir);
}

// -- Helpers (mirrors integration.rs) --------------------------------
// Note: `binary`, `temp_dir`, and `TEST_COUNTER` are duplicated from
// integration.rs. In a future cleanup, extract these into a
// `tests/common/mod.rs` shared module so git_diff.rs only owns
// `git`/`git_available`.

/// Spawn a fresh git repo in a temp dir, or return `None` when git is
/// unavailable so the test skips (dev machines without git).
fn init_repo() -> Option<std::path::PathBuf> {
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
fn run(current_dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(binary())
        .current_dir(current_dir)
        .args(args)
        .output()
        .expect("failed to spawn")
}

fn binary() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rust_llm_tidy") {
        return std::path::PathBuf::from(path);
    }
    let mut path = std::env::current_exe().expect("current_exe must resolve");
    path.pop();
    path.pop();
    path.join("rust-llm-tidy")
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
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

fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-git-{}-{}", pid, seq))
}
