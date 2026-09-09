//! TEXT009 configuration, extraction, and diagnostic output through the CLI.

use super::{common::binary, temp_dir};
use rstest::rstest;
use std::{fs, process::Command};

/// Custom guidance survives extraction and reaches JSON with an error status.
#[rstest]
#[case::markdown("md", "Hello!", 1)]
#[case::rust_raw_string("rs", "fn main() { let _ = r#\"\n// Hello!\n\"#; }\n// Real!", 1)]
#[case::rust("rs", "// Hello!\nfn main() { let _ = \"Hello!\"; }", 1)]
#[case::python("py", "# Hello!\nx = 'Hello!'", 1)]
#[case::yaml("yaml", "# Hello!\nx: 'Hello!'", 1)]
#[case::csharp("cs", "/// Hello!\nclass Sample {}", 1)]
fn cli_should_emit_custom_guidance(#[case] ext: &str, #[case] source: &str, #[case] count: usize) {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let path = dir.join(format!("source.{ext}"));
    fs::write(&path, source).unwrap();
    let config = dir.join(".rust-llm-tidy.yml");
    fs::write(&config, "forbidden_characters:\n  - characters: ['!']\n    title: Be calm\n    message: Use a full stop.\n").unwrap();

    let output = Command::new(binary())
        .args([
            "--checks-only",
            "--include",
            "TEXT009",
            "--json",
            "--config",
        ])
        .arg(config)
        .arg(&path)
        .output()
        .unwrap();
    let records: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert!(!output.status.success());
    assert_eq!(
        records.len(),
        count,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(records[0]["title"], "Be calm");
    let expected_line = if source.starts_with("fn main()") {
        4
    } else {
        1
    };
    assert_eq!(records[0]["line"], expected_line);
    assert_eq!(records[0]["code"], "TEXT009");
    assert_eq!(records[0]["severity"], "error");
    assert_eq!(
        records[0]["message"],
        "forbidden character '!' (U+0021).\nUse a full stop."
    );
    assert_eq!(fs::read_to_string(path).unwrap(), source);
    fs::remove_dir_all(dir).unwrap();
}

/// Replacement policies and malformed entries are resolved before processing.
#[rstest]
#[case::default("{}", "Read\u{2014}this", true)]
#[case::disabled("forbidden_characters: []", "Read\u{2014}this", false)]
#[case::replacement(
    "forbidden_characters: [{characters: ['!'], title: Calm, message: Stop}]",
    "Read\u{2014}this",
    false
)]
#[case::duplicate(
    "forbidden_characters: [{characters: ['!', '!'], title: Calm, message: Stop}]",
    "",
    true
)]
#[case::empty(
    "forbidden_characters: [{characters: [], title: Calm, message: Stop}]",
    "",
    true
)]
#[case::blank_title(
    "forbidden_characters: [{characters: ['!'], title: ' ', message: Stop}]",
    "",
    true
)]
#[case::blank_message(
    "forbidden_characters: [{characters: ['!'], title: Calm, message: ' '}]",
    "",
    true
)]
#[case::missing("forbidden_characters: [{characters: ['!'], title: Calm}]", "", true)]
#[case::sequence(
    "forbidden_characters: [{characters: ['ab'], title: Calm, message: Stop}]",
    "",
    true
)]
fn cli_should_resolve_character_policy(
    #[case] yaml: &str,
    #[case] source: &str,
    #[case] fails: bool,
) {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let path = dir.join("source.md");
    let config = dir.join(".rust-llm-tidy.yml");
    fs::write(&path, source).unwrap();
    fs::write(&config, yaml).unwrap();

    let output = Command::new(binary())
        .args(["--checks-only", "--include", "TEXT009", "--config"])
        .arg(config)
        .arg(&path)
        .output()
        .unwrap();

    assert_eq!(
        !output.status.success(),
        fails,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(path).unwrap(), source);
    fs::remove_dir_all(dir).unwrap();
}
