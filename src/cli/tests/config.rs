//! Integration tests for the `validate` subcommand and config-driven
//! exclusions of `rust-llm-tidy`.
//!
//! Mirrors the helper pattern from `fix.rs`/`doc_check.rs` (`run_command`,
//! `binary`, `manifest_dir`). Each test writes a temp config and/or fixture
//! and runs the built CLI binary with `--config <path>`. Existing tests use
//! `--no-config` (see `fix.rs`), so the repo-root sample config never
//! interferes here.

use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// ── validate ──

/// `validate` exits 0 on a syntactically valid config with at least one match
/// per pattern.
#[test]
fn validate_ok_on_valid_config() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src").join("lib.rs"), "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude:\n  - \"src/lib.rs\"\n").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "validate should exit 0 on a valid config: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `validate` exits non-zero when a pattern matches zero files.
#[test]
fn validate_fails_on_non_matching_path() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude:\n  - \"does/not/exist/**\"\n").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "validate should fail on a non-matching path"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `validate` exits non-zero on an unknown rule name.
#[test]
fn validate_fails_on_unknown_rule() {
    let dir = temp_dir();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src").join("lib.rs"), "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "exclude_rules:\n  - paths: [\"src/**\"]\n    rules: [\"NOPE\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "validate should fail on an unknown rule"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `validate` exits non-zero on malformed YAML.
#[test]
fn validate_fails_on_malformed_yaml() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude: [unclosed\n").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(!output.status.success(), "validate should fail on bad YAML");
    let _ = fs::remove_dir_all(&dir);
}

/// `validate` exits non-zero when no config file is found.
#[test]
fn validate_fails_when_no_config_found() {
    // Run validate from a temp dir with no .rust-llm-tidy.yml and no .git, and
    // pass neither --config nor --no-config. discover walks to fs root without
    // finding a config and returns None, which `validate` treats as failure.
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let output = Command::new(binary())
        .current_dir(&dir)
        .arg("validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "validate should fail when no config is found"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `validate --no-config` exits non-zero because there is no config to
/// validate.
#[test]
fn validate_fails_with_no_config_flag() {
    let output = Command::new(binary())
        .args(["--no-config", "validate"])
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "validate --no-config must exit non-zero"
    );
}

// ── exclude + exclude_rules on subcommands ──

/// `fix --config` with `exclude_rules: [links]` does NOT hoist links on a
/// file that needs link hoisting, while tables/fences are still applied.
#[test]
fn fix_excludes_links_rule() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("in.md");
    // A markdown table with multi-char cells that `fix_tables` would pad-align,
    // plus a repeated inline link that `fix_links` would hoist.
    fs::write(
        &tmp,
        "| Name | Value |\n| --- | --- |\n| a | 1 |\n| longname | 200 |\n\nsee [A](http://x) and [A](http://x)\n",
    )
    .unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "exclude_rules:\n  - paths: [\"in.md\"]\n    rules: [\"links\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "fix", "--dry-run"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "fix should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("[A]: http://x"),
        "links must NOT be hoisted when `links` is disabled: {stdout:?}"
    );
    // Tables are still applied (the `links` disable is selective, not blanket).
    assert!(
        stdout.contains("| -------- |"),
        "tables must still be applied when only `links` is disabled: {stdout:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `fix --config` with `exclude` for the fixture leaves the file unchanged.
#[test]
fn fix_exclude_skips_file() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("in.md");
    let original = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    fs::write(&tmp, original).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude:\n  - \"in.md\"\n").unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "fix"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "fix should succeed even when the file is excluded: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    assert_eq!(actual, original, "excluded file must be unchanged");
    let _ = fs::remove_dir_all(&dir);
}

