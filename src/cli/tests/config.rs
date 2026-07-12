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

/// `--validate` exits 0 on a syntactically valid config with at least one match
/// per pattern.
#[test]
fn validate_ok_on_valid_config() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src").join("lib.rs"), "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude_files:\n  - \"src/lib.rs\"\n").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("--validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "--validate should exit 0 on a valid config: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--validate` exits non-zero when a pattern matches zero files.
#[test]
fn validate_fails_on_non_matching_path() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude_files:\n  - \"does/not/exist/**\"\n").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("--validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "--validate should fail on a non-matching path"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--validate` exits non-zero on an unknown rule name.
#[test]
fn validate_fails_on_unknown_rule() {
    let dir = temp_dir();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src").join("lib.rs"), "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "exclude:\n  - paths: [\"src/**\"]\n    rules: [\"NOPE\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("--validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "--validate should fail on an unknown rule"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--validate` exits non-zero on malformed YAML.
#[test]
fn validate_fails_on_malformed_yaml() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude_files: [unclosed\n").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("--validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "--validate should fail on bad YAML"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--validate` exits non-zero when no config file is found.
#[test]
fn validate_fails_when_no_config_found() {
    // Run --validate from a temp dir with no .rust-llm-tidy.yml and no .git, and
    // pass neither --config nor --no-config. discover walks to fs root without
    // finding a config and returns None, which `--validate` treats as failure.
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let output = Command::new(binary())
        .current_dir(&dir)
        .arg("--validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "--validate should fail when no config is found"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `validate --no-config` exits non-zero because there is no config to
/// validate.
#[test]
fn validate_fails_with_no_config_flag() {
    let output = Command::new(binary())
        .args(["--no-config", "--validate"])
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "--validate --no-config must exit non-zero"
    );
}

// ── exclude + exclude_rules on subcommands ──

/// Bare default command with `exclude: [links]` does NOT hoist links on a
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
        "exclude:\n  - paths: [\"in.md\"]\n    rules: [\"links\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--dry-run"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "default command should succeed: {}",
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

/// Default command with `exclude_files` for the fixture leaves the file unchanged.
#[test]
fn fix_exclude_skips_file() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("in.md");
    let original = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    fs::write(&tmp, original).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude_files:\n  - \"in.md\"\n").unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "default command should succeed even when the file is excluded: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    assert_eq!(actual, original, "excluded file must be unchanged");
    let _ = fs::remove_dir_all(&dir);
}

/// Default command with `exclude: [reorder]` fixes/vis/lints but does
/// not reorder (the input is reordered on a normal run; under `reorder` being
/// disabled it must remain in input order).
#[test]
fn all_excludes_reorder_rule() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // Two top-level fns in NON-canonical order (canonical is caller before
    // callee, per the reorder phase); here callee precedes caller so a normal
    // run would reorder them.
    fs::write(&tmp, "fn callee() {}\nfn caller() { callee(); }\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "exclude:\n  - paths: [\"lib.rs\"]\n    rules: [\"reorder\"]\n",
    )
    .unwrap();

    let _output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    // Default command runs fix/reorder/vis/lints. With `reorder` disabled, the
    // non-canonical input order (callee before caller) must be preserved.
    // Without the disable, it would reorder to caller-before-callee.
    let actual = fs::read_to_string(&tmp).unwrap();
    assert!(
        actual.find("fn callee()").unwrap() < actual.find("fn caller()").unwrap(),
        "reorder disabled: non-canonical callee-before-caller must be preserved"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Default command with `exclude: [DOC001]` suppresses DOC001 findings.
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
        "exclude:\n  - paths: [\"lib.rs\"]\n    rules: [\"DOC001\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("DOC001"),
        "DOC001 must be suppressed by exclude: {stderr:?}"
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
    fs::write(&cfg, "exclude_files:\n  - \"missing/**\"\n").unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
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
    fs::write(&cfg, "exclude_files:\n  - \"../src/lib.rs\"\n").unwrap();

    let validate = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--validate"])
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        validate.status.success(),
        "--validate should succeed when the pattern matches relative to the config dir: {}",
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
    fs::write(&cfg, "exclude_files: []\n").unwrap();

    let output = Command::new(binary())
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "--no-config",
            "--validate",
        ])
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
    fs::write(&cfg, "exclude_files:\n  - \"src/lib.rs\"\n").unwrap();

    let sub = dir.join("src");
    let output = Command::new(binary())
        .current_dir(&sub)
        .arg("--validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "--validate should discover the config by walking up to .git: {}",
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
        .args(["--config", cfg.to_str().unwrap(), "--include", "tables"])
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
        .args(["--config", cfg.to_str().unwrap(), "--include", "tables"])
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
        .args(["--config", cfg.to_str().unwrap(), "--include", "tables"])
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
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "--include",
            "tables",
            "--dry-run",
        ])
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
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "--include",
            "lints",
            "--dry-run",
        ])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    // `lints` (read-only, with --dry-run) has no post-process pass, so `false`
    // never runs and the only possible failure is error-severity diagnostics.
    // We assert the binary did not fail *because of post_process* by checking
    // stderr has no "post_process" mention.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("post_process"),
        "lints must not invoke post_process: {stderr:?}"
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
        "exclude_files:\n  - \"lib.rs\"\npost_process:\n  - command: \"false\"\n    extensions: [\"rs\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--include", "tables"])
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

