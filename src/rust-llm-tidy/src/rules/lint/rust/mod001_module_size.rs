//! `MOD001` - oversized module.
//!
//! [`check_with_options`] excludes top-level `#[cfg(test)]` mod regions unless
//! `include_in_file_tests` is enabled. Files under a `tests/` directory are
//! skipped unless `include_test_files` is enabled ([`is_tests_path`]).
//!
//! Warnings guide module-root organization and explain which test lines count.
//!
//! The rule is file-level, not item-level: it consumes the whole
//! [`ParseResult`] plus the file's path and the per-run threshold. The
//! pipeline therefore invokes it from `check_file` instead of the per-item
//! `run_all` composition.

use crate::reporting::Diagnostic;
use crate::rules::lint::mod001_module_size::diagnostic;
use crate::source::ParseResult;
use core::fmt::Write;
use std::path::Path;

/// Warn when a Rust file exceeds its configured line budget.
///
/// Counts every physical line of `parsed.source` (blank lines included),
/// then, unless `include_in_file_tests` is enabled, subtracts every top-level
/// `#[cfg(test)]`-gated `mod` item span, attributes through closing brace.
///
/// A line that shares a test-module span with production code still
/// counts.
///
/// Fires when the remaining count strictly exceeds `max_lines`: a file at
/// exactly `max_lines` stays silent.
///
/// A `#[cfg(test)]` attribute on a non-`mod` item is not a module region;
/// its lines count.
///
/// The finding reports at the first line past the budget. Files under a `tests/`
/// directory are exempt unless `include_test_files` is enabled ([`is_tests_path`]).
///
/// # Arguments
///
/// - `parsed`: the file's parse facts; `source`, item spans, and item
///   start lines drive the count.
/// - `path`: the file's path, checked for an exact `tests` directory component.
/// - `max_lines`: the resolved `module_size.max_lines` budget.
/// - `include_in_file_tests`: count the whole file instead of subtracting test
///   regions; the message omits exclusions.
/// - `include_test_files`: check Rust files under `tests/` using the same budget
///   and inline-test policy as other Rust files.
pub(crate) fn check_with_options(
    parsed: &ParseResult,
    path: &Path,
    max_lines: usize,
    include_in_file_tests: bool,
    include_test_files: bool,
) -> Option<Diagnostic> {
    if !include_test_files && is_tests_path(path) {
        return None;
    }

    if include_in_file_tests {
        return crate::rules::lint::mod001_module_size::check(&parsed.source, max_lines)
            .map(|finding| with_rust_guidance(finding, true, include_test_files));
    }

    let test_spans: Vec<(usize, usize)> = parsed
        .items
        .iter()
        .filter(|item| item.is_test_module())
        .map(|item| (item.start, item.end))
        .collect();
    // File-order items keep the start lines sorted, as the count requires.
    let production_start_lines: Vec<usize> = parsed
        .items
        .iter()
        .filter(|item| !item.is_test_module())
        .map(|item| item.start_line())
        .collect();
    let (non_test_lines, crossing_line) = count_lines_outside_spans(
        &parsed.source,
        &test_spans,
        &production_start_lines,
        max_lines,
    );

    (non_test_lines > max_lines).then(|| {
        with_rust_guidance(
            diagnostic(
                non_test_lines,
                crossing_line.unwrap_or(1),
                max_lines,
                " outside `#[cfg(test)]` mod regions",
            ),
            false,
            include_test_files,
        )
    })
}

/// Count physical lines no span fully owns, tracking where the count
/// passes `max_lines`.
///
/// A line leaves the count only when both hold:
///
/// - A span covers the line's entire byte range, `\r` from CRLF input
///   aside.
/// - No `production_start_lines` entry names the line.
///
/// Spans run to the next line start, so a test-module span absorbs
/// same-line trailing code (`#[cfg(test)] mod t {} fn c() {}`). The
/// `production_start_lines` list restores such lines: a span removes
/// only lines it alone occupies.
///
/// The piece after a trailing newline is not a physical line.
///
/// Returns the outside-span line count plus the 1-based line holding the
/// `max_lines + 1`-th counted line, or `None` when the file never crosses.
fn count_lines_outside_spans(
    source: &str,
    spans: &[(usize, usize)],
    production_start_lines: &[usize],
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
        // CRLF: the `\r` is line dressing, not span content.
        let line_end_no_cr = if line.ends_with('\r') {
            line_end - 1
        } else {
            line_end
        };
        let hosts_production = production_start_lines.binary_search(&line_number).is_ok();
        let span_owns_line = !hosts_production
            && spans
                .iter()
                .any(|&(start, end)| start <= offset && end >= line_end_no_cr);
        if !span_owns_line {
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
/// `tests/csharp/main.rs`, `src/cli/tests/config/main.rs`).
///
/// A file named `tests.rs` is not a directory component and does not match.
fn is_tests_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new("tests"))
}

