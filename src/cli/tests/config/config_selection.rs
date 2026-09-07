//! Config-file selection tests: `include:`/`exclude:` exclusivity and
//! whitelist behavior for files that match no group.

use super::common::binary;
use super::temp_dir;
use std::fs;
use std::process::Command;

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
