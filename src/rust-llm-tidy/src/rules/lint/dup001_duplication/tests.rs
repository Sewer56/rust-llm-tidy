//! Matcher acceptance for strict queries, textual equality, and independent sites.

use super::{analyze, check};
use crate::config::DuplicationConfig;
use crate::input::changed_lines::ChangedLines;
use core::iter::once;
use core::ops::RangeInclusive;
use rstest::rstest;

const BLOCK: &str = "load();\nclassify();\nrecord();\nflush();\nfinish();\n";

#[rstest]
#[case::indentation(" a \n b\t\n", false, 1)]
#[case::exact_indentation(" a \n b\t\n", true, 0)]
#[case::line_endings("a\r\nb\r\n", false, 1)]
#[case::exact_endings("a\r\nb\r\n", true, 0)]
#[case::internal_space("a a\nb\n", false, 0)]
#[case::identifier("x\nb\n", false, 0)]
#[case::exact_equal("a\nb\n", true, 1)]
fn check_should_compare_only_selected_whitespace_normalization(
    #[case] third: &str,
    #[case] exact_whitespace: bool,
    #[case] count: usize,
) {
    let source = format!("a\nb\na\nb\n{third}");
    let config = DuplicationConfig {
        min_meaningful_lines: 2,
        exact_whitespace,
        ..DuplicationConfig::default()
    };

    let diagnostics = check(&source, &ChangedLines::new(once(5..=6)), config);

    assert_eq!(diagnostics.len(), count, "{diagnostics:?}");
}

#[rstest]
#[case::overlapping_only(7, 0)]
#[case::independent(9, 1)]
fn check_should_count_non_overlapping_sites(#[case] lines: usize, #[case] count: usize) {
    let source = "a\n".repeat(lines);
    let config = DuplicationConfig {
        min_meaningful_lines: 3,
        ..DuplicationConfig::default()
    };

    let diagnostics = check(&source, &ChangedLines::all(&source), config);

    assert_eq!(diagnostics.len(), count, "{diagnostics:?}");
}

#[test]
fn check_should_emit_the_documented_finding_for_the_added_struct_literal() {
    // The before/after pair from `docs/lints.md`, kept in fixtures so the test
    // never parses the documentation tree. Git may check the fixtures out with
    // CRLF on Windows; the rule and its render always use LF.
    let source = include_str!("../../../../tests/fixtures/dup001/documented_source.rs")
        .replace("\r\n", "\n");
    let expected = include_str!("../../../../tests/fixtures/dup001/documented_expected.txt")
        .replace("\r\n", "\n");
    let expected = expected.trim_end();
    let query_start = source
        .lines()
        .position(|line| line.starts_with("let metrics ="))
        .unwrap()
        + 1;

    let findings = check(
        &source,
        &ChangedLines::new(once(query_start..=source.lines().count())),
        DuplicationConfig::default(),
    );

    assert_eq!(findings.len(), 1);
    assert_eq!(format!("input.rs:{}", findings[0]), expected);
}

#[rstest]
#[case::multiline_string("text = '''\n  alpha\n  beta\n'''\n", "text = '''\nalpha\nbeta\n'''\n")]
#[case::heredoc("cat <<EOF\n  alpha\n  beta\nEOF\n", "cat <<EOF\nalpha\nbeta\nEOF\n")]
#[case::indentation_language("if ready:\n  load()\n  save()\n", "if ready:\nload()\nsave()\n")]
fn check_should_expose_literal_and_indentation_limits(
    #[case] indented: &str,
    #[case] unindented: &str,
) {
    let source = format!("{indented}{indented}{unindented}");
    let config = DuplicationConfig {
        min_meaningful_lines: 3,
        ..DuplicationConfig::default()
    };
    let eligible = ChangedLines::new(once(
        indented.lines().count() * 2 + 1..=source.lines().count(),
    ));

    let normalized = check(&source, &eligible, config);
    let exact = check(
        &source,
        &eligible,
        DuplicationConfig {
            exact_whitespace: true,
            ..config
        },
    );

    assert_eq!(normalized.len(), 1, "{normalized:?}");
    assert!(exact.is_empty(), "{exact:?}");
}

#[test]
fn check_should_include_a_query_that_overlaps_an_earlier_match() {
    let source = "a\n".repeat(10);
    let config = DuplicationConfig {
        min_meaningful_lines: 3,
        ..DuplicationConfig::default()
    };

    let diagnostics = check(&source, &ChangedLines::new(once(2..=4)), config);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
    assert!(
        diagnostics[0]
            .message
            .contains("Locations: 2-4, 5-7, 8-10.")
    );
}

