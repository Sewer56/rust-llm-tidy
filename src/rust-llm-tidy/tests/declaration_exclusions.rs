//! File and buffer acceptance coverage for independent declaration exclusions.

use rstest::rstest;
use rust_llm_tidy::config::load_and_compile;
use rust_llm_tidy::reporting::Diagnostic;
use rust_llm_tidy::{RunOptions, SourceOptions, run, tidy_source};
use std::fs;

#[rstest]
#[case::usage("symbol: custom")]
#[case::declaration("symbol: load, target: declaration")]
fn buffer_should_respect_symbol_hint_scope_overrides(#[case] matcher: &str) {
    let rules = format!("[{{{matcher}, title: Review, message: finding, scope: all}}]");
    let options = SourceOptions {
        symbol_rules: serde_yml::from_str(&rules).unwrap(),
        include: vec!["SYM".into()],
        ..SourceOptions::default()
    };

    let report = tidy_source("fn load() { custom(); }", "rs", &options).unwrap();

    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].message, "finding");
}

// Failure boundaries.

#[rstest]
#[case::protected(true)]
#[case::lint_only(false)]
fn entrypoints_should_apply_edit_control_to_visibility(#[case] exclude_edits: bool) {
    let source = "pub(crate) mod m {\n pub fn load() {}\n}\n";
    let rules = format!(
        "[{{symbol: load, target: declaration, action: exclude, \
         exclude_lints: true, exclude_edits: {exclude_edits}}}]"
    );

    let (output, diagnostics) = equivalent(source, "rs", &rules, &["vis", "DOC001"]);

    let expected = if exclude_edits {
        source
    } else {
        "pub(crate) mod m {\n pub(crate) fn load() {}\n}\n"
    };
    assert_eq!(output, expected);
    assert!(diagnostics.is_empty());
}

// Independent controls and final positions.

#[rstest]
#[case::defaults("", true, false)]
#[case::explicit_both("exclude_lints: true, exclude_edits: true", true, false)]
#[case::lint_only("exclude_lints: true, exclude_edits: false", true, true)]
#[case::edit_only("exclude_lints: false, exclude_edits: true", false, false)]
#[case::neither("exclude_lints: false, exclude_edits: false", false, true)]
#[case::default_lints("exclude_edits: false", true, true)]
#[case::default_edits("exclude_lints: false", false, false)]
#[case::selected_lint("exclude_lints: [DOC001], exclude_edits: false", true, true)]
#[case::other_lint("exclude_lints: [DOC004], exclude_edits: false", false, true)]
#[case::empty_list("exclude_lints: [], exclude_edits: false", false, true)]
fn entrypoints_should_apply_independent_controls_after_reordering(
    #[case] controls: &str,
    #[case] suppress: bool,
    #[case] reorder: bool,
) {
    let source = "fn helper() {}\n\npub fn load() { helper(); }\n";
    let rules = format!("[{{symbol: load, target: declaration, action: exclude, {controls}}}]");

    let (output, diagnostics) = equivalent(source, "rs", &rules, &["reorder", "DOC001"]);

    let expected = if reorder {
        "pub fn load() { helper(); }\n\nfn helper() {}\n"
    } else {
        source
    };
    assert_eq!(output, expected);
    assert_eq!(diagnostics.len(), usize::from(!suppress));
    if !suppress {
        assert_eq!(diagnostics[0].line, if reorder { 1 } else { 3 });
    }
}

