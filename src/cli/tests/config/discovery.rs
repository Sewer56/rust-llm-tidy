//! Config discovery tests: walking up to the `.git` root, anchoring globs
//! at the config file's directory, and accepting valid `module_size`
//! settings.

use super::common::binary;
use super::temp_dir;
use std::fs;
use std::process::Command;

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

/// A valid or absent `module_size.max_lines` keeps the default pipeline
/// running cleanly.
#[rstest::rstest]
#[case::configured_threshold("module_size:\n  max_lines: 300\n")]
#[case::absent_section("exclude_files: []\n")]
#[case::neither("module_size:\n  include_non_code: false\n  include_in_file_tests: false\n")]
#[case::non_code_only("module_size:\n  include_non_code: true\n  include_in_file_tests: false\n")]
#[case::inline_tests_only(
    "module_size:\n  include_non_code: false\n  include_in_file_tests: true\n"
)]
#[case::both("module_size:\n  include_non_code: true\n  include_in_file_tests: true\n")]
#[case::test_files_enabled("module_size:\n  include_test_files: true\n")]
#[case::test_files_disabled("module_size:\n  include_test_files: false\n")]
fn cli_should_accept_module_size_when_threshold_is_valid_or_absent(#[case] yaml: &str) {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let tmp = dir.join("lib.rs");
    fs::write(&tmp, "fn example() {}\n").unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, yaml).unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .args(["--exclude", "DOC009"])
        .arg(&tmp)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "{yaml}: {}",
        String::from_utf8_lossy(&output.stderr)
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
    // to the config dir.

    // The config dir is canonicalized, so the relative path must resolve
    // against it.
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
