//! Comment-family text budgets across the lexicon languages.
//!
//! Every test runs `--include lints` on a fixture under
//! `tests/fixtures/doc/`. Prose inside comments measures against the
//! text budgets; string payloads and other quiet regions never do.

use crate::csharp::run_csharp_fixture;
use crate::{defaults_fixture_dir, run_command, run_lexicon_fixture, run_python_fixture, temp_dir};
use rstest::rstest;
use std::fs;

/// C# text budgets fire with original file lines.
///
/// - TEXT001 errors on an over-budget summary paragraph at its first prose line.
/// - TEXT002 warns on a line whose tag-stripped inner text exceeds 80 chars.
#[test]
fn csharp_text_budgets_fire_with_original_lines() {
    let (stderr, exit) = run_csharp_fixture("text-001_text-002_text_budgets.cs");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":10: error[TEXT001]"),
        "TEXT001 must report at the summary's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":19: warning[TEXT002]"),
        "TEXT002 must report at the over-long measured line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "expected exactly 1 TEXT001 finding:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "expected exactly 1 TEXT002 finding:\n{stderr}"
    );
    assert!(
        !stderr.contains("DOC001") && !stderr.contains("DOC004"),
        "the fixture is otherwise documented:\n{stderr}"
    );
}

/// C# text checks stay quiet on the probe classes.
///
/// - Idiomatic XML docs produce no TEXT001/TEXT002 findings.
/// - Long `cref`/`name` attribute values stay unmeasured.
/// - `<code>`/`<example>` blocks stay unmeasured.
/// - Verbatim string content stays unmeasured.
#[test]
fn csharp_text_probes_stay_quiet() {
    let (stderr, exit) = run_csharp_fixture("doc_text_quiet_probes.cs");

    assert_eq!(
        exit, 0,
        "the probe fixture must be clean across every C# lint"
    );
    assert!(
        stderr.is_empty(),
        "idiomatic docs and string content must stay unmeasured:\n{stderr}"
    );
}

/// A default run (no rule selection) checks a mixed-language fixture tree.
///
/// - Every comment-marker family reports its over-budget comment paragraph
///   at its original line.
/// - The string content in each fixture stays unmeasured.
#[test]
fn default_run_lints_comment_prose_in_every_comment_family() {
    let names = [
        "default_budgets.go",
        "default_budgets.rb",
        "default_budgets.sql",
        "default_budgets.el",
        "default_budgets.erl",
    ];
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    for name in names {
        fs::copy(defaults_fixture_dir().join(name), dir.join(name)).unwrap();
    }

    let output = run_command(&["--dry-run"], &dir);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the TEXT001 errors must fail the default run:\n{stderr}"
    );
    for name in names {
        assert!(
            stderr.contains(&format!("{name}:1: error[TEXT001]")),
            "{name}: the comment paragraph must fire in the default run:\n{stderr}"
        );
    }
    assert_eq!(
        stderr.matches("TEXT001").count(),
        names.len(),
        "exactly one comment paragraph per family, never the string content:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        0,
        "no doc line in the fixtures crosses the line budget:\n{stderr}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Lisp `;` and `#| |#` comment prose fires the text budgets at
/// original file lines while `"..."` string content stays quiet.
#[test]
fn el_lexicon_measures_comments_not_strings() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.el");

    assert_ne!(exit, 0, "the TEXT001 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":9: error[TEXT001]"),
        "TEXT001 must report at the block comment's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":15: warning[TEXT002]"),
        "TEXT002 must report at the over-long comment line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        2,
        "exactly the line and block comment paragraphs, never the string:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "exactly the over-long comment line, never the string:\n{stderr}"
    );
}

/// Erlang `%` comment prose fires the text budgets while `<<"...">>`
/// binary content stays quiet.
#[test]
fn erl_lexicon_measures_comments_not_strings() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.erl");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":9: warning[TEXT002]"),
        "TEXT002 must report at the over-long comment line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "exactly the comment paragraph, never the binary literal:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "exactly the over-long comment line, never the binary literal:\n{stderr}"
    );
}

/// JS template literal and string content produce no findings on a
/// probe file whose mis-measured lines would overflow both budgets.
#[test]
fn js_lexicon_string_probes_stay_quiet() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_probes.js");

    assert_eq!(exit, 0, "the probe fixture must be clean:\n{stderr}");
    assert!(
        stderr.is_empty(),
        "template literal and string content must stay unmeasured:\n{stderr}"
    );
}