/// `all --config` with `exclude_rules: [reorder]` fixes/vis/checks but does
/// not reorder (the input is reordered on a normal run; under `reorder` being
/// disabled it must remain in input order).
#[test]
fn all_excludes_reorder_rule() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // Two top-level fns in NON-canonical order (canonical is caller before
    // callee, per the reorder phase); here callee precedes caller so a normal
    // `all` would reorder them.
    fs::write(&tmp, "fn callee() {}\nfn caller() { callee(); }\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "exclude_rules:\n  - paths: [\"lib.rs\"]\n    rules: [\"reorder\"]\n",
    )
    .unwrap();

    let _output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "all"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    // `all` runs fix/reorder/vis/check. With `reorder` disabled, the
    // non-canonical input order (callee before caller) must be preserved.
    // Without the disable, `all` would reorder to caller-before-callee.
    let actual = fs::read_to_string(&tmp).unwrap();
    assert!(
        actual.find("fn callee()").unwrap() < actual.find("fn caller()").unwrap(),
        "reorder disabled: non-canonical callee-before-caller must be preserved"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `check --config` with `exclude_rules: [DOC001]` suppresses DOC001 findings.
#[test]
fn check_excludes_doc001_rule() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // An undocumented pub fn triggers DOC001 + DOC002 (Result with no Errors).
    fs::write(&tmp, "pub fn load() -> Result<(), String> { Ok(()) }\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "exclude_rules:\n  - paths: [\"lib.rs\"]\n    rules: [\"DOC001\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "check"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("DOC001"),
        "DOC001 must be suppressed by exclude_rules: {stderr:?}"
    );
    // A non-disabled diagnostic (DOC002) must still be reported, proving
    // the filter is selective, not clearing all diagnostics.
    assert!(
        stderr.contains("DOC002"),
        "non-disabled DOC002 must still appear: {stderr:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A non-matching-path config hard-fails a regular command (non-zero exit, not
/// a warning).
#[test]
fn regular_command_hard_fails_on_non_matching_path() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude:\n  - \"missing/**\"\n").unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "check"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "non-matching-path config must hard-fail, not warn"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Patterns are resolved relative to the config file's directory (config
/// placed in a temp subdir).
#[test]
fn patterns_resolved_relative_to_config_dir() {
    let dir = temp_dir();
    let sub = dir.join("cfg-dir");
    fs::create_dir_all(&sub).unwrap();
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("lib.rs"), "pub fn example() {}\n").unwrap();

    // Config in `cfg-dir/`, but the excluded path is `../src/lib.rs` relative
    // to the config dir. The config dir is canonicalized, so the relative path
    // must resolve against it.
    let cfg = sub.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude:\n  - \"../src/lib.rs\"\n").unwrap();

    let validate = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "validate"])
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        validate.status.success(),
        "validate should succeed when the pattern matches relative to the config dir: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

// ── flag exclusivity ──

/// `--config` and `--no-config` are mutually exclusive; supplying both
/// causes a non-zero exit (clap `conflicts_with` enforcement).
#[test]
fn config_and_no_config_are_mutually_exclusive() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude: []\n").unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--no-config", "validate"])
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "--config and --no-config must be mutually exclusive"
    );
    let _ = fs::remove_dir_all(&dir);
}

// ── auto-discovery ──

/// Placing `.rust-llm-tidy.yml` in a temp dir with a `.git` marker and running
/// from a subdir discovers the config.
#[test]
fn auto_discovery_walks_up_to_git_root() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".git"), "gitdir: placeholder\n").unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src").join("lib.rs"), "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude:\n  - \"src/lib.rs\"\n").unwrap();

    let sub = dir.join("src");
    let output = Command::new(binary())
        .current_dir(&sub)
        .arg("validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "validate should discover the config by walking up to .git: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

// ── post_process ──

/// A `post_process` step running `true` on a .rs file runs and exit is 0.
#[test]
fn post_process_runs_on_matching_extension() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "post_process:\n  - command: \"true\"\n    extensions: [\"rs\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "fix"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "post_process `true` should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `post_process` with `extensions: [\"md\"]` does NOT run on a .rs file (no
/// failure reported).
#[test]
fn post_process_skips_non_matching_extension() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    // `false` would fail if invoked; restricting to .md means it must not run.
    fs::write(
        &cfg,
        "post_process:\n  - command: \"false\"\n    extensions: [\"md\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "fix"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "post_process must NOT run on a .rs file when extensions=[md]: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A failing `post_process` command causes a non-zero exit.
#[test]
fn post_process_failure_exits_nonzero() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "post_process:\n  - command: \"false\"\n    extensions: [\"rs\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "fix"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "a failing post_process command must exit non-zero"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--dry-run` skips `post_process` entirely (a failing command does not run).
#[test]
fn post_process_skipped_under_dry_run() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "post_process:\n  - command: \"false\"\n    extensions: [\"rs\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "fix", "--dry-run"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "--dry-run must skip post_process so `false` never runs"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `post_process` does not run on `check` (read-only).
#[test]
fn post_process_not_run_on_check() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "post_process:\n  - command: \"false\"\n    extensions: [\"rs\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "check"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    // `check` has no post-process pass, so `false` never runs and the only
    // possible failure is error-severity diagnostics. We assert the binary did
    // not fail *because of post_process* by checking stderr has no
    // "post_process" mention.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("post_process"),
        "check must not invoke post_process: {stderr:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// An excluded file is NOT post-processed.
#[test]
fn excluded_file_not_post_processed() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    // Exclude the file AND run a failing post_process on .rs files. Excluded
    // files are skipped, so post_process never sees the file -> exit 0.
    fs::write(
        &cfg,
        "exclude:\n  - \"lib.rs\"\npost_process:\n  - command: \"false\"\n    extensions: [\"rs\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "fix"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "excluded file must not be post-processed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

// -- Helpers (mirrors fix.rs) -----------------------------------

/// Create a numbered temporary directory.
fn temp_dir() -> std::path::PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-cfg-dir-{}-{}", pid, seq))
}

/// Return the path to the `rust-llm-tidy` debug binary.
fn binary() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rust_llm_tidy") {
        return std::path::PathBuf::from(path);
    }

    let mut path = std::env::current_exe().expect("current_exe must resolve");
    // Drop `<test-name>-<hash>` and `deps/` to reach `target/<triple>/debug/`.
    path.pop();
    path.pop();
    path.join("rust-llm-tidy")
}
