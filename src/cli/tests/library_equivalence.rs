//! CLI and library consumers observe the same final source and JSON records.

use rust_llm_tidy::{RunOptions, run};
use serde_json::{Value, json};
use std::fs;
use std::process::Command;

mod common;

/// TEXT010 needs file context, so its equivalence fixture sits under `docs/`
/// and the explicit config keeps both paths on the same policy.
#[test]
fn cli_should_match_library_for_documentation_context() {
    let directory = tempfile::tempdir().unwrap();
    let docs = directory.path().join("docs");
    fs::create_dir_all(&docs).unwrap();
    let path = docs.join("setup.md");
    let source = "\n# Setup\n\nSteps.\n";
    fs::write(&path, source).unwrap();
    let config_path = directory.path().join("config.yml");
    fs::write(&config_path, "{}\n").unwrap();
    let config = rust_llm_tidy::config::load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path.clone()],
        include: vec!["TEXT010".into()],
        all_lines: true,
        apply: true,
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();
    let file = &report.files[0];
    let expected: Vec<Value> = file
        .diagnostics
        .iter()
        .map(|d| {
            json!({
                "path": path, "line": d.line,
                "severity": match d.severity {
                    rust_llm_tidy::reporting::Severity::Error => "error",
                    rust_llm_tidy::reporting::Severity::Warning => "warning",
                    rust_llm_tidy::reporting::Severity::Hint => "hint",
                    rust_llm_tidy::reporting::Severity::Reminder => "reminder",
                },
                "code": d.code, "message": d.message, "item_kind": d.item_kind,
                "item_name": d.item_name, "title": d.title(),
            })
        })
        .collect();

    let output = Command::new(common::binary())
        .current_dir(directory.path())
        .arg("--config")
        .arg(&config_path)
        .args(["--json", "--all-lines", "--include", "TEXT010"])
        .arg(&path)
        .output()
        .unwrap();
    let actual: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert_eq!(actual, expected);
    assert_eq!(output.status.success(), report.ensure_success().is_ok());
}

#[test]
fn cli_should_match_library_source_and_records() {
    let directory = tempfile::tempdir().unwrap();
    let fixtures = [
        ("links", "md", "Read [guide](https://example.com).\n"),
        (
            "reorder",
            "rs",
            "fn helper() {}\n\nfn main() { helper(); }\n",
        ),
        ("DOC001", "rs", "pub fn load() {}\n"),
        (
            "DOC002",
            "cs",
            "class C { public void Load() { throw new E(); } }\n",
        ),
    ];

    for (rule, extension, source) in fixtures {
        let path = directory.path().join(format!("input.{extension}"));
        fs::write(&path, source).unwrap();
        let options = RunOptions {
            paths: vec![path.clone()],
            include: vec![rule.into()],
            apply: true,
            ..RunOptions::default()
        };
        let report = run(&options, None).unwrap();
        let library_source = fs::read_to_string(&path).unwrap();
        let file = &report.files[0];
        let mut expected: Vec<Value> = file
            .diagnostics
            .iter()
            .map(|d| {
                json!({
                    "path": path, "line": d.line,
                    "severity": match d.severity {
                        rust_llm_tidy::reporting::Severity::Error => "error",
                        rust_llm_tidy::reporting::Severity::Warning => "warning",
                        rust_llm_tidy::reporting::Severity::Hint => "hint",
                        rust_llm_tidy::reporting::Severity::Reminder => "reminder",
                    },
                    "code": d.code, "message": d.message, "item_kind": d.item_kind,
                    "item_name": d.item_name, "title": d.title(),
                })
            })
            .collect();
        expected.extend(file.changes.iter().map(|c| {
            json!({
                "path": path, "line": c.line, "severity": "success", "code": c.code,
                "message": c.message, "item_kind": c.kind.as_str(), "item_name": c.name,
                "title": null,
            })
        }));
        fs::write(&path, source).unwrap();

        let output = Command::new(common::binary())
            .args(["--no-config", "--json", "--include", rule])
            .arg(&path)
            .output()
            .unwrap();
        let actual: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), library_source, "{rule}");
        assert_eq!(actual, expected, "{rule}");
        assert_eq!(
            output.status.success(),
            report.ensure_success().is_ok(),
            "{rule}"
        );
    }
}
