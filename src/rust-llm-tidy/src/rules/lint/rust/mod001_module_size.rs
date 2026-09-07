//! `MOD001` - oversized module.
//!
//! [`check`] fires once per `.rs` file whose lines outside top-level
//! `#[cfg(test)]` mod regions exceed the configured budget. Files under a
//! `tests/` directory never fire ([`is_tests_path`]).
//!
//! The rule is file-level, not item-level: it consumes the whole
//! [`ParseResult`] plus the file's path and the per-run threshold. The
//! pipeline therefore invokes it from `check_file` instead of the per-item
//! `run_all` composition.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MODULE_SIZE;
use crate::source::ParseResult;
use std::path::Path;

/// `MOD001` - warn when a module outgrows the non-test line budget.
///
/// Counts every physical line of `parsed.source` (blank lines included),
/// then subtracts every top-level `#[cfg(test)]`-gated `mod` item span,
/// attributes through closing brace.
///
/// Fires when the remaining count strictly exceeds `max_lines`: a file at
/// exactly `max_lines` stays silent.
///
/// A `#[cfg(test)]` attribute on a non-`mod` item is not a module region;
/// its lines count.
///
/// The finding reports at the first line past the budget. It never fires
/// for `path` under a `tests/` directory ([`is_tests_path`]).
///
/// # Arguments
///
/// - `parsed`: the file's parse facts; `source` and item spans drive the
///   count.
/// - `path`: the file's path; a `tests` directory component skips the rule.
/// - `max_lines`: the resolved `module_size.max_lines` budget.
pub(crate) fn check(parsed: &ParseResult, path: &Path, max_lines: usize) -> Option<Diagnostic> {
    if is_tests_path(path) {
        return None;
    }

    let test_spans: Vec<(usize, usize)> = parsed
        .items
        .iter()
        .filter(|item| item.is_test_module())
        .map(|item| (item.start, item.end))
        .collect();
    let (non_test_lines, crossing_line) =
        count_lines_outside_spans(&parsed.source, &test_spans, max_lines);

    (non_test_lines > max_lines).then(|| Diagnostic {
        severity: Severity::Warning,
        code: CODE_MODULE_SIZE,
        message: format!(
            "module has {non_test_lines} lines outside `#[cfg(test)]` mod regions, \
             over the {max_lines}-line budget (module_size.max_lines); move new code \
             that is a separate thing to its own file, and plan new files as several \
             modules up front rather than one growing module"
        ),
        line: crossing_line.unwrap_or(1),
        item_kind: "file".to_string(),
        item_name: None,
    })
}

/// Count physical lines whose byte range avoids every span in `spans`,
/// tracking where the count passes `max_lines`.
///
/// A line belongs to a span when the line's byte range - `[start, end)`,
/// newline excluded - intersects it. A test-module span therefore removes
/// exactly its own lines. The piece after a trailing newline is not a
/// physical line.
///
/// Returns the outside-span line count plus the 1-based line holding the
/// `max_lines + 1`-th counted line, or `None` when the file never crosses.
fn count_lines_outside_spans(
    source: &str,
    spans: &[(usize, usize)],
    max_lines: usize,
) -> (usize, Option<usize>) {
    let mut non_test_lines = 0;
    let mut crossing_line = None;
    let mut line_number = 0;
    let mut offset = 0;
    for line in source.split('\n') {
        // The piece after a trailing newline is not a physical line.
        if offset == source.len() {
            break;
        }
        line_number += 1;
        let line_end = offset + line.len();
        let in_span = spans
            .iter()
            .any(|&(start, end)| start < line_end && end > offset);
        if !in_span {
            non_test_lines += 1;
            if non_test_lines > max_lines && crossing_line.is_none() {
                crossing_line = Some(line_number);
            }
        }
        offset = line_end + 1;
    }
    (non_test_lines, crossing_line)
}