/// Explicit `--include lints` on a `.js` file fires the text budgets.
///
/// Details:
/// - TEXT001 fires for over-budget `//` and `/** */` prose.
/// - TEXT002 fires for an over-long comment line.
/// - Both report at original file lines.
#[test]
fn js_lexicon_text_budgets_fire_with_original_lines() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.js");

    assert_ne!(exit, 0, "the TEXT001 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":11: error[TEXT001]"),
        "TEXT001 must report at the JSDoc paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":20: warning[TEXT002]"),
        "TEXT002 must report at the over-long comment line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        2,
        "expected exactly 2 TEXT001 findings:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "expected exactly 1 TEXT002 finding:\n{stderr}"
    );
}

/// Each lexicon-family fixture fires exactly one comment paragraph,
/// at its first prose line, and no line crosses the line budget.
///
/// The fixtures' quiet payloads (strings, heredocs, block strings,
/// code) never measure.
///
/// Block-marker cases (`<# #>`, `(* *)`, `{- -}`) report at their
/// first prose line, which follows the line-marker opening. Only
/// `sh` differs: its `#!` shebang is itself a comment line, so the
/// paragraph starts at line 1 like the rest.
#[rstest]
#[case::powershell("doc_text_lexicon_budgets.ps1", 2)]
#[case::applescript("doc_text_lexicon_budgets.applescript", 2)]
#[case::purescript("doc_text_lexicon_budgets.purs", 2)]
#[case::ruby("doc_text_lexicon_budgets.rb", 1)]
#[case::shell("doc_text_lexicon_budgets.sh", 1)]
#[case::yaml("doc_text_lexicon_budgets.yaml", 1)]
#[case::graphql("doc_text_lexicon_budgets.graphql", 1)]
#[case::fish("doc_text_lexicon_budgets.fish", 1)]
#[case::cmake("doc_text_lexicon_budgets.cmake", 1)]
#[case::verilog("doc_text_lexicon_budgets.v", 1)]
#[case::vhdl("doc_text_lexicon_budgets.vhd", 1)]
#[case::toml("doc_text_lexicon_budgets.toml", 1)]
#[case::ksh("doc_text_lexicon_budgets.ksh", 1)]
#[case::latex_style("doc_text_lexicon_budgets.sty", 1)]
#[case::scss("doc_text_lexicon_budgets.scss", 1)]
fn lexicon_comment_prose_fires_once_at_its_first_line(#[case] name: &str, #[case] line: usize) {
    let (stderr, exit) = run_lexicon_fixture(name);

    assert_ne!(
        exit, 0,
        "{name}: the TEXT001 error must fail the run:\n{stderr}"
    );
    assert!(
        stderr.contains(&format!("{name}:{line}: error[TEXT001]")),
        "{name}: the comment paragraph must fire at its first prose line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "{name}: exactly the comment paragraph, never the payload:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        0,
        "{name}: no doc line in the fixture crosses the line budget:\n{stderr}"
    );
}

/// Python `#` comment prose fires TEXT001 while triple-quoted string
/// content and `<<` operators stay quiet.
#[test]
fn py_text_checks_measure_comments_not_strings() {
    let (stderr, exit) = run_python_fixture("doc_text_lexicon_budgets.py");

    assert_ne!(exit, 0, "the TEXT001 error must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        1,
        "exactly the comment paragraph, never the string content:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        0,
        "no doc line in the fixture crosses the line budget:\n{stderr}"
    );
}

/// SQL `--` and `/* */` comment prose fires the text budgets at
/// original file lines while `'...'` string content stays quiet.
#[test]
fn sql_lexicon_measures_comments_not_strings() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_budgets.sql");

    assert_ne!(exit, 0, "the TEXT001 errors must fail the run:\n{stderr}");
    assert!(
        stderr.contains(":1: error[TEXT001]"),
        "TEXT001 must report at the comment paragraph's first line:\n{stderr}"
    );
    assert!(
        stderr.contains(":8: error[TEXT001]"),
        "TEXT001 must report at the block comment's first prose line:\n{stderr}"
    );
    assert!(
        stderr.contains(":13: warning[TEXT002]"),
        "TEXT002 must report at the over-long comment line:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT001").count(),
        2,
        "exactly the line and block comment paragraphs, never the string:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("TEXT002").count(),
        1,
        "exactly the over-long comment line, never the string:\n{stderr}"
    );
}

/// A YAML file with a block-scalar header fails closed: no findings at
/// all, not even for a real over-budget comment line.
#[test]
fn yaml_block_scalar_probes_fail_closed() {
    let (stderr, exit) = run_lexicon_fixture("doc_text_lexicon_probes.yaml");

    assert_eq!(
        exit, 0,
        "the block-scalar probe fixture must be clean:\n{stderr}"
    );
    assert!(
        stderr.is_empty(),
        "a block-scalar YAML file must produce zero findings:\n{stderr}"
    );
}
