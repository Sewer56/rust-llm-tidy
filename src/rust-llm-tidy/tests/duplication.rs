//! Public DUP001 participation, scope, suppression, and non-mutation contracts.

use rstest::rstest;
use rust_llm_tidy::config::{DuplicationConfig, load_and_compile};
use rust_llm_tidy::{RunOptions, SourceOptions, run, tidy_source};
use std::fs;

const BLOCK: &str = "load();\nclassify();\nrecord();\nflush();\nfinish();\n";

#[test]
fn run_should_allow_cli_source_extensions_without_transformations() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.extra");
    let source = BLOCK.repeat(DuplicationConfig::default().min_occurrences);
    fs::write(&path, &source).unwrap();
    let options = RunOptions {
        paths: vec![path.clone()],
        extensions: vec!["extra".into()],
        all_lines: true,
        apply: true,
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    report.ensure_success().unwrap();
    assert_eq!(report.files[0].diagnostics.len(), 1);
    assert!(!report.files[0].processed);
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[rstest]
#[case::disabled_code("exclude: [{rules: [DUP001]}]", 0)]
#[case::disabled_group("exclude: [{rules: [lints]}]", 0)]
#[case::excluded_file("exclude_files: [input.js]", 0)]
#[case::unselected("include: [{rules: [TEXT001]}]", 0)]
#[case::defaults("{}", 1)]
#[case::custom_threshold("duplication: {min_occurrences: 4}", 0)]
fn run_should_honor_duplication_policy(#[case] yaml: &str, #[case] count: usize) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.js");
    let config_path = directory.path().join("config.yml");
    fs::write(
        &path,
        BLOCK.repeat(DuplicationConfig::default().min_occurrences),
    )
    .unwrap();
    fs::write(&config_path, yaml).unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path],
        all_lines: true,
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();

    report.ensure_success().unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .flat_map(|file| &file.diagnostics)
            .filter(|d| d.code == "DUP001")
            .count(),
        count
    );
}

#[rstest]
#[case::rust("rs", "", true, 1)]
#[case::csharp("cs", "", true, 1)]
#[case::python("py", "", true, 1)]
#[case::backendless("js", "", true, 1)]
#[case::configured_extra("custom", "extra_extensions: [custom]\n", true, 1)]
#[case::configured_replacement("custom", "extensions: [custom]\n", true, 1)]
#[case::uppercase("CUSTOM", "extra_extensions: [custom]\n", true, 1)]
#[case::prose("md", "", true, 0)]
#[case::configured_prose("txt", "extra_extensions: [txt]\n", true, 0)]
#[case::data("json", "extra_extensions: [json]\n", true, 0)]
#[case::configuration("yaml", "", true, 0)]
#[case::unknown("custom", "", true, 0)]
#[case::config_scope("rs", "lint_scopes: {DUP001: all}\n", false, 1)]
#[case::changed_without_permission("rs", "", false, 0)]
fn run_should_limit_duplication_to_eligible_sources_and_scopes(
    #[case] extension: &str,
    #[case] yaml: &str,
    #[case] all_lines: bool,
    #[case] count: usize,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("input.{extension}"));
    let config_path = directory.path().join("config.yml");
    let source = BLOCK.repeat(DuplicationConfig::default().min_occurrences);
    fs::write(&path, &source).unwrap();
    fs::write(&config_path, if yaml.is_empty() { "{}" } else { yaml }).unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path.clone()],
        include: vec!["DUP001".into()],
        all_lines,
        apply: true,
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();

    report.ensure_success().unwrap();
    assert_eq!(
        report
            .files
            .iter()
            .map(|file| file.diagnostics.len())
            .sum::<usize>(),
        count
    );
    assert_eq!(fs::read_to_string(path).unwrap(), source);
    assert!(
        report
            .files
            .iter()
            .all(|file| file.changes.is_empty() && !file.processed)
    );
    if extension == "rs" && !all_lines && yaml.is_empty() {
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("Git reads were not granted"));
    }
}

#[test]
fn run_should_not_count_copies_in_other_files() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..DuplicationConfig::default().min_occurrences)
        .map(|index| {
            let path = directory.path().join(format!("input{index}.js"));
            fs::write(&path, BLOCK).unwrap();
            path
        })
        .collect();
    let options = RunOptions {
        paths,
        all_lines: true,
        include: vec!["DUP001".into()],
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    assert!(report.files.iter().all(|file| file.diagnostics.is_empty()));
}

#[test]
fn run_should_preserve_declaration_suppression() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let config_path = directory.path().join("config.yml");
    fs::write(&path, format!("fn classify() {{\n{}}}\n", BLOCK.repeat(3))).unwrap();
    fs::write(&config_path, "symbol_rules: [{symbol: classify, action: exclude, target: declaration, exclude_lints: [DUP001]}]").unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path],
        all_lines: true,
        include: vec!["DUP001".into()],
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();

    report.ensure_success().unwrap();
    assert!(report.files[0].diagnostics.is_empty());
}

#[rstest]
#[case::no_git(false)]
#[case::git_without_repository(true)]
fn run_should_warn_and_skip_without_a_baseline(#[case] git_changed: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.js");
    fs::write(
        &path,
        BLOCK.repeat(DuplicationConfig::default().min_occurrences),
    )
    .unwrap();
    let options = RunOptions {
        paths: vec![path],
        git_changed,
        include: vec!["DUP001".into()],
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    report.ensure_success().unwrap();
    assert!(report.files[0].diagnostics.is_empty());
    assert_eq!(report.warnings.len(), 1);
}

#[test]
fn source_and_file_should_render_identical_all_lines_findings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let source = format!(
        "fn classify() {{\n{}}}\n",
        BLOCK.repeat(DuplicationConfig::default().min_occurrences)
    );
    fs::write(&path, &source).unwrap();
    let buffer_options = SourceOptions {
        include: vec!["DUP001".into()],
        all_lines: true,
        ..SourceOptions::default()
    };
    let file_options = RunOptions {
        paths: vec![path.clone()],
        include: buffer_options.include.clone(),
        all_lines: true,
        ..RunOptions::default()
    };

    let buffer = tidy_source(&source, "rs", &buffer_options).unwrap();
    let file = run(&file_options, None).unwrap();

    assert_eq!(buffer.diagnostics.len(), 1);
    assert_eq!(
        buffer
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        file.files[0]
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    assert_eq!(buffer.source, fs::read_to_string(path).unwrap());
}

#[test]
fn source_should_warn_and_skip_duplication_without_query_authority() {
    let source = BLOCK.repeat(DuplicationConfig::default().min_occurrences);
    let options = SourceOptions {
        include: vec!["DUP001".into()],
        ..SourceOptions::default()
    };

    let report = tidy_source(&source, "rs", &options).unwrap();

    assert!(report.diagnostics.is_empty());
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("no input diff"));
}
