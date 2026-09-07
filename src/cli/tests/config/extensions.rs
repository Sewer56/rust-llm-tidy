//! Extension selection tests: the `extensions:` and `extra_extensions:`
//! keys - malformed-entry rejection, list replacement, and rule
//! composition.

use super::common::binary;
use super::temp_dir;
use std::fs;
use std::process::Command;

/// Malformed entries in either extension key fail validation with a
/// non-zero exit and an error naming the value.
#[test]
fn extension_keys_reject_malformed_entries() {
    for yaml in [
        "extensions: [\".log\"]\n",
        "extensions: [\"\"]\n",
        "extensions: [\"notes.log\"]\n",
        "extensions: [\"dir/log\"]\n",
        "extra_extensions: [\".log\"]\n",
        "extra_extensions: [\"\"]\n",
        "extra_extensions: [\"notes.log\"]\n",
        "extra_extensions: [\"dir/log\"]\n",
    ] {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join(".rust-llm-tidy.yml");
        fs::write(&cfg, yaml).unwrap();

        let output = Command::new(binary())
            .args(["--config", cfg.to_str().unwrap(), "--validate"])
            .output()
            .expect("failed to spawn rust-llm-tidy");
        assert!(
            !output.status.success(),
            "malformed entry in `{yaml}` must fail validation"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid extension"),
            "stderr should name the invalid extension: {stderr}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

/// `extensions:` composes with `include` whitelist mode: the replaced-base
/// `.py` file runs only the whitelisted op that its profile also allows.
#[test]
fn extensions_key_composes_with_include_whitelist() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    // Omitted `paths` implies every path.
    fs::write(
        &cfg,
        "extensions: [\"py\"]\ninclude:\n  - rules: [\"fences\"]\n",
    )
    .unwrap();
    let py = dir.join("x.py");
    fs::write(
        &py,
        "# | a | b |\n# | --- | --- |\n# | 1 | 22 |\n\n# ```text\n# ```rust\n# inner\n# ```\n# ```\n",
    )
    .unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .arg(&py)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "run with extensions + include should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = fs::read_to_string(&py).unwrap();
    assert!(
        after.contains("# ~~~rust"),
        "the whitelisted fences op must run: {after}"
    );
    assert!(
        after.contains("# | a | b |\n# | --- | --- |\n# | 1 | 22 |"),
        "tables must not run outside the whitelist: {after}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A non-empty `extensions:` list replaces the default list: only the
/// listed extension is processed, and `--validate` accepts the key.
#[test]
fn extensions_key_replaces_default_extensions() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, "extensions: [\"txt\"]\n").unwrap();
    let txt = dir.join("notes.txt");
    fs::write(&txt, "| a | b |\n| --- | --- |\n| 1 | 22 |\n").unwrap();
    let md = dir.join("doc.md");
    fs::write(&md, "| a | b |\n| --- | --- |\n| 1 | 22 |\n").unwrap();

    let validate = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--validate"])
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        validate.status.success(),
        "--validate must accept the extensions key: {}",
        String::from_utf8_lossy(&validate.stderr)
    );

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .arg(&dir)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "run with extensions key should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("notes.txt"),
        "the listed .txt file must be allowed and processed: {stderr}"
    );
    assert!(
        !stderr.contains("doc.md"),
        "the dropped default .md extension must not be processed: {stderr}"
    );
    assert_eq!(
        stderr.matches("tables were aligned").count(),
        1,
        "only the .txt table must be aligned: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `extra_extensions:` composes with `exclude` groups.
///
/// The allowed `.py` file whose path matches the group keeps its tables
/// disabled while a sibling still gets the default table fix.
#[test]
fn extra_extensions_compose_with_exclude_rules() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        "extra_extensions: [\"py\"]\nexclude:\n  - paths: [\"x.py\"]\n    rules: [\"tables\"]\n",
    )
    .unwrap();
    let excluded = dir.join("x.py");
    let excluded_original = "# | a | b |\n# | --- | --- |\n# | 1 | 22 |\n";
    fs::write(&excluded, excluded_original).unwrap();
    let sibling = dir.join("y.py");
    fs::write(&sibling, "# | a | b |\n# | --- | --- |\n# | 1 | 22 |\n").unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap()])
        .arg(&dir)
        .output()
        .expect("failed to spawn rust-llm-tidy");
    assert!(
        output.status.success(),
        "run with extensions + exclude should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(&excluded).unwrap(),
        excluded_original,
        "tables must stay disabled for the excluded path"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("y.py") && stderr.contains("tables were aligned"),
        "the sibling must still get its default table fix: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Unknown-extension selection cannot authorize table fixes; Markdown still fixes.
#[rstest::rstest]
#[case::extra_extension("extra_extensions: [log]\n")]
#[case::custom_extensions("extensions: [log, md]\n")]
fn tables_should_preserve_unknown_extensions_when_selected(#[case] yaml: &str) {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(&cfg, yaml).unwrap();

    let source = "| a | b |\n| --- | --- |\n| 1 | 22 |\n";
    let log = dir.join("notes.log");
    fs::write(&log, source).unwrap();
    let md = dir.join("doc.md");
    fs::write(&md, source).unwrap();

    let output = Command::new(binary())
        .args(["--config", cfg.to_str().unwrap(), "--include", "tables"])
        .arg(&dir)
        .output()
        .expect("failed to spawn rust-llm-tidy");

    assert!(
        output.status.success(),
        "run with extension selection should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("notes.log"),
        "the unknown .log file must have no table change record: {stderr}"
    );
    assert!(
        stderr.contains("doc.md"),
        "the default extensions must keep working: {stderr}"
    );
    assert_eq!(
        stderr.matches("tables were aligned").count(),
        1,
        "only the Markdown table must be aligned: {stderr}"
    );
    assert_eq!(fs::read_to_string(&log).unwrap(), source);
    assert_ne!(fs::read_to_string(&md).unwrap(), source);

    let _ = fs::remove_dir_all(&dir);
}
