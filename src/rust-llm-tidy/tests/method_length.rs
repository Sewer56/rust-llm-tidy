//! LEN001 through the public `run` entry point.
//!
//! The rule dispatches at the config-aware `check_file` seam. The
//! default and configured thresholds and code selection must therefore
//! behave like every other lint through the real pipeline.

use rstest::rstest;
use rust_llm_tidy::{RunOptions, config, run};
use std::fs;

/// LEN001 fires through `run` exactly when the measured body lines
/// exceed the resolved threshold and the code is not excluded.
#[rstest]
#[case::default_threshold_fires_over(76, None, false, true)]
#[case::default_threshold_silent_at(75, None, false, false)]
#[case::tightened_threshold_fires(3, Some("method_length:\n  max_lines: 2\n"), false, true)]
#[case::loosened_threshold_silent(76, Some("method_length:\n  max_lines: 300\n"), false, false)]
#[case::excluded_by_code(76, None, true, false)]
fn run_should_honor_the_method_length_threshold(
    #[case] body_lines: usize,
    #[case] config_yaml: Option<&str>,
    #[case] exclude_len001: bool,
    #[case] fires: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sized.rs");
    fs::write(&path, sized_source(body_lines)).unwrap();
    let compiled = config_yaml.map(|yaml| {
        let config_path = directory.path().join(".rust-llm-tidy.yml");
        fs::write(&config_path, yaml).unwrap();
        config::load_and_compile(&config_path).unwrap()
    });
    let options = RunOptions {
        paths: vec![path],
        exclude: if exclude_len001 {
            vec!["LEN001".into()]
        } else {
            Vec::new()
        },
        ..RunOptions::default()
    };

    let report = run(&options, compiled.as_ref()).unwrap();

    let len001: Vec<_> = report.files[0]
        .diagnostics
        .iter()
        .filter(|d| d.code == "LEN001")
        .collect();
    assert_eq!(len001.len(), usize::from(fires));
    if fires {
        assert_eq!(len001[0].line, 1);
        assert_eq!(len001[0].item_name.as_deref(), Some("sized"));
    }
}

/// `--include LEN001` whitelists the rule alone: it fires at the default
/// threshold while no other lint runs.
#[test]
fn run_should_isolate_len001_when_included_by_code() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sized.rs");
    fs::write(&path, sized_source(76)).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["LEN001".into()],
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    let diagnostics = &report.files[0].diagnostics;
    assert_eq!(diagnostics.len(), 1, "only LEN001 runs in whitelist mode");
    assert_eq!(diagnostics[0].code, "LEN001");
}

/// `fn sized()` whose body holds exactly `body_lines` measured lines: one
/// statement per line, no blanks or comments.
fn sized_source(body_lines: usize) -> String {
    let body: String = (0..body_lines)
        .map(|i| format!("    let _v{i} = {i};\n"))
        .collect();
    format!("fn sized() {{\n{body}}}\n")
}
