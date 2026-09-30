//! MOD005 reports a lone `mod.rs` without moving the file or failing the CLI.

use super::common::binary;
use rstest::rstest;
use std::fs;
use std::process::Command;

/// File, directory, and bare-filename inputs all reach the filesystem lint.
#[rstest]
#[case::file("banana/mod.rs", false)]
#[case::directory("banana", false)]
#[case::bare_filename("mod.rs", true)]
fn cli_should_report_hint_without_edits_when_mod_is_alone(
    #[case] input: &str,
    #[case] inside_module: bool,
) {
    // Arrange.
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("banana");
    fs::create_dir(&directory).unwrap();
    let path = directory.join("mod.rs");
    let source = "//! Banana.\nfn peel() {}\n";
    fs::write(&path, source).unwrap();
    let config = root.path().join("config.yml");
    fs::write(&config, "{}\n").unwrap();
    let cwd = if inside_module {
        &directory
    } else {
        root.path()
    };

    // Act.
    let output = Command::new(binary())
        .current_dir(cwd)
        .arg("--config")
        .arg(&config)
        .args(["--include", "MOD005", input])
        .output()
        .unwrap();

    // Assert.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains(
            "mod.rs:1: hint[MOD005]: directory `banana/` contains only `mod.rs`.\n\n\
             Why: This directory adds a navigation step without grouping other files.\n\n\
             Suggestions:\n\
             - Consider moving `banana/mod.rs` to `banana.rs` and removing the empty directory.\n\
             - Check relative `include!`, `include_str!`, `include_bytes!`, and `#[path]` paths,\n  \
               and explicit references to the old location. Preserve behavior; do not overwrite an existing file. (file)"
        ),
        "{stderr}"
    );
    assert_eq!(stderr.matches("hint[MOD005]").count(), 1);
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert!(!root.path().join("banana.rs").exists());
}
