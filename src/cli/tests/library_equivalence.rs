//! CLI and library consumers observe the same final source and JSON records.

use rust_llm_tidy::{RunOptions, run};
use serde_json::{Value, json};
use std::fs;
use std::process::Command;

mod common;

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
