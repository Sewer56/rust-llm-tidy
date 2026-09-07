//! `--validate` tests: acceptance of valid configs and failure on
//! malformed YAML, unknown rules, non-matching paths, and invalid values.

use super::common::binary;
use super::temp_dir;
use std::fs;
use std::process::Command;

/// `--validate` accepts a config that sets `module_size.max_lines`.
#[test]
fn validate_accepts_module_size_max_lines() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "module_size:\n  max_lines: 500\n").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("--validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "--validate should accept the module_size key: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `--validate` exits non-zero when `links.min_occurrences` is below 1.
#[test]
fn validate_fails_on_links_min_occurrences_zero() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "links:\n  min_occurrences: 0\n").unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("--validate")
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        !output.status.success(),
        "--validate should fail on min_occurrences: 0"
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

/// `--validate` exits non-zero when no config file is found.
#[test]
fn validate_fails_when_no_config_found() {
    // Run --validate from a temp dir with no .rust-llm-tidy.yml and no .git;
    // pass neither --config nor --no-config.

    // discover walks to fs root without finding a config and returns None,
    // which `--validate` treats as failure.
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

/// `--validate --no-config` exits non-zero because there is no config to
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

// ── --validate ──

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

/// Non-boolean exclusion and suppression values fail config validation.
#[rstest::rstest]
#[case::quoted_true("\"true\"")]
#[case::quoted_false("\"false\"")]
#[case::numeric_zero("0")]
#[case::empty_sequence("[]")]
#[case::null("null")]
fn validation_should_reject_boolean_settings_when_value_is_not_boolean(
    #[case] value: &str,
    #[values(
        "exclude_license_documents",
        "passive_narration:\n  suppress_in_release_notes",
        "module_size:\n  include_non_code",
        "module_size:\n  include_in_file_tests",
        "module_size:\n  include_test_files"
    )]
    setting: &str,
) {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");

    let yaml = format!("{setting}: {value}\n");
    fs::write(&cfg, &yaml).unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("--validate")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{yaml}: {stderr}");
    assert!(stderr.contains("failed to parse YAML config"), "{stderr}");

    fs::remove_dir_all(dir).unwrap();
}

/// An invalid `module_size.max_lines` value fails `--validate` with a
/// non-zero exit naming the failure.
#[rstest::rstest]
#[case::zero("module_size:\n  max_lines: 0\n", "module_size.max_lines must be >= 1")]
#[case::negative("module_size:\n  max_lines: -1\n", "failed to parse YAML config")]
#[case::non_integer("module_size:\n  max_lines: many\n", "failed to parse YAML config")]
fn validation_should_reject_module_size_max_lines_when_value_is_invalid(
    #[case] yaml: &str,
    #[case] failure: &str,
) {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, yaml).unwrap();

    let output = Command::new(binary())
        .arg("--config")
        .arg(&cfg)
        .arg("--validate")
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{yaml}: {stderr}");
    assert!(stderr.contains(failure), "{yaml}: {stderr}");

    fs::remove_dir_all(dir).unwrap();
}
