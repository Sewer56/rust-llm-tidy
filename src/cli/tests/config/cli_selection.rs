//! CLI flag selection tests: how `--include`/`--exclude` combine, override
//! config modes, reject unknown ops, and shape the default pipeline.

use super::common::binary;
use super::temp_dir;
use std::fs;
use std::process::Command;

/// Default pipeline with `exclude: [reorder]` fixes/vis/lints but does
/// not reorder.
///
/// The input is reordered on a normal run; under `reorder` being disabled
/// it must remain in input order.
#[test]
fn all_excludes_reorder_rule() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // Two top-level fns in NON-canonical order: callee precedes caller, so a
    // normal run would reorder them. Canonical is caller before callee, per
    // the reorder phase.
    fs::write(&tmp, "fn callee() {}\nfn caller() { callee(); }\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "exclude:\n  - paths: [\"lib.rs\"]\n    rules: [\"reorder\"]\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .args(["--exclude", "DOC009"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "pipeline should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Default pipeline runs fix/reorder/vis/lints.
    //
    // With `reorder` disabled, the non-canonical input order (callee before caller)
    // must be preserved.
    // Without the disable, it would reorder to caller-before-callee.
    let actual = fs::read_to_string(&tmp).unwrap();
    assert!(
        actual.find("fn callee()").unwrap() < actual.find("fn caller()").unwrap(),
        "reorder disabled: non-canonical callee-before-caller must be preserved"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Default pipeline with `exclude: [DOC001]` suppresses DOC001 findings.
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

/// --exclude lints suppresses all lint codes, including specifically included ones.
#[test]
fn exclude_lints_overrides_included_lint_code() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn undocumented() {}\n").unwrap();
    let output = Command::new(binary())
        .args(["--no-config", "--include", "DOC001", "--exclude", "lints"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(
        output.status.success(),
        "--exclude lints must suppress included DOC001: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("DOC001"));
    let _ = fs::remove_dir_all(&dir);
}

/// Default pipeline with `exclude_files` for the fixture leaves the file unchanged.
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
        "default pipeline should succeed even when the file is excluded: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let actual = fs::read_to_string(&tmp).unwrap();
    assert_eq!(actual, original, "excluded file must be unchanged");
    let _ = fs::remove_dir_all(&dir);
}

// ── exclude + exclude_rules on pipeline operations ──

/// Bare default pipeline with `exclude: [links]` does NOT hoist links on a
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
        !output.status.success(),
        "dry-run must fail for proposed table changes: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Dry-run reports change records on stderr, never reconstructed source.
    assert!(
        output.stdout.is_empty(),
        "dry-run must not print reconstructed source to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("hoist link"),
        "links must NOT be hoisted when `links` is disabled: {stderr:?}"
    );
    // Tables are still applied (the `links` disable is selective, not blanket).
    assert!(
        stderr.contains("tables were aligned"),
        "tables must still be applied when only `links` is disabled: {stderr:?}"
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
/// --exclude lints yields enabled={vis}.
///
/// vis narrows the inner fn, but lints does NOT run, so the bare `pub fn f`
/// that would trigger DOC001 stays clean.
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

// ── New tests: include/exclude modes, --include/--exclude flags ──

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

/// --include lints with --exclude DOC001 keeps DOC001 disabled in whitelist mode.
#[test]
fn include_lints_exclude_lint_code() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn undocumented() {}\n").unwrap();
    let output = Command::new(binary())
        .args(["--no-config", "--include", "lints", "--exclude", "DOC001"])
        .args(["--exclude", "DOC009"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    assert!(
        output.status.success(),
        "--exclude DOC001 must suppress DOC001 with --include lints: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("DOC001"));
    let _ = fs::remove_dir_all(&dir);
}

/// Whitelist a single lint code: the lint pass runs scoped to DOC001 only,
/// no other ops/lints run, and the file is unmutated.
#[test]
fn include_single_lint_code_runs_only_that_code() {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    // Undocumented Result-returning pub fn triggers DOC001 and DOC002.
    fs::write(&tmp, "pub fn load() -> Result<(), String> { Ok(()) }\n").unwrap();
    let output = Command::new(binary())
        .args(["--no-config", "--include", "DOC001"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "--include DOC001 must surface DOC001 diagnostics: {}",
        stderr
    );
    assert!(
        stderr.contains("DOC001"),
        "--include DOC001 must report DOC001: {stderr:?}"
    );
    assert!(
        !stderr.contains("DOC002"),
        "--include DOC001 must not report DOC002: {stderr:?}"
    );
    // No fix/reorder/vis in the whitelist -> file untouched.
    let actual = fs::read_to_string(&tmp).unwrap();
    assert_eq!(
        actual, "pub fn load() -> Result<(), String> { Ok(()) }\n",
        "file must be unmutated by the DOC001-only lint pass"
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
