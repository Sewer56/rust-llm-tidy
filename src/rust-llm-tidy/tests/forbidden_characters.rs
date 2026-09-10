//! TEXT009 default policy through file and buffer entry points.

use rstest::rstest;
use rust_llm_tidy::{RunOptions, SourceOptions, run, tidy_source};
use std::fs;

/// File and buffer consumers receive identical full-comment diagnostics.
#[rstest]
#[case::rust("/* Read\u{2014}this */ fn example() {} // Read\u{2014}that", "rs", &[1, 1])]
#[case::csharp("/* Read\u{2014}this */ class Sample {} // Read\u{2014}that", "cs", &[1, 1])]
#[case::python(
    "\"\"\"Read\u{2014}this\"\"\"\nx = 'Read\u{2014}that' # Read\u{2014}that",
    "py",
    &[1, 2]
)]
#[case::javascript("/* Read\u{2014}this */ const x = 'Read\u{2014}that';", "js", &[1])]
#[case::indented_rust(
    "mod example {\n    /**\n     * Read\u{2014}this\n     *     Code\u{2014}example\n     */\n    fn nested() {}\n}",
    "rs",
    &[3]
)]
#[case::indented_csharp(
    "class Sample {\n    /*\n     * Read\u{2014}this\n     *     Code\u{2014}example\n     */\n    void Nested() {}\n}",
    "cs",
    &[3]
)]
#[case::unstarred_rust("mod example {\n    /**\n     Read\u{2014}this\n         Code\u{2014}example\n     */\n    fn nested() {}\n}", "rs", &[3])]
#[case::unstarred_csharp("class Sample {\n    /*\n     Read\u{2014}this\n         Code\u{2014}example\n     */\n    void Nested() {}\n}", "cs", &[3])]
fn entrypoints_should_produce_equal_character_diagnostics(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] lines: &[usize],
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("input.{ext}"));
    fs::write(&path, source).unwrap();
    let options = RunOptions {
        paths: vec![path.clone()],
        include: vec!["TEXT009".into()],
        ..RunOptions::default()
    };
    let source_options = SourceOptions {
        include: vec!["TEXT009".into()],
        ..SourceOptions::default()
    };

    let file = run(&options, None).unwrap();
    let buffer = tidy_source(source, ext, &source_options).unwrap();

    assert_eq!(file.files[0].diagnostics, buffer.diagnostics);
    let actual: Vec<_> = buffer
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.line)
        .collect();
    assert_eq!(actual, lines);
    assert_eq!(buffer.source, source);
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}
