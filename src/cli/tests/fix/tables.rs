//! Table realignment across languages: what changes and what must not.
//!
//! The shared runner helpers live in `mod.rs`.

use super::{fixture_dir, run_command, temp_file};
use rstest::rstest;
use std::fs;

/// Pipe-bearing lines that never form a GFM table stay byte-unchanged under
/// the default run.
///
/// Cases include Haskell guard runs, SQL `||` concatenation, and Lua comment
/// notes without a delimiter row.
#[test]
fn non_table_pipe_lines_stay_byte_unchanged() {
    for name in [
        "table_hs_guards_untouched.hs",
        "table_sql_concat_untouched.sql",
        "table_lua_dash_notes_untouched.lua",
    ] {
        let ext = name.rsplit_once('.').unwrap().1;
        let original = fs::read_to_string(fixture_dir().join(name)).unwrap();
        let tmp = temp_file(ext);
        fs::write(&tmp, &original).unwrap();

        let output = run_command(&[], &tmp);
        let after = fs::read_to_string(&tmp).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(&tmp);
        assert!(
            output.status.success(),
            "{name}: default run should succeed: {stderr}"
        );
        assert_eq!(
            after, original,
            "{name}: non-table pipe lines must stay byte-unchanged"
        );
        assert!(
            !stderr.contains("success["),
            "{name}: a non-table file must report zero change records: {stderr}"
        );
    }
}

// ── Per-language table fixtures ────────────────────────────────────

/// Tables realign in verified comments and prose; unsupported sources stay exact.
///
/// Changed tables retain their marker and indentation and report one fix.
/// A second dry run leaves the consumed bytes intact and reports no fixes.
#[rstest]
#[case::go_preserved("table_slash_comment_before.go", "table_slash_comment_before.go")]
#[case::csharp_realigned("table_xml_doc_comment_before.cs", "table_xml_doc_comment_after.cs")]
#[case::python_realigned("table_hash_comment_before.py", "table_hash_comment_after.py")]
#[case::sql_preserved("table_dash_comment_before.sql", "table_dash_comment_before.sql")]
#[case::elisp_preserved("table_semi_comment_before.el", "table_semi_comment_before.el")]
#[case::tex_preserved("table_percent_comment_before.tex", "table_percent_comment_before.tex")]
#[case::prose_realigned("table_txt_before.txt", "table_txt_after.txt")]
fn tables_should_follow_language_rewrite_boundaries(
    #[case] before_name: &str,
    #[case] expected_name: &str,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(before_name);
    let source = fs::read(fixture_dir().join(before_name)).unwrap();
    let expected = fs::read(fixture_dir().join(expected_name)).unwrap();
    fs::write(&path, &source).unwrap();

    let output = run_command(&[], &path);
    let consumed = fs::read(&path).unwrap();

    assert_eq!(consumed, expected);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("success[FIX]").count(),
        usize::from(source != expected),
        "only a realigned table should report a fix: {stderr}"
    );

    let second = run_command(&["--include", "tables", "--dry-run"], &path);
    let second_consumed = fs::read(&path).unwrap();

    assert!(second.status.success(), "{second:?}");
    assert!(second.stderr.is_empty(), "{second:?}");
    assert_eq!(second_consumed, expected);
}
