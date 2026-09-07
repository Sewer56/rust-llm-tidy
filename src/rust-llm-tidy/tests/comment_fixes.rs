//! Public text fixes rewrite standalone comment runs, never neighboring source.

use rstest::rstest;
use rust_llm_tidy::reporting::{Change, ChangeKind};
use rust_llm_tidy::{RunOptions, SourceOptions, config, run, tidy_source};
use std::borrow::Cow;
use std::fs;

const ALIGNED: &str = "| a    | b |\n| ---- | - |\n| long | c |\n";
const FENCE: &str = "```text\n```rust\nλ\n```\n```\n";
const FIXED_FENCE: &str = "```text\n~~~rust\nλ\n~~~\n```\n";
const LINK: &str = "[A](http://x)";
const LINK_THRESHOLD: usize = 2;
const TABLE: &str = "| a | b |\n|---|---|\n| long | c |\n";

#[rstest]
#[case::marker("/// ", "")]
#[case::indentation("  //! ", "")]
#[case::code("//! ", "fn f() {}\n")]
fn fences_should_keep_ownership_separate_when_comment_runs_change(
    #[case] next_prefix: &str,
    #[case] separator: &str,
) {
    let source = format!(
        "//! ```text\n{separator}{}",
        comment("```rust\nλ\n```\n", next_prefix)
    );

    assert_equivalent(&source, "rs", &["fences"], 1, &source);
}

// Source boundaries and preservation.

#[rstest]
#[case::rust("rs", "\t/// ", "const TEXT: &str = \"é\";\r\n")]
#[case::csharp("cs", "  // ", "class Café {}\r\n")]
#[case::python("py", "\t# ", "text = 'é'\r\n")]
#[case::stub("pyi", "  # ", "text: str\r\n")]
fn fences_should_preserve_crlf_indentation_unicode_and_eof(
    #[case] extension: &str,
    #[case] prefix: &str,
    #[case] code: &str,
) {
    let source = format!(
        "{code}{}",
        comment(FENCE.trim_end(), prefix).replace('\n', "\r\n")
    );
    let expected = format!(
        "{code}{}",
        comment(FIXED_FENCE.trim_end(), prefix).replace('\n', "\r\n")
    );

    let changes = assert_equivalent(&source, extension, &["fences"], 1, &expected);

    assert_eq!(
        changes
            .iter()
            .map(|change| change.line.unwrap().get())
            .collect::<Vec<_>>(),
        [3, 5]
    );
}

#[rstest]
#[case::rust_slashes("rs", "//// ", "fn code() {}\n")]
#[case::rust_bangs("rs", "//!! ", "fn code() {}\n")]
#[case::csharp_slashes("cs", "//// ", "class C {}\n")]
#[case::python_hash("py", "## ", "x = 1\n")]
fn fixes_should_borrow_source_when_marker_runs_exceed_the_profile_marker(
    #[case] extension: &str,
    #[case] prefix: &str,
    #[case] code: &str,
) {
    let source = format!(
        "{code}{}",
        comment(&format!("{TABLE}{FENCE}{LINK}\n"), prefix)
    );

    assert_equivalent(
        &source,
        extension,
        &["tables", "fences", "links"],
        1,
        &source,
    );
}

#[rstest]
#[case::rust_block("rs", "/*\n", "*/\n", "/// ")]
#[case::rust_doc_block("rs", "/**\n", "*/\nfn f() {}\n", "/// ")]
#[case::csharp_block("cs", "/*\n", "*/\nclass C {}\n", "// ")]
#[case::doc_attribute("rs", "#[doc = r#\"\n", "\"#]\nfn f() {}\n", "/// ")]
#[case::python_docstring("py", "\"\"\"\n", "\"\"\"\n", "# ")]
#[case::stub_docstring("pyi", "\"\"\"\n", "\"\"\"\n", "# ")]
fn fixes_should_borrow_source_when_prose_is_not_a_line_comment(
    #[case] extension: &str,
    #[case] open: &str,
    #[case] close: &str,
    #[case] prefix: &str,
) {
    let source = format!(
        "{open}{}{close}",
        comment(&format!("{TABLE}{FENCE}{LINK}\n"), prefix)
    );

    assert_equivalent(
        &source,
        extension,
        &["tables", "fences", "links"],
        1,
        &source,
    );
}

#[rstest]
#[case::rust("rs", "fn broken( {\n", "/// ")]
#[case::csharp("cs", "class {\n", "// ")]
#[case::python("py", "def broken(:\n", "# ")]
#[case::stub("pyi", "def broken(:\n", "# ")]
fn fixes_should_borrow_source_when_syntax_errors_prevent_authorization(
    #[case] extension: &str,
    #[case] broken: &str,
    #[case] prefix: &str,
) {
    let source = format!(
        "{broken}{}{}{}\n",
        comment(TABLE, prefix),
        comment(FENCE, prefix),
        comment(LINK, prefix)
    );

    assert_equivalent(
        &source,
        extension,
        &["tables", "fences", "links"],
        1,
        &source,
    );
}

// Core transformations and adjacent literals.