#[test]
fn check_should_keep_distinct_overlapping_groups() {
    let source = "a\nb\nc\nx\na\nb\ny\na\nb\nz\nb\nc\nw\nb\nc\n";
    let config = DuplicationConfig {
        min_meaningful_lines: 2,
        ..DuplicationConfig::default()
    };

    let diagnostics = check(source, &ChangedLines::all(source), config);

    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!(
        diagnostics.iter().map(|d| d.line).collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn check_should_keep_rendered_results_stable_across_runs() {
    let source = format!("{}unique();\n{}", BLOCK.repeat(3), "x\ny\nz\n".repeat(3));
    let config = DuplicationConfig {
        min_meaningful_lines: 3,
        ..DuplicationConfig::default()
    };
    let eligible = ChangedLines::all(&source);
    let expected: Vec<_> = check(&source, &eligible, config)
        .iter()
        .map(ToString::to_string)
        .collect();

    // Repeated execution exercises randomized map traversal.
    for _ in 0..10 {
        let actual: Vec<_> = check(&source, &eligible, config)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(actual, expected);
    }
}

#[rstest]
#[case::small(30)]
#[case::medium(120)]
#[case::large(480)]
fn check_should_preserve_consumed_findings_under_growing_collision_load(#[case] groups: usize) {
    let config = DuplicationConfig::default();
    let source: String = (0..groups)
        .map(|group| {
            let block: String = (0..config.min_meaningful_lines)
                .map(|line| format!("step_{group}_{line}();\n"))
                .collect();
            (0..config.min_occurrences)
                .map(|site| format!("{block}separator_{group}_{site}();\n"))
                .collect::<String>()
        })
        .collect();
    let eligible = ChangedLines::all(&source);

    let normal = check(&source, &eligible, config);
    let collisions = analyze(&source, &eligible, config, 0);

    assert_eq!(normal.len(), groups);
    assert_eq!(
        normal.iter().map(ToString::to_string).collect::<Vec<_>>(),
        collisions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

#[rstest]
#[case::blank("a\n\nb\n", "a\nb\n", 0)]
#[case::punctuation("a\n{}\nb\n", "a\n[]\nb\n", 0)]
#[case::preserved("a\n{}\n\nb\n", "a\n{}\n\nb\n", 1)]
#[case::comments("// alpha\n// beta\n", "// alpha\n// beta\n", 1)]
#[case::literal_value("x = 1;\ny = 2;\n", "x = 1;\ny = 3;\n", 0)]
#[case::unicode("α();\nβ();\n", "α();\nβ();\n", 1)]
fn check_should_preserve_non_whitespace_source_text(
    #[case] original: &str,
    #[case] third: &str,
    #[case] count: usize,
) {
    let source = format!("{original}{original}{third}");
    let config = DuplicationConfig {
        min_meaningful_lines: 2,
        ..DuplicationConfig::default()
    };
    let start = original.lines().count() * 2 + 1;

    let diagnostics = check(
        &source,
        &ChangedLines::new(once(start..=source.lines().count())),
        config,
    );

    assert_eq!(diagnostics.len(), count, "{diagnostics:?}");
}

#[test]
fn check_should_render_identically_despite_fingerprint_collisions() {
    let source = format!("{BLOCK}unique();\n{BLOCK}different();\n{BLOCK}");
    let eligible = ChangedLines::all(&source);
    let config = DuplicationConfig::default();

    let normal = check(&source, &eligible, config);
    let collisions = analyze(&source, &eligible, config, 0);

    assert!(!normal.is_empty());
    assert_eq!(
        normal.iter().map(ToString::to_string).collect::<Vec<_>>(),
        collisions
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

#[rstest]
#[case::whole_third(vec![11..=15], 1)]
#[case::one_old_line(vec![12..=12], 0)]
#[case::separate_short_runs(vec![11..=12, 14..=15], 0)]
#[case::empty_diff(vec![], 0)]
#[case::query_cannot_borrow_context(vec![12..=15], 0)]
fn check_should_require_a_complete_query_run(
    #[case] ranges: Vec<RangeInclusive<usize>>,
    #[case] count: usize,
) {
    let config = DuplicationConfig::default();
    let source = BLOCK.repeat(config.min_occurrences);

    let diagnostics = check(&source, &ChangedLines::new(ranges), config);

    assert_eq!(diagnostics.len(), count, "{diagnostics:?}");
    if count != 0 {
        assert_eq!(diagnostics[0].line, 11);
        assert!(
            diagnostics[0]
                .message
                .contains("Locations: 1-5, 6-10, 11-15.")
        );
    }
}

#[rstest]
#[case::empty("", 0)]
#[case::one(BLOCK, 0)]
#[case::two(concat!("a\nb\nc\nd\ne\n", "a\nb\nc\nd\ne\n"), 0)]
#[case::three(concat!("a\nb\nc\nd\ne\n", "a\nb\nc\nd\ne\n", "a\nb\nc\nd\ne\n"), 1)]
fn check_should_require_default_thresholds(#[case] source: &str, #[case] count: usize) {
    let diagnostics = check(
        source,
        &ChangedLines::all(source),
        DuplicationConfig::default(),
    );

    assert_eq!(diagnostics.len(), count, "{diagnostics:?}");
}

#[rstest]
#[case::below_line_threshold(4, 3, 0)]
#[case::at_line_threshold(5, 3, 1)]
#[case::above_line_threshold(6, 3, 1)]
#[case::below_site_threshold(5, 2, 0)]
#[case::above_site_threshold(5, 4, 1)]
fn check_should_respect_threshold_boundaries(
    #[case] meaningful_lines: usize,
    #[case] occurrences: usize,
    #[case] count: usize,
) {
    let config = DuplicationConfig::default();
    let block: String = (0..meaningful_lines)
        .map(|index| format!("step_{index}();\n"))
        .collect();
    let source = block.repeat(occurrences);
    let query = ChangedLines::new(once(1..=meaningful_lines));

    let findings = check(&source, &query, config);

    assert_eq!(findings.len(), count);
}

#[test]
fn check_should_retain_shorter_groups_with_extra_occurrences() {
    let source = "a\nb\nc\nx\na\nb\nc\ny\na\nb\nc\nz\na\nb\n";
    let config = DuplicationConfig {
        min_meaningful_lines: 2,
        ..DuplicationConfig::default()
    };

    let diagnostics = check(source, &ChangedLines::all(source), config);

    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("3 meaningful lines repeat at 3"))
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("2 meaningful lines repeat at 4"))
    );
}

#[test]
fn check_should_skip_nul_containing_source() {
    let source = format!(
        "\0{}",
        BLOCK.repeat(DuplicationConfig::default().min_occurrences)
    );

    let findings = check(
        &source,
        &ChangedLines::all(&source),
        DuplicationConfig::default(),
    );

    assert!(findings.is_empty());
}