#[rstest]
#[case::false_last(false)]
#[case::false_first(true)]
fn entrypoints_should_combine_overlapping_exclusions_additively(#[case] false_first: bool) {
    let source = "pub mod outer {\n    pub fn load(value: i32) {}\n}\n";
    let suppress = "{symbol: outer, target: declaration, action: exclude, exclude_lints: [DOC001], exclude_edits: false}, \
                    {symbol: load, target: declaration, action: exclude, exclude_lints: [DOC004], exclude_edits: false}";
    let allow = "{symbol: load, target: declaration, action: exclude, exclude_lints: false, exclude_edits: false}";
    let rules = if false_first {
        format!("[{allow}, {suppress}]")
    } else {
        format!("[{suppress}, {allow}]")
    };

    let (output, diagnostics) = equivalent(source, "rs", &rules, &["DOC001", "DOC004"]);

    assert_eq!(output, source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[rstest]
#[case::lint_only("exclude_lints: true, exclude_edits: false", "DOC001")]
#[case::edit_only("exclude_lints: false, exclude_edits: true", "reorder")]
#[case::defaults("", "tables")]
fn entrypoints_should_fail_closed_when_required_declarations_have_syntax_errors(
    #[case] controls: &str,
    #[case] operation: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let config_path = directory.path().join("config.yml");
    let source = "fn load( {\n";
    let rules = format!("[{{symbol: load, target: declaration, action: exclude, {controls}}}]");
    fs::write(&path, source).unwrap();
    fs::write(&config_path, format!("symbol_rules: {rules}")).unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = SourceOptions {
        symbol_rules: serde_yml::from_str(&rules).unwrap(),
        include: vec![operation.into()],
        ..SourceOptions::default()
    };
    let file_options = RunOptions {
        paths: vec![path.clone()],
        apply: true,
        include: options.include.clone(),
        ..RunOptions::default()
    };

    let buffer_error = tidy_source(source, "rs", &options).unwrap_err();
    let file = run(&file_options, Some(&config)).unwrap();

    assert!(format!("{buffer_error:#}").contains("syntax-error tree"));
    assert!(file.ensure_success().is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[rstest]
#[case::false_last(false)]
#[case::false_first(true)]
fn entrypoints_should_keep_edit_protection_when_another_rule_allows_edits(
    #[case] false_first: bool,
) {
    let source = "fn helper() {}\n\npub fn load() { helper(); }\n";
    let protect = "{symbol: load, target: declaration, action: exclude, exclude_lints: false}";
    let allow = "{symbol: load, target: declaration, action: exclude, exclude_lints: false, exclude_edits: false}";
    let rules = if false_first {
        format!("[{allow}, {protect}]")
    } else {
        format!("[{protect}, {allow}]")
    };

    let (output, diagnostics) = equivalent(source, "rs", &rules, &["reorder", "DOC001"]);

    assert_eq!(output, source);
    assert_eq!(diagnostics.len(), 1);
}

#[rstest]
#[case::rust("pub fn load(value: i32) {}\n", "rs", "load")]
#[case::csharp("public class C { public void Load(int value) {} }\n", "cs", "C")]
fn entrypoints_should_keep_other_findings_when_only_one_code_is_excluded(
    #[case] source: &str,
    #[case] ext: &str,
    #[case] symbol: &str,
) {
    let rules = format!(
        "[{{symbol: {symbol}, target: declaration, action: exclude, \
         exclude_lints: [DOC001], exclude_edits: false}}]"
    );

    let (output, diagnostics) = equivalent(source, ext, &rules, &["DOC001", "DOC004"]);

    assert_eq!(output, source);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code, "DOC004");
}

#[test]
fn entrypoints_should_not_suppress_siblings_at_old_declaration_positions() {
    let source = "pub fn helper() {}\n\npub fn load() { helper(); }\n";
    let rules = "[{symbol: load, target: declaration, action: exclude, \
                 exclude_lints: [DOC001], exclude_edits: false}]";

    let (output, diagnostics) = equivalent(source, "rs", rules, &["reorder", "DOC001"]);

    assert_eq!(
        output,
        "pub fn load() { helper(); }\n\npub fn helper() {}\n"
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].item_name.as_deref(), Some("helper"));
    assert_eq!(diagnostics[0].line, 3);
}

#[rstest]
#[case::protected(true)]
#[case::editable(false)]
fn entrypoints_should_protect_owned_doc_edits_independently_of_lints(#[case] exclude_edits: bool) {
    let source = "/// | A | B |\n/// |---|---|\n/// | long | x |\nfn load() {}\n";
    let rules = format!(
        "[{{symbol: load, target: declaration, action: exclude, \
         exclude_lints: true, exclude_edits: {exclude_edits}}}]"
    );

    let (output, diagnostics) = equivalent(source, "rs", &rules, &["tables", "TEXT002"]);

    assert_eq!(output == source, exclude_edits);
    assert!(diagnostics.is_empty());
}

#[rstest]
#[case::unknown("exclude_lints: [DOC999]")]
#[case::operation("exclude_lints: [reorder]")]
#[case::retired("exclude_lints: [DOC007]")]
fn entrypoints_should_reject_unregistered_exclusion_codes(#[case] controls: &str) {
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("config.yml");
    let rules = format!("[{{symbol: load, target: declaration, action: exclude, {controls}}}]");
    fs::write(&config_path, format!("symbol_rules: {rules}")).unwrap();
    let options = SourceOptions {
        symbol_rules: serde_yml::from_str(&rules).unwrap(),
        ..SourceOptions::default()
    };

    let file_error = load_and_compile(&config_path).unwrap_err();
    let buffer_error = tidy_source("fn load() {}", "rs", &options).unwrap_err();

    assert!(format!("{file_error:#}").contains("unknown lint code"));
    assert!(format!("{buffer_error:#}").contains("unknown lint code"));
}

#[rstest]
#[case::all("true", &[])]
#[case::none("false", &["SYM", "SYM", "SYM", "TEXT006"])]
#[case::text_only("[TEXT006]", &["SYM", "SYM", "SYM"])]
#[case::symbol_only("[SYM]", &["TEXT006"])]
#[case::both("[TEXT006, SYM]", &[])]
fn entrypoints_should_suppress_owned_docs_and_custom_and_builtin_hints(
    #[case] selection: &str,
    #[case] expected: &[&str],
) {
    let source = "/// Utilize the value.\npub fn load() { Vec::new(); custom(); }\n";
    let rules = format!(
        "[{{symbol: load, target: declaration, action: exclude, \
         exclude_lints: {selection}, exclude_edits: false}}, \
         {{symbol: custom, title: Call, message: finding}}, \
         {{regex: Utilize, title: Text, message: finding, comments: only}}]"
    );

    let (output, diagnostics) = equivalent(source, "rs", &rules, &["SYM", "TEXT006"]);

    assert_eq!(output, source);
    let mut codes: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    codes.sort_unstable();
    assert_eq!(codes, expected);
}

#[cfg(unix)]
#[rstest]
#[case::defaults("", false)]
#[case::edits_default_post_false("exclude_post_process: false", false)]
#[case::edits_true_post_default("exclude_edits: true", false)]
#[case::edits_false_post_default("exclude_edits: false", false)]
#[case::edits_true_post_false("exclude_edits: true, exclude_post_process: false", false)]
#[case::edits_false_post_false("exclude_edits: false, exclude_post_process: false", false)]
#[case::edits_default_post_true("exclude_post_process: true", true)]
#[case::edits_true_post_true("exclude_edits: true, exclude_post_process: true", true)]
#[case::edits_false_post_true("exclude_edits: false, exclude_post_process: true", true)]
#[case::post_only(
    "exclude_lints: false, exclude_edits: false, exclude_post_process: true",
    true
)]
#[case::no_match("symbol: other, exclude_post_process: true", false)]
#[case::other_language("languages: [csharp], exclude_post_process: true", false)]
#[case::other_extension("extensions: [cs], exclude_post_process: true", false)]
fn file_should_apply_postprocessing_opt_out_independently(
    #[case] controls: &str,
    #[case] skipped: bool,
) {
    let matcher = if controls.starts_with("symbol:") {
        ""
    } else {
        "symbol: load,"
    };
    let rules = format!("[{{{matcher} target: declaration, action: exclude, {controls}}}]");

    assert_postprocessing("fn load() {}\n", &rules, skipped, false, false);
}