/// Add Rust module-root guidance and the effective test-counting policies.
fn with_rust_guidance(
    mut finding: Diagnostic,
    include_in_file_tests: bool,
    include_test_files: bool,
) -> Diagnostic {
    let regions = if include_in_file_tests {
        "counted"
    } else {
        "excluded"
    };
    let files = if include_test_files {
        "checked"
    } else {
        "skipped"
    };

    indoc::writedoc!(
        finding.message,
        "

        - When splitting a Rust module, consider keeping main entry points and
          high-level orchestration in the module root (`mod.rs` or `foo.rs`).
          Put implementation details in child modules.
        - If entry points belong in child modules, consider selective re-exports
          through the root without widening visibility.
        - Update root docs to explain the module's purpose and direct readers
          to main entry points and relevant child modules.
        - Top-level `#[cfg(test)]` test modules are {regions}.
          Other lines count, including comments and blank lines.
        - Rust files in `tests/` directories are {files}."
    )
    .expect("writing to a String cannot fail");
    finding
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reporting::Severity;
    use crate::rules::lint::CODE_MODULE_SIZE;
    use rstest::rstest;

    /// Exercise the default test-excluding policy in existing Rust cases.
    fn check(parsed: &ParseResult, path: &Path, max_lines: usize) -> Option<Diagnostic> {
        check_with_options(parsed, path, max_lines, false, false)
    }

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
            diagnostic.message.starts_with(
                "file has 3 lines outside `#[cfg(test)]` mod regions,\n\
                 over the 2-line budget (module_size.max_lines).\n"
            ),
            "the Rust warning must explain the test exclusion: {}",
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

    // A line mixing a test module with trailing production code still
    // counts: the production item started on that line keeps it in the
    // budget.
    #[test]
    fn counts_a_line_mixing_a_test_module_with_production_code() {
        let parsed = parse("fn a() {}\nfn b() {}\n#[cfg(test)] mod t {} fn c() {}\n");

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 2).expect("3 non-test lines > 2");

        assert_eq!(
            diagnostic.line, 3,
            "the mixed line is the first past the budget"
        );
        assert!(
            check(&parsed, Path::new("src/lib.rs"), 3).is_none(),
            "exactly 3 non-test lines"
        );
    }

    // A test module's closing brace sharing its line with a following
    // production item keeps that line in the count too.
    #[test]
    fn counts_a_closing_brace_line_shared_with_production_code() {
        let parsed = parse("fn a() {}\nfn b() {}\n#[cfg(test)]\nmod t {\n} fn c() {}\n");

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 2).expect("3 non-test lines > 2");

        assert_eq!(
            diagnostic.line, 5,
            "the shared closing-brace line crosses the budget"
        );
    }

    // CRLF input: fully covered test-module lines stay excluded even
    // though each line ends with `\r`.
    #[test]
    fn excludes_fully_covered_crlf_test_module_lines() {
        let source = "fn a() {}\r\n#[cfg(test)]\r\nmod t {\r\n}\r\nfn b() {}\r\n";
        let parsed = parse(source);

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 1).expect("2 non-test lines > 1");

        assert_eq!(diagnostic.line, 5, "fn b's CRLF line crosses the budget");
    }

    // Direct helper contract, unreachable through `check` (its spans
    // always end past the newline). A span may stop at the `\r` and
    // still cover the line.
    #[test]
    fn covers_a_line_whose_span_stops_at_the_carriage_return() {
        let (count, crossing) = count_lines_outside_spans("fn a() {}\r\n", &[(0, 9)], &[], 0);

        assert_eq!(
            (count, crossing),
            (0, None),
            "the span still covers the line"
        );
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
