//! Public library behavior for source buffers and controlled file execution.

use rust_llm_tidy::{RunOptions, SourceOptions, config, run, tidy_source};
use std::borrow::Cow;
use std::fs;

// Construction and core behavior.

#[test]
fn run_should_preserve_license_documents_when_selecting_files_or_directories() {
    let directory = tempfile::tempdir().unwrap();
    let prose = "Read [guide](https://example.com).\n";
    let source = "pub fn load() {}\n";
    let licenses = [
        "LICENSE",
        "LiCeNsE.MD",
        "LICENCE-MIT.txt",
        "COPYING_notice.md",
    ];
    let controls = ["licenses.md", "copyingcat.txt", "guide.md"];
    let mut inputs = Vec::new();
    for name in licenses
        .iter()
        .chain(&controls)
        .chain(["license.rs"].iter())
    {
        inputs.push(directory.path().join(name));
    }

    for explicit in [false, true] {
        for path in &inputs {
            fs::write(
                path,
                if path.ends_with("license.rs") {
                    source
                } else {
                    prose
                },
            )
            .unwrap();
        }
        let options = RunOptions {
            paths: if explicit {
                inputs.clone()
            } else {
                vec![directory.path().into()]
            },
            include: vec!["links".into(), "DOC001".into()],
            apply: true,
            ..RunOptions::default()
        };

        let report = run(&options, None).unwrap();

        assert_eq!(report.files.len(), controls.len() + 1);
        for name in licenses {
            assert_eq!(
                fs::read_to_string(directory.path().join(name)).unwrap(),
                prose
            );
        }
        for name in controls {
            assert_ne!(
                fs::read_to_string(directory.path().join(name)).unwrap(),
                prose
            );
        }
        let rust = report
            .files
            .iter()
            .find(|file| file.path.ends_with("license.rs"))
            .unwrap();
        assert!(rust.processed);
        assert_eq!(rust.diagnostics[0].code, "DOC001");
    }
}

#[test]
fn run_should_process_nothing_with_default_options() {
    let report = run(&RunOptions::default(), None).unwrap();

    assert!(report.files.is_empty());
    assert!(report.warnings.is_empty());
    assert!(report.post_process_failures.is_empty());
    report.ensure_success().unwrap();
}

#[test]
fn run_should_require_explicit_write_and_subprocess_permissions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("guide.md");
    let config_path = directory.path().join(".rust-llm-tidy.yml");
    let missing_command = directory.path().join("nonexistent-postprocessor");
    fs::write(
        &config_path,
        format!(
            "post_process:\n  - command: '{}'\n",
            missing_command.display()
        ),
    )
    .unwrap();
    let compiled = config::load_and_compile(&config_path).unwrap();
    let source = "Read [guide](https://example.com).\n";

    for (apply, post_process, expect_failure) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        fs::write(&path, source).unwrap();
        let options = RunOptions {
            paths: vec![path.clone()],
            include: vec!["links".into()],
            apply,
            post_process,
            ..RunOptions::default()
        };

        let report = run(&options, Some(&compiled)).unwrap();
        let after = fs::read_to_string(&path).unwrap();

        assert_eq!(after != source, apply);
        assert_eq!(!report.post_process_failures.is_empty(), expect_failure);
        assert_eq!(report.ensure_success().is_err(), expect_failure);
        assert_eq!(report.files[0].changes.len(), 1);
        if expect_failure {
            assert!(report.post_process_failures[0].spawn_failed);
        }
    }
}

#[test]
fn run_should_retain_successful_results_when_another_file_cannot_be_read() {
    let directory = tempfile::tempdir().unwrap();
    let good = directory.path().join("good.rs");
    let bad = directory.path().join("bad.rs");
    fs::write(&good, "pub fn load() {}\n").unwrap();
    fs::write(&bad, [0xff]).unwrap();
    let options = RunOptions {
        paths: vec![good.clone(), bad.clone()],
        include: vec!["DOC001".into()],
        ..RunOptions::default()
    };

    let report = run(&options, None).unwrap();

    assert_eq!(report.files.len(), 2);
    assert!(
        report
            .files
            .iter()
            .find(|f| f.path == bad)
            .unwrap()
            .failure
            .is_some()
    );
    assert_eq!(
        report
            .files
            .iter()
            .find(|f| f.path == good)
            .unwrap()
            .diagnostics[0]
            .code,
        "DOC001"
    );
    assert_eq!(report.error_count(), 1);
    assert!(report.ensure_success().is_err());
}

#[test]
fn source_should_borrow_unchanged_output_when_rules_are_excluded() {
    let directory = tempfile::tempdir().unwrap();
    for (source, extension, rule) in [
        ("pub fn load() {}\n", "rs", "DOC001"),
        ("Read [guide](https://example.com).\n", "md", "links"),
        (
            "fn helper() {}\n\npub fn load() { helper(); }\n",
            "rs",
            "reorder",
        ),
    ] {
        let path = directory.path().join(format!("{rule}.{extension}"));
        fs::write(&path, source).unwrap();
        let mut options = SourceOptions {
            include: vec![rule.into()],
            ..SourceOptions::default()
        };
        let control = tidy_source(source, extension, &options).unwrap();
        assert!(
            !control.changes.is_empty() || !control.diagnostics.is_empty(),
            "{rule}"
        );
        options.exclude.push(rule.into());

        let report = tidy_source(source, extension, &options).unwrap();
        let file = run(
            &RunOptions {
                paths: vec![path.clone()],
                apply: true,
                include: options.include.clone(),
                exclude: options.exclude.clone(),
                ..RunOptions::default()
            },
            None,
        )
        .unwrap();

        assert!(matches!(report.source, Cow::Borrowed(_)), "{rule}");
        assert_eq!(report.source, source);
        assert!(report.changes.is_empty());
        assert!(report.diagnostics.is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), report.source);
        assert_eq!(file.files[0].changes, report.changes);
        assert_eq!(file.files[0].diagnostics, report.diagnostics);
    }
}