#[cfg(unix)]
#[rstest]
#[case::false_first(true)]
#[case::false_last(false)]
fn file_should_combine_postprocessing_opt_outs_additively(#[case] false_first: bool) {
    let opt_out = "{symbol: load, target: declaration, action: exclude, exclude_edits: false, exclude_post_process: true}";
    let allow = "{symbol: load, target: declaration, action: exclude, exclude_post_process: false}";
    let rules = if false_first {
        format!("[{allow}, {opt_out}]")
    } else {
        format!("[{opt_out}, {allow}]")
    };

    assert_postprocessing("fn load() {}\n", &rules, true, false, false);
}

#[cfg(unix)]
#[rstest]
#[case::applicable("symbol: load, exclude_post_process: true", true)]
#[case::unmatched("symbol: other, exclude_post_process: true", true)]
#[case::disabled("symbol: load, exclude_post_process: false", false)]
#[case::default("symbol: load", false)]
#[case::other_language("symbol: load, languages: [csharp], exclude_post_process: true", false)]
fn file_should_fail_closed_when_postprocessing_exclusion_needs_valid_syntax(
    #[case] controls: &str,
    #[case] failed: bool,
) {
    let rules = format!(
        "[{{target: declaration, action: exclude, exclude_edits: false, \
         exclude_lints: false, {controls}}}]"
    );

    assert_postprocessing("fn load( {\n", &rules, false, failed, false);
}

