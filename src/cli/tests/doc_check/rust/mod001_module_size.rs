//! MOD001 module-size behavior specific to Rust sources.
//!
//! Covers `#[cfg(test)]` region exclusion, threshold movement, and rule
//! selection through both CLI flags and configuration. The cross-language
//! MOD001 suite lives in `crate::mod001_module_size`.

use crate::common::binary;
use crate::{run_command, temp_dir, temp_file};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Lines inside the `#[cfg(test)]` mod region never count: 553 physical
/// lines with 450 counted stay silent.
#[test]
fn mod001_should_exclude_cfg_test_mod_lines_from_the_count() {
    let path = temp_file("rs");
    fs::write(
        &path,
        format!(
            "{}#[cfg(test)]\nmod tests {{\n{}\n}}\n",
            mod001_module_source(450),
            "    fn helper() {{}}\n".repeat(100)
        ),
    )
    .unwrap();

    let output = run_command(&["--include", "MOD001"], &path);
    let _ = fs::remove_file(&path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && !stderr.contains("MOD001"),
        "the test region must leave the file under the budget:\n{stderr}"
    );
}

/// `--include MOD001` whitelists the code alone: the finding fires and
/// every other code stays off.
#[test]
fn mod001_should_fire_alone_when_included_by_code() {
    let path = temp_file("rs");
    fs::write(
        &path,
        format!("{}pub fn undocumented() {{}}\n", mod001_module_source(501)),
    )
    .unwrap();

    let output = run_command(&["--include", "MOD001"], &path);
    let _ = fs::remove_file(&path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "MOD001 warnings must not fail the run: {stderr}"
    );
    assert_eq!(
        stderr.matches("MOD001").count(),
        1,
        "the code whitelist must accept and run MOD001:\n{stderr}"
    );
    assert!(
        !stderr.contains("DOC001"),
        "the code-only whitelist must suppress every other code:\n{stderr}"
    );
}

/// A configured `module_size.max_lines` moves the firing point: 350
/// non-test lines fire under a 300 budget and stay silent under the 500
/// default.
#[test]
fn mod001_should_follow_the_configured_max_lines_threshold() {
    let (dir, file) = mod001_module_with_config(350, "module_size:\n  max_lines: 300\n");

    let configured = Command::new(binary())
        .arg("--config")
        .arg(dir.join(".rust-llm-tidy.yml"))
        .args(["--include", "MOD001"])
        .arg(&file)
        .output()
        .unwrap();

    let configured_stderr = String::from_utf8_lossy(&configured.stderr);
    assert!(
        configured.status.success(),
        "MOD001 warnings must not fail the run: {configured_stderr}"
    );
    assert!(
        configured_stderr.contains(":301: warning[MOD001]"),
        "the 300 budget must move the firing point:\n{configured_stderr}"
    );

    let default = run_command(&["--include", "MOD001"], &file);
    let _ = fs::remove_dir_all(&dir);
    let default_stderr = String::from_utf8_lossy(&default.stderr);
    assert!(
        default.status.success() && !default_stderr.contains("MOD001"),
        "350 lines stay under the 500 default:\n{default_stderr}"
    );
}

/// Files under a `tests` directory never fire MOD001 while the other
/// codes still lint them.
#[test]
fn mod001_should_skip_files_under_a_tests_directory_while_other_codes_run() {
    let dir = temp_dir();
    let tests_dir = dir.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();
    // Over-cap module plus an undocumented pub fn: MOD001 must stay silent
    // while DOC001 still fires.
    fs::write(
        tests_dir.join("over.rs"),
        format!("{}pub fn undocumented() {{}}\n", mod001_module_source(501)),
    )
    .unwrap();

    let output = run_command(&["--include", "MOD001", "--include", "DOC001"], &tests_dir);
    let _ = fs::remove_dir_all(&dir);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("MOD001"),
        "no MOD001 under a tests/ directory:\n{stderr}"
    );
    assert!(
        stderr.contains("DOC001"),
        "other codes must still lint tests/ files:\n{stderr}"
    );
}

/// Exactly at the default budget: silent, because strictly greater fires.
#[test]
fn mod001_should_stay_silent_at_exactly_the_default_budget() {
    let path = temp_file("rs");
    fs::write(&path, mod001_module_source(500)).unwrap();

    let output = run_command(&["--include", "MOD001"], &path);
    let _ = fs::remove_file(&path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stderr.is_empty(),
        "500 lines is at the budget, not over it:\n{stderr}"
    );
}

/// `--exclude MOD001` suppresses the finding like any other code.
#[test]
fn mod001_should_suppress_when_excluded_by_code() {
    let path = temp_file("rs");
    fs::write(&path, mod001_module_source(501)).unwrap();

    let output = run_command(&["--include", "MOD001", "--exclude", "MOD001"], &path);
    let _ = fs::remove_file(&path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && !stderr.contains("MOD001"),
        "--exclude MOD001 must suppress the finding:\n{stderr}"
    );
}

/// Over the default budget: one warning at the first line past the budget,
/// and warnings never fail the run.
#[test]
fn mod001_should_warn_when_non_test_lines_exceed_the_default_budget() {
    let path = temp_file("rs");
    fs::write(&path, mod001_module_source(501)).unwrap();

    let output = run_command(&["--include", "MOD001"], &path);
    let _ = fs::remove_file(&path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "MOD001 warnings must not fail the run: {stderr}"
    );
    assert_eq!(
        stderr.matches("MOD001").count(),
        1,
        "exactly one MOD001 finding per file:\n{stderr}"
    );
    assert!(
        stderr.contains(":501: warning[MOD001]"),
        "MOD001 must report at the first line past the budget:\n{stderr}"
    );
}

// ── MOD001 helpers ────────────────────────────────────────────────

/// Write `yaml` beside a fresh `over.rs` of `count` private fn lines and
/// return `(dir, file)`; the config governs runs against `file`.
fn mod001_module_with_config(count: usize, yaml: &str) -> (PathBuf, PathBuf) {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join("over.rs");
    fs::write(&file, mod001_module_source(count)).unwrap();
    fs::write(dir.join(".rust-llm-tidy.yml"), yaml).unwrap();
    (dir, file)
}

/// Generate `count` private fn lines with zero-padded names.
fn mod001_module_source(count: usize) -> String {
    (0..count)
        .map(|i| format!("fn filler_{i:03}() {{}}\n"))
        .collect()
}
