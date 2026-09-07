//! `post_process:` step tests: extension gating, skipping under `--dry-run`
//! and read-only ops, excluded files, and exit-code propagation.

use super::common::binary;
use super::temp_dir;
use std::fs;
use std::process::Command;

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
        format!(
            "exclude_files:\n  - \"lib.rs\"\npost_process:\n  - {}\n    extensions: [\"rs\"]\n",
            post_process_command(1)
        ),
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

/// Return platform-specific command YAML for a command with requested exit code.
/// `true` and `false` are not available as standalone commands on Windows.
fn post_process_command(exit_code: u8) -> String {
    if cfg!(windows) {
        format!("command: \"cmd.exe\"\n    args: [\"/C\", \"exit\", \"{exit_code}\"]")
    } else {
        let command = if exit_code == 0 { "true" } else { "false" };
        format!("command: \"{command}\"")
    }
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
        format!(
            "post_process:\n  - {}\n    extensions: [\"rs\"]\n",
            post_process_command(1)
        ),
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

/// `post_process` does not run on `lints` (read-only).
#[test]
fn post_process_not_run_on_check() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        format!(
            "post_process:\n  - {}\n    extensions: [\"rs\"]\n",
            post_process_command(1)
        ),
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--include", "lints"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    // `lints` is read-only and has no post-process pass, so `false` never runs
    // and the only possible failure is error-severity diagnostics.
    //
    // We assert the binary did not fail *because of post_process* by checking
    // stderr has no "post_process" mention.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("post_process"),
        "lints must not invoke post_process: {stderr:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A `.md` file with only Rust-only ops enabled is not post-processed:
/// reorder/vis never mutate Markdown, so a failing step must not run on it.
#[test]
fn post_process_not_run_on_md_with_only_rust_ops() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("doc.md");
    fs::write(&tmp, "# Guide\n\nBody text.\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        format!(
            "post_process:\n  - {}\n    extensions: [\"md\"]\n",
            post_process_command(1)
        ),
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--include", "reorder"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "reorder never mutates .md, so post_process must not run on it: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

// ── post_process ──

/// A successful `post_process` step on a .rs file runs and exits 0.
#[test]
fn post_process_runs_on_matching_extension() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        format!(
            "post_process:\n  - {}\n    extensions: [\"rs\"]\n",
            post_process_command(0)
        ),
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--include", "tables"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "successful post_process should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
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
        format!(
            "post_process:\n  - {}\n    extensions: [\"rs\"]\n",
            post_process_command(1)
        ),
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

/// `post_process` with `extensions: [\"md\"]` does NOT run on a .rs file (no
/// failure reported).
#[test]
fn post_process_skips_non_matching_extension() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "pub fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    // A failing command would fail if invoked; restricting to .md means it must
    // not run.
    fs::write(
        &cfg,
        format!(
            "post_process:\n  - {}\n    extensions: [\"md\"]\n",
            post_process_command(1)
        ),
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
