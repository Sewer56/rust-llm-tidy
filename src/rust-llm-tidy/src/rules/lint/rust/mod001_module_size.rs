//! `MOD001` - oversized module.
//!
//! [`check_with_options`] excludes top-level `#[cfg(test)]` mod regions unless
//! `include_in_file_tests` is enabled. Files under a `tests/` directory are
//! skipped unless `include_test_files` is enabled ([`is_tests_path`]).
//!
//! Warnings suggest split patterns and explain which test lines count.
//!
//! The rule is file-level, not item-level: it consumes the whole
//! [`ParseResult`] plus the file's path and the per-run threshold. The
//! pipeline therefore invokes it from `check_file` instead of the per-item
//! `run_all` composition.

use crate::reporting::Diagnostic;
use crate::rules::lint::mod001_module_size::{check, diagnostic};
use crate::source::ParseResult;
use crate::text::comments;
use core::fmt::Write;
use std::ffi::OsStr;
use std::path::Path;

/// Warn when a Rust file exceeds its configured line budget.
///
/// Counts every physical line of `parsed.source` (blank lines included).
///
/// Subtracts the module header when `exclude_module_headers` is enabled
/// and, unless `include_in_file_tests` is enabled, every top-level
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
///   regions; the message omits test exclusions.
/// - `include_test_files`: check Rust files under `tests/` using the same budget
///   and inline-test policy as other Rust files.
/// - `exclude_module_headers`: keep the module header's lines out of the count,
///   as `module_size.exclude_module_headers` resolves it.
pub(crate) fn check_with_options(
    parsed: &ParseResult,
    path: &Path,
    max_lines: usize,
    include_in_file_tests: bool,
    include_test_files: bool,
    exclude_module_headers: bool,
) -> Option<Diagnostic> {
    if !include_test_files && is_tests_path(path) {
        return None;
    }

    let header = if exclude_module_headers {
        comments::header_lines(&parsed.source, "rs")
    } else {
        0
    };

    if include_in_file_tests {
        return check(&parsed.source, "rs", max_lines, exclude_module_headers)
            .map(|finding| with_rust_guidance(finding, true, include_test_files, header > 0));
    }

    // The header span covers its lines' bytes without the final newline,
    // so a blank line after the header still counts.
    let mut spans = Vec::with_capacity(usize::from(header > 0));
    if header > 0 {
        spans.push((0, header_span_end(&parsed.source, header)));
    }
    spans.extend(
        parsed
            .items
            .iter()
            .filter(|item| item.is_test_module())
            .map(|item| (item.start, item.end)),
    );
    // File-order items keep the start lines sorted, as the count requires.
    let production_start_lines: Vec<usize> = parsed
        .items
        .iter()
        .filter(|item| !item.is_test_module())
        .map(|item| item.start_line())
        .collect();
    let (non_test_lines, crossing_line) =
        count_lines_outside_spans(&parsed.source, &spans, &production_start_lines, max_lines);

    let exclusions = if header > 0 {
        " outside `#[cfg(test)]` mod regions and module headers"
    } else {
        " outside `#[cfg(test)]` mod regions"
    };

    (non_test_lines > max_lines).then(|| {
        with_rust_guidance(
            diagnostic(
                non_test_lines,
                crossing_line.unwrap_or(1),
                max_lines,
                exclusions,
            ),
            false,
            include_test_files,
            header > 0,
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

/// The byte offset just past the header's last content byte, before its
/// line's newline: the span that owns exactly the header's lines.
fn header_span_end(source: &str, header: usize) -> usize {
    let mut offset = 0;
    for line in source.split('\n').take(header) {
        offset += line.len() + 1;
    }
    // `offset` sits one past the newline (or past the source end for a
    // final unterminated line). Back up one byte so the next line stays
    // outside the span.
    (offset - 1).min(source.len())
}

/// Whether `path` has a `tests` directory component (e.g.
/// `tests/csharp/main.rs`, `src/cli/tests/config/main.rs`).
///
/// A file named `tests.rs` is not a directory component and does not match.
fn is_tests_path(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == OsStr::new("tests"))
}

/// Add Rust-specific split advice and the effective test-counting policies.
fn with_rust_guidance(
    mut finding: Diagnostic,
    include_in_file_tests: bool,
    include_test_files: bool,
    excludes_header: bool,
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
    let headers = if excludes_header {
        "\n- Module headers are excluded from the count."
    } else {
        ""
    };

    indoc::writedoc!(
        finding.message,
        "

        - In Rust, the module root (`mod.rs` or `foo.rs`) is the usual home for
          those entry points.
        - Selective re-exports can expose child-module entry points without a
          forwarding wrapper.
        Counting:
        - Top-level `#[cfg(test)]` test modules are {regions}.
          Other lines count, including comments and blank lines.
        - Rust files in `tests/` directories are {files}.{headers}"
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
        check_with_options(parsed, path, max_lines, false, false, true)
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

    // ── module-header exclusion ──

    // The header span and the test-module spans leave the count together.
    #[test]
    fn stacks_header_exclusion_with_test_module_exclusion() {
        let source = format!("//! Docs.\n{}#[cfg(test)]\nmod t {{\n}}\n", fn_lines(3));
        let parsed = parse(&source);

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 2).expect("3 counted > 2");

        assert!(
            diagnostic.message.starts_with(
                "file has 3 lines outside `#[cfg(test)]` mod regions and module headers,\n"
            ),
            "{}",
            diagnostic.message
        );
        assert_eq!(diagnostic.line, 4, "fn filler_3 is the third counted line");
    }

    // A blank line after the header is an ordinary counted blank: the
    // header span stops before the header's final newline on purpose.
    #[test]
    fn counts_a_blank_line_after_the_header() {
        let parsed = parse("//! Docs.\n\nfn a() {}\n");

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 1).expect("blank + fn > 1");

        assert_eq!(
            diagnostic.line, 3,
            "the fn line crosses; the blank stayed counted"
        );
    }

    // With inline tests counted, only the header leaves the count.
    #[test]
    fn excludes_the_header_when_inline_tests_are_counted() {
        let source = "//! Docs.\nfn a() {}\n#[cfg(test)]\nmod t {}\n";
        let parsed = parse(source);

        let diagnostic = check_with_options(&parsed, Path::new("src/lib.rs"), 2, true, false, true)
            .expect("3 non-header lines > 2");

        assert!(
            diagnostic
                .message
                .starts_with("file has 3 lines outside module headers,\n"),
            "{}",
            diagnostic.message
        );
        assert_eq!(diagnostic.line, 4);
    }

    // `//!` module docs and leading file comments leave the count; the
    // message says so next to the test exclusion.
    #[test]
    fn excludes_module_headers_from_the_count() {
        let source = format!("//! Module docs.\n// Note.\n{}", fn_lines(3));
        let parsed = parse(&source);

        let diagnostic =
            check(&parsed, Path::new("src/lib.rs"), 2).expect("3 non-header lines > 2");

        assert!(
            diagnostic.message.starts_with(
                "file has 3 lines outside `#[cfg(test)]` mod regions and module headers,\n"
            ),
            "{}",
            diagnostic.message
        );
        assert_eq!(diagnostic.line, 5, "the first counted line past the budget");
    }

    // Inner attributes and item documentation stay counted: they are not
    // module headers even when they sit at the file's top.
    #[test]
    fn counts_inner_attributes_and_item_docs() {
        let source = "#![allow(dead_code)]\n/// Item doc.\nfn a() {}\nfn b() {}\n";
        let parsed = parse(source);

        let diagnostic = check(&parsed, Path::new("src/lib.rs"), 3).expect("4 lines > 3");

        assert!(
            diagnostic
                .message
                .starts_with("file has 4 lines outside `#[cfg(test)]` mod regions,\n"),
            "{}",
            diagnostic.message
        );
        assert_eq!(diagnostic.line, 4);
    }

    // `exclude_module_headers: false` restores counting header lines,
    // with the plain test-exclusion message.
    #[test]
    fn counts_module_headers_when_exclusion_is_disabled() {
        let source = format!("//! Module docs.\n{}", fn_lines(3));
        let parsed = parse(&source);

        let diagnostic =
            check_with_options(&parsed, Path::new("src/lib.rs"), 3, false, false, false)
                .expect("4 lines > 3");

        assert!(
            diagnostic
                .message
                .starts_with("file has 4 lines outside `#[cfg(test)]` mod regions,\n"),
            "{}",
            diagnostic.message
        );
        assert_eq!(diagnostic.line, 4);
    }

    // A blank line between banner and module docs joins the header
    // without counting itself.
    #[test]
    fn excludes_a_header_split_by_a_blank_line() {
        let source = format!("// Banner.\n\n//! Docs.\n{}", fn_lines(2));
        let parsed = parse(&source);

        assert!(
            check(&parsed, Path::new("src/lib.rs"), 3).is_none(),
            "the header block's 3 lines (banner, blank, docs) never count; only 2 remain"
        );
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