#[test]
fn source_should_match_applied_file_results_for_standalone_operations() {
    let directory = tempfile::tempdir().unwrap();
    let fixtures = [
        ("links", "md", "Read [guide](https://example.com).\n"),
        (
            "tables",
            "md",
            "| a | longer |\n| --- | --- |\n| value | b |\n",
        ),
        (
            "reorder",
            "rs",
            "fn helper() {}\n\nfn main() { helper(); }\n",
        ),
        ("lints", "cs", "class C { public void Load() {} }\n"),
    ];

    for (operation, extension, source) in fixtures {
        let path = directory.path().join(format!("{operation}.{extension}"));
        fs::write(&path, source).unwrap();
        let selection = vec![operation.to_string()];
        let source_options = SourceOptions {
            include: selection.clone(),
            ..SourceOptions::default()
        };
        let file_options = RunOptions {
            paths: vec![path.clone()],
            apply: true,
            include: selection,
            ..RunOptions::default()
        };

        let buffer = tidy_source(source, extension, &source_options).unwrap();
        let files = run(&file_options, None).unwrap();
        let consumed = fs::read_to_string(path).unwrap();

        assert_eq!(consumed, buffer.source, "{operation}: final source");
        assert_eq!(
            files.files[0].changes, buffer.changes,
            "{operation}: changes"
        );
        assert_eq!(
            files.files[0].diagnostics, buffer.diagnostics,
            "{operation}: findings"
        );
    }
}

// Failures and execution permissions.

#[test]
fn source_should_reject_invalid_options() {
    let cases = [
        (
            "unknown_rule",
            "rs",
            SourceOptions {
                include: vec!["UNKNOWN".into()],
                ..SourceOptions::default()
            },
        ),
        ("invalid_extension", ".rs", SourceOptions::default()),
        (
            "zero_threshold",
            "md",
            SourceOptions {
                links_min_occurrences: 0,
                ..SourceOptions::default()
            },
        ),
    ];

    for (name, extension, options) in cases {
        let result = tidy_source("", extension, &options);

        assert!(result.is_err(), "{name}");
    }
}

#[test]
fn source_should_report_final_positions_when_reordering_moves_findings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let source = "fn helper() {}\n\npub fn load() { helper(); }\n";
    fs::write(&path, source).unwrap();
    let options = SourceOptions {
        include: vec!["reorder".into(), "DOC001".into()],
        ..SourceOptions::default()
    };
    let mut file_options = RunOptions {
        paths: vec![path.clone()],
        include: options.include.clone(),
        ..RunOptions::default()
    };

    let preview = run(&file_options, None).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    let buffer = tidy_source(source, "rs", &options).unwrap();
    file_options.apply = true;
    let applied = run(&file_options, None).unwrap();

    assert_eq!(preview.files[0].diagnostics[0].line, 3);
    assert!(!buffer.changes.is_empty());
    assert_eq!(buffer.diagnostics[0].line, 1);
    assert_eq!(
        buffer.source.lines().next(),
        Some("pub fn load() { helper(); }")
    );
    assert_eq!(fs::read_to_string(path).unwrap(), buffer.source);
    assert_eq!(applied.files[0].diagnostics, buffer.diagnostics);
    assert_eq!(applied.files[0].changes, buffer.changes);
}

#[test]
fn source_should_report_language_specific_findings() {
    for (source, extension, code) in [
        ("pub fn load() {}\n", "rs", "DOC001"),
        ("class C { public void Load() {} }", "cs", "DOC001"),
        (
            "A prose line that runs far beyond the eighty character budget for documentation lines.",
            "md",
            "TEXT002",
        ),
    ] {
        let options = SourceOptions {
            include: vec![code.into()],
            ..SourceOptions::default()
        };

        let report = tidy_source(source, extension, &options).unwrap();

        assert!(matches!(report.source, Cow::Borrowed(_)), "{extension}");
        assert_eq!(report.source, source);
        assert!(report.changes.is_empty());
        assert!(!report.diagnostics.is_empty(), "{extension}");
        assert!(report.diagnostics.iter().all(|d| d.code == code));
    }
}

#[test]
fn source_should_suppress_python_findings_when_syntax_is_invalid() {
    let prose = "# filler words pad the paragraph past the two hundred forty limit\n".repeat(5);
    let options = SourceOptions {
        include: vec!["lints".into()],
        ..SourceOptions::default()
    };
    let control = tidy_source(&prose, "py", &options).unwrap();
    assert!(!control.diagnostics.is_empty());

    for broken in ["def broken(:\n    pass\n", "def broken(\n"] {
        let source = format!("{broken}{prose}");
        let backend = rust_llm_tidy::languages::backend_for("py").unwrap();
        let parsed = backend.parse(&source).unwrap();

        let report = tidy_source(&source, "py", &options).unwrap();

        assert!(parsed.syntax_tree().root_node().has_error());
        assert!(backend.lint(&parsed).is_empty());
        assert!(report.diagnostics.is_empty());
    }
}
