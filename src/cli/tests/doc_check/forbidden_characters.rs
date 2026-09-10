//! TEXT009 configuration, extraction, and diagnostic output through the CLI.

use super::{common::binary, temp_dir};
use rstest::rstest;
use std::{fs, process::Command};

/// Additions compose with either defaults or an explicit replacement.
#[rstest]
#[case::defaults("", "Read\u{2014}this!", 2)]
#[case::empty("forbidden_characters: []\n", "Read\u{2014}this!", 1)]
#[case::replacement(
    "forbidden_characters: [{characters: ['?'], title: Ask, message: Rewrite}]\n",
    "Read\u{2014}this!?",
    2
)]
fn cli_should_add_entries_to_effective_policy(
    #[case] base: &str,
    #[case] source: &str,
    #[case] count: usize,
) {
    let yaml = format!(
        "{base}extra_forbidden_characters: [{{characters: ['!'], title: Calm, message: Rewrite}}]"
    );

    let (success, records, stderr) = character_records(source, "md", &yaml);

    assert!(!success, "{stderr}");
    assert_eq!(records.len(), count, "{stderr}");
}

/// Full comment bodies retain original lines and prose exclusions.
#[rstest]
#[case::rust_block("rs", "/*\n * Prose!\n * `Code!`\n */\nfn main() {}", &[2])]
#[case::rust_same_line("rs", "/* First! */ fn example() {} /* Second! */", &[1, 1])]
#[case::rust_fence("rs", "// ```text\n// Code!\n// ```\n// Prose!\nfn main() {}", &[4])]
#[case::csharp_block("cs", "/*\n * Prose!\n * `Code!`\n */\nclass Sample {}", &[2])]
#[case::csharp_raw("cs", "class Sample { string Value = \"\"\"\n// string!\n\"\"\"; } // Prose!", &[3])]
#[case::python_triple("py", "value = \"\"\"\n# string!\n\"\"\"\n# Prose!", &[4])]
#[case::sql("sql", "/* Prose! */\nSELECT 'string!'; -- Tail!", &[1, 2])]
#[case::lua("lua", "--[[\nProse!\n]]\n-- Tail!", &[2, 4])]
#[case::haskell("hs", "{- Prose! -}\n-- Tail!", &[1, 2])]
#[case::scheme("scm", "#| Prose! |#\n; Tail!", &[1, 2])]
#[case::matlab("m", "%{\nProse!\n%}\n% Tail!", &[2, 4])]
#[case::ruby("rb", "value = 'string!' # Prose!", &[1])]
#[case::shell("sh", "echo 'string!' # Prose!", &[1])]
#[case::rust_invalid("rs", "fn { // Prose!", &[])]
#[case::javascript_ambiguous("js", "/* open!", &[])]
fn cli_should_check_complete_comment_prose(
    #[case] ext: &str,
    #[case] source: &str,
    #[case] lines: &[u64],
) {
    let yaml = "forbidden_characters: [{characters: ['!'], title: Review, message: Rewrite, scope: {docs: false}}]";

    let (success, records, stderr) = character_records(source, ext, yaml);

    let actual: Vec<_> = records
        .iter()
        .map(|record| record["line"].as_u64().unwrap())
        .collect();
    assert_eq!(actual, lines, "{stderr}");
    assert_eq!(success, lines.is_empty(), "{stderr}");
}

/// Custom guidance survives extraction and reaches JSON with an error status.
#[rstest]
#[case::markdown("md", "Hello!", 1)]
#[case::rust_raw_string("rs", "fn main() { let _ = r#\"\n// Hello!\n\"#; }\n// Real!", 1)]
#[case::rust("rs", "// Hello!\nfn main() { let _ = \"Hello!\"; }", 1)]
#[case::python("py", "# Hello!\nx = 'Hello!'", 1)]
#[case::yaml("yaml", "# Hello!\nx: 'Hello!'", 1)]
#[case::csharp("cs", "/// Hello!\nclass Sample {}", 1)]
#[case::rust_trailing("rs", "fn main() {} // Hello!", 1)]
#[case::rust_block("rs", "/* Hello! */ fn main() {}", 1)]
#[case::rust_nested("rs", "/* Hello! /* nested */ */ fn main() {}", 1)]
#[case::csharp_trailing("cs", "class Sample {} // Hello!", 1)]
#[case::csharp_block("cs", "/* Hello! */ class Sample {}", 1)]
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
    let expected_line = if source.contains("r#\"") { 4 } else { 1 };
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

/// Conflicting entries and unknown scope keys fail configuration loading.
#[rstest]
#[case::default_overlap(
    "extra_forbidden_characters: [{characters: ['\u{2014}'], title: Review, message: Rewrite}]",
    "extra_forbidden_characters[0]"
)]
#[case::entry_overlap(
    "forbidden_characters: [{characters: ['!'], title: A, message: A}, {characters: ['!'], title: B, message: B, scope: {docs: false}}]",
    "forbidden_characters[1]"
)]
#[case::disabled_duplicate(
    "forbidden_characters: [{characters: ['!', '!'], title: A, message: A, scope: {docs: false, comments: false}}]",
    "duplicate character"
)]
#[case::unknown_scope(
    "forbidden_characters: [{characters: ['!'], title: A, message: A, scope: {doc: true}}]",
    "unknown field"
)]
fn cli_should_reject_ambiguous_character_policy(#[case] yaml: &str, #[case] error: &str) {
    let (success, _, stderr) = character_records("", "md", yaml);

    assert!(!success);
    assert!(stderr.contains(error), "{stderr}");
}