#[rstest]
#[case::tables(TABLE, ALIGNED, "tables", ChangeKind::Table)]
#[case::fences(FENCE, FIXED_FENCE, "fences", ChangeKind::Fence)]
fn fixes_should_rewrite_comments_without_changing_adjacent_literals(
    #[case] before: &str,
    #[case] after: &str,
    #[case] rule: &str,
    #[case] kind: ChangeKind,
    #[values("rs", "cs", "py", "pyi")] extension: &str,
) {
    let (prefix, open, close) = match extension {
        "rs" => ("///", "const TEXT: &str = r#\"\n", "\"#;\n"),
        "cs" => ("//", "class C { string Text = @\"\n", "\"; }\n"),
        _ => ("#", "text = \"\"\"\n", "\"\"\"\n"),
    };
    let literal = format!("{open}{}{close}", comment(before, &format!("{prefix} ")));
    let source = format!("{literal}{}", comment(before, &format!("{prefix} ")));
    let rewritten = comment(after, &format!("{prefix} "));
    let expected = format!("{literal}{rewritten}");

    let changes = assert_equivalent(&source, extension, &[rule], 1, &expected);

    let expected_kinds = if kind == ChangeKind::Fence {
        vec![kind; 2]
    } else {
        vec![kind]
    };
    assert_eq!(
        changes.iter().map(|change| change.kind).collect::<Vec<_>>(),
        expected_kinds
    );
}

#[rstest]
#[case::marker("/// ", "")]
#[case::indentation("  //! ", "")]
#[case::code("//! ", "fn f() {}\n")]
fn links_should_count_occurrences_and_emit_definitions_per_run(
    #[case] next_prefix: &str,
    #[case] separator: &str,
) {
    let repeated = std::iter::repeat_n(LINK, LINK_THRESHOLD)
        .collect::<Vec<_>>()
        .join(" ");
    let references = std::iter::repeat_n("[A]", LINK_THRESHOLD)
        .collect::<Vec<_>>()
        .join(" ");
    let source = format!("//! {LINK}\n{separator}{next_prefix}{repeated}\n");
    let marker = next_prefix.trim_end();
    let expected = format!(
        "//! {LINK}\n{separator}{next_prefix}{references}\n{marker}\n{next_prefix}[A]: http://x\n"
    );

    let changes = assert_equivalent(&source, "rs", &["links"], LINK_THRESHOLD, &expected);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Link);
}

#[test]
fn links_should_rewrite_doc_comments_without_changing_adjacent_literals() {
    let literal = "const TEXT: &str = r#\"\n/// [A](http://x)\n\"#;\n";
    let source = format!("{literal}/// {LINK}\n");
    let expected = format!("{literal}/// [A]\n///\n/// [A]: http://x\n");

    let changes = assert_equivalent(&source, "rs", &["links"], 1, &expected);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Link);
}

#[rstest]
#[case::rust("rs", "fn f() {} /// | a | b |\n/// |---|---|\n/// | long | c |\n")]
#[case::csharp("cs", "class C {} // | a | b |\n// |---|---|\n// | long | c |\n")]
#[case::python("py", "x = 1 # | a | b |\n# |---|---|\n# | long | c |\n")]
#[case::stub("pyi", "x: int # | a | b |\n# |---|---|\n# | long | c |\n")]
fn tables_should_borrow_source_when_headers_are_inline_comments(
    #[case] extension: &str,
    #[case] source: &str,
) {
    assert_equivalent(source, extension, &["tables"], 1, source);
}

/// Two rewritten comment runs in one file still report a single table record.
#[test]
fn tables_should_report_one_change_when_multiple_comment_runs_are_rewritten() {
    let source = format!(
        "{}fn separate() {{}}\n{}",
        comment(TABLE, "/// "),
        comment(TABLE, "/// ")
    );
    let expected = format!(
        "{}fn separate() {{}}\n{}",
        comment(ALIGNED, "/// "),
        comment(ALIGNED, "/// ")
    );

    let changes = assert_equivalent(&source, "rs", &["tables"], 1, &expected);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Table);
}

/// Compare consumed file bytes and records with the buffer API and an exact oracle.
fn assert_equivalent(
    source: &str,
    extension: &str,
    rules: &[&str],
    threshold: usize,
    expected: &str,
) -> Vec<Change> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("input.{extension}"));
    let config_path = directory.path().join("config.yml");
    fs::write(&path, source).unwrap();
    fs::write(
        &config_path,
        format!("links:\n  min_occurrences: {threshold}\n"),
    )
    .unwrap();
    let compiled = config::load_and_compile(&config_path).unwrap();
    let options = SourceOptions {
        include: rules.iter().map(|rule| (*rule).into()).collect(),
        links_min_occurrences: threshold,
        ..SourceOptions::default()
    };
    let file_options = RunOptions {
        paths: vec![path.clone()],
        apply: true,
        include: options.include.clone(),
        ..RunOptions::default()
    };

    let buffer = tidy_source(source, extension, &options).unwrap();
    let files = run(&file_options, Some(&compiled)).unwrap();
    let consumed = fs::read(path).unwrap();

    files.ensure_success().unwrap();
    assert_eq!(files.files.len(), 1);
    assert_eq!(consumed, expected.as_bytes());
    assert_eq!(buffer.source.as_bytes(), consumed);
    assert_eq!(files.files[0].changes, buffer.changes);
    assert_eq!(files.files[0].diagnostics, buffer.diagnostics);
    if source == expected {
        assert!(matches!(buffer.source, Cow::Borrowed(_)));
        assert!(buffer.changes.is_empty());
    } else {
        assert!(!buffer.changes.is_empty());
    }
    buffer.changes
}

/// Render fixture prose as comment lines without changing its payload bytes.
fn comment(prose: &str, prefix: &str) -> String {
    prose
        .split_inclusive('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect()
}