/// Whether `path` has a `tests` directory component (e.g.
/// `tests/integration.rs`, `src/cli/tests/config.rs`).
///
/// Such files never fire MOD001 but stay open to every other code. A file
/// named `tests.rs` is not a directory component and does not match.
fn is_tests_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new("tests"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// Parse `source` with the Rust backend's parser.
    fn parse(source: &str) -> ParseResult {
        crate::languages::rust::parse::parse_source(source).unwrap()
    }

    /// `count` private fn lines: no docs, visibility, or test attrs, so
    /// MOD001 is the only code that can fire on the file.
    fn fn_lines(count: usize) -> String {
        (0..count)
            .map(|i| format!("fn filler_{i}() {{}}\n"))
            .collect()
    }

    // ── firing and the strict boundary ──

    // Over the budget: one warning at the first line past the budget.
    #[test]
    fn fires_when_non_test_lines_exceed_the_threshold() {
        let parsed = parse(&fn_lines(3));

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 2).expect("3 lines > 2");

        assert_eq!(diagnostic.code, CODE_MODULE_SIZE);
        assert_eq!(diagnostic.severity, Severity::Warning);
        assert_eq!(diagnostic.line, 3, "the first line past the budget");
        assert_eq!(diagnostic.item_kind, "file");
        assert!(diagnostic.item_name.is_none());
        assert!(
            diagnostic.message.contains("separate thing")
                && diagnostic.message.contains("up front"),
            "the message must carry both guidance clauses: {}",
            diagnostic.message
        );
    }

    // Exactly at the budget: silent (strictly greater fires).
    #[test]
    fn stays_silent_at_exactly_the_threshold() {
        let parsed = parse(&fn_lines(2));

        assert!(check(&parsed, Path::new("src/lib.rs"), 2).is_none());
    }

    // Under the budget: silent.
    #[test]
    fn stays_silent_under_the_threshold() {
        let parsed = parse(&fn_lines(1));

        assert!(check(&parsed, Path::new("src/lib.rs"), 500).is_none());
    }

    // Blank lines are file lines and count toward the budget.
    #[test]
    fn counts_blank_lines_toward_the_budget() {
        let parsed = parse("fn a() {}\n\nfn b() {}\n");

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 2).expect("3 lines > 2");

        assert_eq!(diagnostic.line, 3);
    }

    // ── `#[cfg(test)]` region exclusion ──

    // The whole `#[cfg(test)]` mod span, attribute through closing brace,
    // leaves the count.
    #[test]
    fn excludes_cfg_test_mod_regions_from_the_count() {
        let source = format!(
            "{}#[cfg(test)]\nmod tests {{\n{}\n}}\n",
            fn_lines(2),
            "    fn helper() {{}}\n".repeat(5)
        );
        let parsed = parse(&source);

        // 10 physical lines, 2 counted.
        assert!(
            check(&parsed, Path::new("src/lib.rs"), 8).is_none(),
            "test-module lines must not count toward the budget"
        );
        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 1).expect("2 non-test lines > 1");
        assert_eq!(diagnostic.line, 2);
    }

    // Every test-module region drops out, not just the first.
    #[test]
    fn excludes_every_cfg_test_mod_region() {
        let source = format!(
            "{}#[cfg(test)]\nmod a {{\n}}\n#[cfg(test)]\nmod b {{\n}}\n",
            fn_lines(2)
        );
        let parsed = parse(&source);

        // 8 physical lines, 2 counted.
        assert!(check(&parsed, Path::new("src/lib.rs"), 5).is_none());
    }

    // A `#[cfg(test)]` attribute on a non-`mod` item is not a module
    // region: its lines count.
    #[test]
    fn counts_cfg_test_attributes_on_non_mod_items() {
        let parsed = parse("#[cfg(test)]\nfn gated() {}\nfn other() {}\n");

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 2).expect("3 lines > 2");

        assert_eq!(diagnostic.line, 3);
    }

    // The crossing line lands after a mid-file test region, on the line
    // holding the `max_lines + 1`-th counted line.
    #[test]
    fn reports_the_crossing_line_after_test_regions() {
        let source = format!(
            "{}#[cfg(test)]\nmod tests {{\n}}\n{}",
            fn_lines(2),
            fn_lines(2)
        );
        let parsed = parse(&source);

        // Counted lines sit at file lines 1, 2, 6, 7.
        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 3).expect("4 counted > 3");

        assert_eq!(diagnostic.line, 7);
    }

    // ── tests/ path skip ──

    // A `tests` directory component at any depth skips the rule.
    #[rstest]
    #[case::integration_tests_root("tests/integration.rs")]
    #[case::nested_tool_tests_dir("src/cli/tests/config.rs")]
    fn never_fires_under_a_tests_directory(#[case] path: &str) {
        let parsed = parse(&fn_lines(600));

        assert!(
            check(&parsed, Path::new(path), 500).is_none(),
            "no MOD001 under a `tests` directory component"
        );
    }

    // Only an exact `tests` component matches: a `tests.rs` file name or a
    // singular `test` directory still fires.
    #[rstest]
    #[case::file_named_tests("src/tests.rs")]
    #[case::singular_test_dir("src/test/config.rs")]
    fn fires_when_no_component_is_exactly_tests(#[case] path: &str) {
        let parsed = parse(&fn_lines(3));

        assert!(check(&parsed, Path::new(path), 2).is_some());
    }
}