// ── New tests: include/exclude modes, --include/--exclude flags ──

/// include + exclude both present -> config-load error.
#[test]
fn include_and_exclude_xor_errors() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("lib.rs"), "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "include:\n  - rules: [tables]\nexclude:\n  - rules: [reorder]\n",
    )
    .unwrap();
    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--validate"])
        .output()
        .expect("failed to spawn");
    assert!(
        !output.status.success(),
        "include + exclude must hard-fail at config load: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Whitelist mode: a file matching NO include group runs nothing.
#[test]
fn include_whitelist_runs_nothing_for_unmatched_file() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    // Sibling file under `other/` so the include pattern matches at least
    // one file (preserved semantic check); the target `lib.rs` is outside it.
    fs::create_dir_all(dir.join("other")).unwrap();
    fs::write(dir.join("other").join("lib.rs"), "fn matched() {}\n").unwrap();
    let tmp = dir.join("lib.rs");
    // Bare `pub` that vis would narrow and lints would flag - neither runs.
    fs::write(&tmp, "pub fn undocumented() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    // Whitelist: only run `vis, lints` (ops that would affect a .rs file),
    // and crucially NOT on this path.
    fs::write(
        &cfg,
        "include:\n  - paths: [\"other/**/*.rs\"]\n    rules: [vis, lints]\n",
    )
    .unwrap();
    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(
        output.status.success(),
        "unmatched file in whitelist mode must run nothing (no lint failure): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = fs::read_to_string(&tmp).unwrap();
    assert_eq!(
        actual, "pub fn undocumented() {}\n",
        "file must be unchanged"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Blacklist mode: exclude: [{rules: [lints]}] suppresses all lint failure.
#[test]
fn exclude_lints_op_suppresses_all_lint_failure() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // DOC001 + DOC002 would both fire on a normal run.
    fs::write(&tmp, "pub fn load() -> Result<(), String> { Ok(()) }\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "exclude:\n  - rules: [\"lints\"]\n").unwrap();
    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(
        output.status.success(),
        "lints op disabled -> run must succeed despite doc gaps: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// --include override: only run `vis`, no lint failure on an undocumented fn.
#[test]
fn include_flag_overrides_config_mode() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // Bare pub fn -> vis would narrow; lints would error (DOC001). --include vis
    // must override the default mode so lints does NOT run.
    fs::write(&tmp, "pub(crate) mod m {\n    pub fn f() {}\n}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    // Config is blacklist mode (lints on); --include must override it.
    fs::write(&cfg, "exclude:\n  - rules: [vis]\n").unwrap();
    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--include", "vis"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(
        output.status.success(),
        "--include vis must override config and skip lints: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = fs::read_to_string(&tmp).unwrap();
    assert!(
        actual.contains("pub(crate) fn f"),
        "vis must narrow: {actual}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Whitelist a single lint code: the lint pass runs scoped to DOC001 only,
/// no other ops/lints run, and the file is unmutated.
#[test]
fn include_single_lint_code_runs_only_that_code() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // DOC001 (missing doc on a public item) fires on each bare pub fn.
    fs::write(&tmp, "pub fn one() {}\npub fn two() {}\n").unwrap();
    let output = Command::new(binary())
        .args(["--no-config", "--include", "DOC001"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(
        !output.status.success(),
        "--include DOC001 must surface DOC001 diagnostics: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // No fix/reorder/vis in the whitelist -> file untouched.
    let actual = fs::read_to_string(&tmp).unwrap();
    assert_eq!(
        actual, "pub fn one() {}\npub fn two() {}\n",
        "file must be unmutated by the DOC001-only lint pass"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// --exclude additive: even with no config, --exclude lints skips lint failure.
#[test]
fn exclude_flag_additive_skips_lints() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn undocumented() {}\n").unwrap();
    let output = Command::new(binary())
        .args(["--no-config", "--exclude", "lints"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(
        output.status.success(),
        "--exclude lints must skip the lint pass: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// --include / --exclude with an unknown op errors.
#[test]
fn flags_reject_unknown_op() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let output = Command::new(binary())
        .args(["--no-config", "--include", "BOGUS"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(!output.status.success(), "--include BOGUS must error");

    let output = Command::new(binary())
        .args(["--no-config", "--exclude", "BOGUS"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(!output.status.success(), "--exclude BOGUS must error");
    let _ = fs::remove_dir_all(&dir);
}

/// --include + --exclude combine in whitelist mode: --include vis,lints then
/// --exclude lints yields enabled={vis}. vis narrows the inner fn, but lints
/// does NOT run, so the bare `pub fn f` that would trigger DOC001 stays clean.
#[test]
fn include_and_exclude_cli_combine_in_whitelist_mode() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // vis would narrow `pub fn f`; lints/DOC001 would normally flag it.
    fs::write(&tmp, "pub(crate) mod m {\n    pub fn f() {}\n}\n").unwrap();
    let output = Command::new(binary())
        .args([
            "--no-config",
            "--include",
            "vis",
            "--include",
            "lints",
            "--exclude",
            "lints",
        ])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(
        output.status.success(),
        "whitelist {{vis,lints}} - {{lints}} = {{vis}}: vis runs, lints skipped (no DOC001): {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = fs::read_to_string(&tmp).unwrap();
    assert!(
        actual.contains("pub(crate) fn f"),
        "vis must still narrow despite --exclude lints: {actual}"
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