#[cfg(unix)]
#[rstest]
#[case::allowed(false)]
#[case::excluded(true)]
fn file_should_report_subprocess_failure_only_when_postprocessing_runs(#[case] skipped: bool) {
    let rules = format!(
        "[{{symbol: load, target: declaration, action: exclude, exclude_post_process: {skipped}}}]"
    );

    assert_postprocessing("fn load() {}\n", &rules, skipped, false, true);
}

#[rstest]
#[case::all("true")]
#[case::selected("[DOC009, MOD001]")]
fn file_should_retain_file_level_findings_when_declarations_are_excluded(#[case] selection: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let config_path = directory.path().join("config.yml");
    fs::write(&path, "pub fn load() {\n}\n").unwrap();
    fs::write(
        &config_path,
        format!(
            "module_size: {{max_lines: 1}}\nsymbol_rules: [{{symbol: load, \
         target: declaration, action: exclude, exclude_lints: {selection}}}]"
        ),
    )
    .unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path],
        include: vec!["DOC009".into(), "MOD001".into()],
        all_lines: true,
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();

    let mut codes: Vec<_> = report.files[0].diagnostics.iter().map(|d| d.code).collect();
    codes.sort_unstable();
    assert_eq!(codes, ["DOC009", "MOD001"]);
}

/// Observe actual subprocess writes and failure reporting under one file policy.
#[cfg(unix)]
fn assert_postprocessing(
    source: &str,
    rules: &str,
    skipped: bool,
    failed: bool,
    command_fails: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.rs");
    let config_path = directory.path().join("config.yml");
    let exit_code = u8::from(command_fails);
    fs::write(&path, source).unwrap();
    fs::write(&config_path, format!(
        "symbol_rules: {rules}\n\
         post_process: [{{command: sh, args: ['-c', 'printf \"// processed\\n\" >> \"$1\"; exit {exit_code}', '--']}}]"
    )).unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = RunOptions {
        paths: vec![path.clone()],
        include: vec!["tables".into()],
        apply: true,
        post_process: true,
        ..RunOptions::default()
    };

    let report = run(&options, Some(&config)).unwrap();
    let output = fs::read_to_string(path).unwrap();

    let executed = !skipped && !failed;
    assert_eq!(
        report.ensure_success().is_err(),
        failed || (executed && command_fails)
    );
    assert_eq!(report.files[0].failure.is_some(), failed);
    if failed {
        assert!(
            report.files[0]
                .failure
                .as_ref()
                .unwrap()
                .contains("syntax-error tree")
        );
    }
    assert_eq!(
        report.post_process_failures.len(),
        usize::from(executed && command_fails)
    );
    assert_eq!(
        output,
        if executed {
            format!("{source}// processed\n")
        } else {
            source.into()
        }
    );
    assert_eq!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("post-processing skipped")),
        skipped
    );
}

/// Compare both public entrypoints' consumed bytes, changes and findings.
fn equivalent(source: &str, ext: &str, rules: &str, include: &[&str]) -> (String, Vec<Diagnostic>) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("input.{ext}"));
    let config_path = directory.path().join("config.yml");
    fs::write(&path, source).unwrap();
    fs::write(&config_path, format!("symbol_rules: {rules}")).unwrap();
    let config = load_and_compile(&config_path).unwrap();
    let options = SourceOptions {
        symbol_rules: serde_yml::from_str(rules).unwrap(),
        include: include.iter().map(|code| (*code).into()).collect(),
        all_lines: true,
        ..SourceOptions::default()
    };
    let file_options = RunOptions {
        paths: vec![path.clone()],
        include: options.include.clone(),
        apply: true,
        all_lines: true,
        ..RunOptions::default()
    };

    let buffer = tidy_source(source, ext, &options).unwrap();
    let file = run(&file_options, Some(&config)).unwrap();
    let consumed = fs::read_to_string(path).unwrap();

    assert_eq!(file.files.len(), 1);
    assert!(file.files[0].failure.is_none(), "{:?}", file.files[0]);
    assert_eq!(consumed, buffer.source);
    assert_eq!(file.files[0].changes, buffer.changes);
    assert_eq!(file.files[0].diagnostics, buffer.diagnostics);
    (consumed, buffer.diagnostics)
}
