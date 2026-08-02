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

/// No args + uncommitted change -> file is tidied in place via the default
/// command.
#[test]
fn no_args_processes_git_diff() {
    if !git_available() {
        return;
    }
    let repo = temp_dir();
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    // Commit the canonical (caller-first) state.
    let file = "a file 'quoted'.rs";
    fs::write(repo.join(file), "fn b() { a(); }\nfn a() {}\n").unwrap();
    git(&repo, &["add", file]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    // Stage an unsorted change: callee-first is non-canonical.
    fs::write(repo.join(file), "fn a() {}\nfn b() { a(); }\n").unwrap();
    git(&repo, &["add", file]);
    let out = Command::new(binary())
        .current_dir(&repo)
        .args(["--no-config", "--include", "reorder"])
        .output()
        .expect("failed to spawn");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let actual = fs::read_to_string(repo.join(file)).unwrap();
    assert!(actual.find("fn b").unwrap() < actual.find("fn a").unwrap()); // caller first
    let _ = fs::remove_dir_all(&repo);
}

/// No args + tracked change and untracked file -> both files processed.
#[test]
fn no_args_processes_tracked_and_untracked_files() {
    if !git_available() {
        return;
    }
    let repo = temp_dir();
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "a.rs"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    // Tracked, un-sorted change (callee-first is non-canonical).
    fs::write(repo.join("a.rs"), "fn a() {}\nfn b() { a(); }\n").unwrap();
    // Untracked file must be included alongside tracked changes.
    fs::write(repo.join("b.rs"), "fn a() {}\nfn b() { a(); }\n").unwrap();
    let out = Command::new(binary())
        .current_dir(&repo)
        .args(["--no-config", "--include", "reorder"])
        .output()
        .expect("failed to spawn");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let actual = fs::read_to_string(repo.join("a.rs")).unwrap();
    assert!(actual.find("fn b").unwrap() < actual.find("fn a").unwrap());
    let untracked = fs::read_to_string(repo.join("b.rs")).unwrap();
    assert!(untracked.find("fn b").unwrap() < untracked.find("fn a").unwrap());
    let _ = fs::remove_dir_all(&repo);
}

/// No args + only a deleted file -> nothing to do, exit 0.
#[test]
fn no_args_empty_diff_succeeds() {
    if !git_available() {
        return;
    }
    let repo = temp_dir();
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
    fs::write(repo.join("a.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "a.rs"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    // Delete the only file (deletions are skipped by --diff-filter=ACMR).
    fs::remove_file(repo.join("a.rs")).unwrap();
    let out = Command::new(binary())
        .current_dir(&repo)
        .args(["--no-config"])
        .output()
        .expect("failed to spawn");
    assert!(
        out.status.success(),
        "empty diff must succeed (0 files processed): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = fs::remove_dir_all(&repo);
}

/// Empty diff still fails when the config is invalid: config validation runs
/// up front, before the empty-list short-circuit (REQ-006 half:
/// "config still validated up front").
#[test]
fn no_args_empty_diff_with_bad_config_errors() {
    if !git_available() {
        return;
    }
    let repo = temp_dir();
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "user.name", "t"]);
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
    let out = Command::new(binary())
        .current_dir(&repo)
        .arg("--config")
        .arg(&cfg)
        .output()
        .expect("failed to spawn");
    assert!(
        !out.status.success(),
        "empty diff must still hard-fail on an invalid config: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = fs::remove_dir_all(&repo);
}

/// No args outside a git repo -> non-zero exit, helpful stderr.
#[test]
fn no_args_not_in_repo_errors() {
    if !git_available() {
        return;
    }
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let out = Command::new(binary())
        .current_dir(&dir)
        .args(["--no-config"])
        .output()
        .expect("failed to spawn");
    assert!(
        !out.status.success(),
        "no args + not in a repo must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.is_empty(),
        "stderr should tell the user to pass paths: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

// -- Helpers (mirrors integration.rs) --------------------------------
// Note: `binary`, `temp_dir`, and `TEST_COUNTER` are duplicated from
// integration.rs. In a future cleanup, extract these into a
// `tests/common/mod.rs` shared module so git_diff.rs only owns
// `git`/`git_available`.

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

fn binary() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rust_llm_tidy") {
        return std::path::PathBuf::from(path);
    }
    let mut path = std::env::current_exe().expect("current_exe must resolve");
    path.pop();
    path.pop();
    path.join("rust-llm-tidy")
}