/// Each punctuation category is enabled without configuration.
#[rstest]
#[case::en_dash('\u{2013}')]
#[case::em_dash('\u{2014}')]
#[case::left_single('\u{2018}')]
#[case::right_single('\u{2019}')]
#[case::left_double('\u{201C}')]
#[case::right_double('\u{201D}')]
#[case::ellipsis('\u{2026}')]
fn cli_should_reject_default_punctuation(#[case] character: char) {
    let (success, records, stderr) = character_records(&format!("Read{character}this"), "md", "{}");

    assert!(!success, "{stderr}");
    assert_eq!(records.len(), 1);
    assert!(
        records[0]["message"]
            .as_str()
            .unwrap()
            .contains(&format!("U+{:04X}", character as u32))
    );
}

/// Equivalent list composition produces identical rendered JSON diagnostics.
#[test]
fn cli_should_render_equal_diagnostics_for_equivalent_policies() {
    let source = "/// Doc!\nfn example() {} // Comment?";
    let combined = "forbidden_characters: [{characters: ['!'], title: Docs, message: Explain, scope: {comments: false}}, {characters: ['?'], title: Comments, message: Explain, scope: {docs: false}}]";
    let additive = "forbidden_characters: [{characters: ['!'], title: Docs, message: Explain, scope: {comments: false}}]\nextra_forbidden_characters: [{characters: ['?'], title: Comments, message: Explain, scope: {docs: false}}]";

    let (_, mut left, _) = character_records(source, "rs", combined);
    let (_, mut right, _) = character_records(source, "rs", additive);
    // Fixture paths differ; all diagnostic payload fields must agree.
    for record in left.iter_mut().chain(right.iter_mut()) {
        record.as_object_mut().unwrap().remove("path");
        record.as_object_mut().unwrap().remove("file");
    }

    assert_eq!(left.len(), 2);
    assert_eq!(left, right);
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

/// Entries choose documentation and ordinary comments independently.
#[rstest]
#[case::rust("rs", "//! Doc!\n/* Comment! */\nfn main() {} // Trailing!", 1, 2)]
#[case::rust_inner_block("rs", "/*! Doc! */\n/* Comment! */", 1, 1)]
#[case::rust_attribute("rs", "#[doc = \"Doc!\"]\nfn example() {} // Comment!", 1, 1)]
#[case::csharp("cs", "/// <summary>Doc!</summary>\nclass Sample {} // Comment!", 1, 1)]
#[case::python("py", "\"\"\"Doc!\"\"\"\nvalue = 'string!' # Comment!", 1, 1)]
#[case::markdown("md", "Doc!", 1, 0)]
#[case::javascript("js", "/* Comment! */\nconst value = 'string!'; // Trailing!", 0, 2)]
#[case::yaml("yaml", "value: 'string!' # Comment!", 0, 1)]
fn cli_should_select_entry_scope(
    #[case] ext: &str,
    #[case] source: &str,
    #[case] doc_count: usize,
    #[case] comment_count: usize,
    #[values(false, true)] docs: bool,
    #[values(false, true)] comments: bool,
) {
    let yaml = format!(
        "forbidden_characters:\n  - characters: ['!']\n    title: Review\n    message: Rewrite\n    scope: {{docs: {docs}, comments: {comments}}}"
    );

    let (success, records, stderr) = character_records(source, ext, &yaml);

    let expected = usize::from(docs) * doc_count + usize::from(comments) * comment_count;
    assert_eq!(records.len(), expected, "{stderr}");
    assert_eq!(success, expected == 0, "{stderr}");
}

/// Disjoint entries retain distinct guidance for the same character.
#[test]
fn cli_should_use_category_guidance_when_character_scopes_are_disjoint() {
    let yaml = "forbidden_characters:\n  - {characters: ['!'], title: Docs, message: Document, scope: {comments: false}}\n  - {characters: ['!'], title: Comments, message: Explain, scope: {docs: false}}";

    let (_, records, stderr) =
        character_records("/// Doc!\nfn example() {} // Comment!", "rs", yaml);

    assert_eq!(records.len(), 2, "{stderr}");
    assert_eq!(records[0]["title"], "Docs");
    assert_eq!(records[1]["title"], "Comments");
}

/// Run the real CLI with an isolated configuration and preserve its JSON output.
fn character_records(
    source: &str,
    ext: &str,
    yaml: &str,
) -> (bool, Vec<serde_json::Value>, String) {
    let dir = temp_dir();
    fs::create_dir(&dir).unwrap();
    let path = dir.join(format!("source.{ext}"));
    let config = dir.join(".rust-llm-tidy.yml");
    fs::write(&path, source).unwrap();
    fs::write(&config, yaml).unwrap();

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
    let records = serde_json::from_slice(&output.stdout).unwrap_or_default();

    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    fs::remove_dir_all(dir).unwrap();
    (
        output.status.success(),
        records,
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}
