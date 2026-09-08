//! Module-header recognition for MOD001's line budget.
//!
//! A module header is the leading block of full-line file comments and
//! module documentation before the file's code: the family's comment
//! markers, Rust `//!` and `/*! ... */` docs, and Python's module
//! docstring.
//!
//! [`header_lines`] returns the block's physical-line extent, so the
//! size rule can keep it out of the budget.
//!
//! The block starts at line 1 and ends at the first line that is
//! neither header content nor blank. Python's module docstring, when
//! present, always ends the block.
//!
//! Blank lines inside the block belong to it; a blank after the
//! block's last content line is an ordinary counted blank.
//!
//! Failure keeps lines counted. An unterminated block comment or
//! docstring cancels its own lines from the header. A closing
//! delimiter sharing its line with code counts that line.

/// The header syntax one extension recognizes.
struct HeaderSyntax {
    /// The line-comment marker.
    line: &'static str,
    /// The block-comment open/close pair.
    block: Option<(&'static str, &'static str)>,
    /// Whether block markers comment only alone on their lines.
    alone_markers: bool,
    /// Whether a module docstring may form the header.
    docstring: bool,
    /// Whether `///` and `/**` document the following item instead of
    /// the file: only Rust and C# read them that way.
    item_docs: bool,
}

/// The number of leading physical lines forming `ext`'s module header.
///
/// Unsupported extensions and files whose first line is code return 0.
/// The returned lines are always a contiguous prefix of the file, so
/// `count - header` needs no per-line bookkeeping.
///
/// # Arguments
///
/// - `source`: the file's raw text; `\r\n` endings are handled.
/// - `ext`: the file extension without the leading dot, matched
///   ASCII case-insensitively.
pub fn header_lines(source: &str, ext: &str) -> usize {
    let Some(syntax) = syntax_for(ext) else {
        return 0;
    };
    let (line, block, alone_markers, docstring_allowed) = (
        syntax.line,
        syntax.block,
        syntax.alone_markers,
        syntax.docstring,
    );

    // The last physical line of the header block; blank lines inside
    // the block stay skipped, so trailing blanks after it still count.
    let mut end = 0usize;
    // The header length before the currently open region: restored when
    // the region never closes, keeping its uncertain lines counted.
    let mut before_open_region = 0usize;
    // The closing delimiter of the open block comment or docstring.
    let mut fence: Option<&'static str> = None;
    // Whether the open fence belongs to Python's module docstring: its
    // close is the header's last part, later comments are body comments.
    let mut in_docstring = false;

    for (idx, raw) in source.split('\n').enumerate() {
        let number = idx + 1;
        let text = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = text.trim_start();

        if let Some(close) = fence {
            match text.find(close) {
                Some(pos) => {
                    if tail_is_comment_or_empty(&text[pos + close.len()..], line) {
                        end = number;
                        fence = None;
                        if in_docstring {
                            break;
                        }
                    } else {
                        // Code after the closer shares the line: the
                        // line counts and the header ends here, with
                        // the region's earlier lines still excluded.
                        fence = None;
                        break;
                    }
                }
                // Fully inside the region.
                None => end = number,
            }
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with(line) && (!syntax.item_docs || !is_item_doc(trimmed)) {
            end = number;
            continue;
        }

        if let Some((open, close)) = block
            && (if alone_markers {
                trimmed == open
            } else {
                trimmed.starts_with(open)
            })
            && (!syntax.item_docs || !is_item_doc(trimmed))
        {
            before_open_region = end;
            let after_open = &text[text.len() - trimmed.len() + open.len()..];
            match after_open.find(close) {
                // Closed on the opener line.
                Some(pos) => {
                    if tail_is_comment_or_empty(&after_open[pos + close.len()..], line) {
                        end = number;
                    } else {
                        // Code follows the closer on the same line.
                        break;
                    }
                }
                None => {
                    end = number;
                    fence = Some(close);
                }
            }
            continue;
        }

        if docstring_allowed && let Some((fence_str, after)) = module_docstring_open(text) {
            before_open_region = end;
            in_docstring = true;
            match after.find(fence_str) {
                Some(pos) => {
                    if tail_is_comment_or_empty(&after[pos + fence_str.len()..], line) {
                        end = number;
                    }
                    // The module docstring is the last header part:
                    // the line counts only when code follows its
                    // closer, and either way the header ends.
                    break;
                }
                None => {
                    end = number;
                    fence = Some(fence_str);
                }
            }
            continue;
        }

        // The first code line ends the header.
        break;
    }

    if fence.is_some() {
        // An unterminated region is uncertain: keep its lines counted.
        end = before_open_region;
    }
    end
}

/// Whether a Rust or C# comment line documents the following item
/// rather than the file: `///` outer docs and `/**` blocks.
fn is_item_doc(trimmed: &str) -> bool {
    trimmed.starts_with("///") || trimmed.starts_with("/**")
}

/// Whether `text` opens Python's module docstring at column 0.
///
/// An optional string prefix (`r`, `b`, `u`, `f`, combined) may precede
/// the triple quote. Returns the closing fence and the text after the
/// opener.
fn module_docstring_open(text: &str) -> Option<(&'static str, &str)> {
    let after_prefix = text.trim_start_matches(['r', 'R', 'b', 'B', 'u', 'U', 'f', 'F']);
    for fence in ["\"\"\"", "'''"] {
        if let Some(after) = after_prefix.strip_prefix(fence) {
            return Some((fence, after));
        }
    }
    None
}

/// The header syntax for `ext`.
///
/// Rust and C# sit outside the comment lexicon; both use `//` and
/// `/* */`. Python sits in the AST tier and adds its module docstring.
/// Every other comment-bearing family reuses its lexicon row.
fn syntax_for(ext: &str) -> Option<HeaderSyntax> {
    if ext.eq_ignore_ascii_case("rs") || ext.eq_ignore_ascii_case("cs") {
        return Some(HeaderSyntax {
            line: "//",
            block: Some(("/*", "*/")),
            alone_markers: false,
            docstring: false,
            item_docs: true,
        });
    }
    if ext.eq_ignore_ascii_case("py") || ext.eq_ignore_ascii_case("pyi") {
        return Some(HeaderSyntax {
            line: "#",
            block: None,
            alone_markers: false,
            docstring: true,
            item_docs: false,
        });
    }
    let lexicon = super::lexicon_for(ext)?;
    Some(HeaderSyntax {
        line: lexicon.line,
        block: lexicon.block,
        alone_markers: lexicon.block_markers_alone,
        docstring: false,
        item_docs: false,
    })
}

/// Whether the text after a comment closer is empty or itself one
/// comment: whitespace, or a line comment starting with `line`.
fn tail_is_comment_or_empty(tail: &str, line: &str) -> bool {
    let tail = tail.trim_start();
    tail.is_empty() || tail.starts_with(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// Core header shapes per language family, with the header's line
    /// count and the follow-up code line.
    #[rstest]
    #[case::rust_module_docs("rs", "//! Docs.\n//! More.\nfn a() {}\n", 2)]
    #[case::rust_block_module_doc("rs", "/*! Module doc. */\nfn a() {}\n", 1)]
    #[case::rust_file_comments("rs", "// Note.\nfn a() {}\n", 1)]
    #[case::rust_item_docs_stay_code("rs", "/// Item doc.\nfn a() {}\n", 0)]
    #[case::rust_item_block_docs_stay_code("rs", "/** Item doc. */\nfn a() {}\n", 0)]
    #[case::rust_inner_attribute_stays_code("rs", "#![allow(dead_code)]\nfn a() {}\n", 0)]
    #[case::csharp_file_comments("cs", "// Note.\nclass A {}\n", 1)]
    #[case::csharp_block_header("cs", "/* Header. */\nclass A {}\n", 1)]
    #[case::python_comments_then_docstring("py", "# Note.\n\"\"\"Doc.\"\"\"\nx = 1\n", 2)]
    #[case::python_raw_docstring("py", "r\"\"\"Doc.\n\"\"\"\nx = 1\n", 2)]
    #[case::python_single_quote_docstring("pyi", "'''Doc.'''\nx = 1\n", 1)]
    #[case::python_indented_string_stays_code("py", "  \"\"\"Not module level.\"\"\"\n", 0)]
    #[case::python_docstring_only_follow_up_comments("py", "\"\"\"Doc.\"\"\"\n# body comment\n", 1)]
    #[case::python_assigned_string_stays_code("py", "X = \"\"\"value\"\"\"\n", 0)]
    #[case::shebang("sh", "#!/bin/sh\necho hi\n", 1)]
    #[case::sql_dash_comments("sql", "-- Note.\nSELECT 1;\n", 1)]
    #[case::sql_block_comment("sql", "/* Note. */\nSELECT 1;\n", 1)]
    #[case::lisp_semicolon_comments("el", "; Note.\n(message)\n", 1)]
    #[case::erlang_percent_comments("erl", "% Note.\nok().\n", 1)]
    #[case::haskell_block_pragma_shape("hs", "{- Note. -}\nmain = return ()\n", 1)]
    #[case::js_block_doc_header("js", "/** @file Tool entry. */\nvalue = 1;\n", 1)]
    #[case::js_line_doc_header("js", "/// note\nvalue = 1;\n", 1)]
    #[case::csharp_xml_item_docs_stay_code("cs", "/// Item doc.\nclass A {}\n", 0)]
    #[case::csharp_block_item_docs_stay_code("cs", "/** Item doc. */\nclass A {}\n", 0)]
    #[case::code_first("js", "value = 1;\n// later comment\n", 0)]
    #[case::unsupported_extension("md", "# Title\n\nText.\n", 0)]
    fn header_lines_should_recognize_each_family(
        #[case] ext: &str,
        #[case] source: &str,
        #[case] expected: usize,
    ) {
        assert_eq!(header_lines(source, ext), expected, "{source:?}");
    }

    /// A multi-line block close ends nothing: the header continues
    /// with the line comments after it, matching the single-line form.
    #[rstest]
    #[case::rust_multiline_then_comments("rs", "/*\n * Doc.\n */\n// Note.\nfn a() {}\n", 4)]
    #[case::rust_single_line_then_comments("rs", "/* Doc. */\n// Note.\nfn a() {}\n", 2)]
    #[case::sql_two_blocks("sql", "/* One. */\n-- Two.\nSELECT 1;\n", 2)]
    fn header_lines_should_continue_after_a_closed_block(
        #[case] ext: &str,
        #[case] source: &str,
        #[case] expected: usize,
    ) {
        assert_eq!(header_lines(source, ext), expected, "{source:?}");
    }

    /// Blank lines inside a header never count, and trailing blanks after
    /// the block stay counted as ordinary blanks.
    #[test]
    fn header_lines_should_skip_inner_blanks_without_counting_them() {
        // The inner blank belongs to the header block; the trailing one
        // stays an ordinary counted blank line.
        assert_eq!(header_lines("// Banner.\n\n// Second.\n", "js"), 3);
        assert_eq!(header_lines("// Banner.\n\n", "js"), 1);
    }

    /// A closer sharing its line with code counts that line and ends the
    /// header before it; the same holds for the docstring's closer.
    #[rstest]
    #[case::block_then_code("rs", "/* Header. */ fn a() {}\n", 0)]
    #[case::docstring_then_code("py", "\"\"\"Doc.\"\"\" x = 1\n", 0)]
    #[case::block_then_trailing_comment("js", "/* Header. */ // note\nvalue = 1;\n", 1)]
    #[case::multiline_close_then_code("js", "/*\n * Doc.\n */ value = 1;\n", 2)]
    fn header_lines_should_count_lines_mixing_the_closer_with_code(
        #[case] ext: &str,
        #[case] source: &str,
        #[case] expected: usize,
    ) {
        assert_eq!(header_lines(source, ext), expected, "{source:?}");
    }

    /// An unterminated block comment or docstring keeps its uncertain
    /// lines counted.
    #[rstest]
    #[case::unterminated_block("js", "// Fine.\n/* never closed\n", 1)]
    #[case::unterminated_docstring("py", "\"\"\"never closed\n", 0)]
    fn header_lines_should_fail_open_on_unterminated_regions(
        #[case] ext: &str,
        #[case] source: &str,
        #[case] expected: usize,
    ) {
        assert_eq!(header_lines(source, ext), expected, "{source:?}");
    }

    /// CRLF endings measure like LF ones, and a header-only file counts
    /// every one of its lines.
    #[test]
    fn header_lines_should_handle_crlf_and_header_only_files() {
        assert_eq!(header_lines("// Note.\r\nvalue = 1;\r\n", "js"), 1);
        assert_eq!(header_lines("// One.\n// Two.\n", "js"), 2);
        assert_eq!(header_lines("// No newline", "js"), 1);
    }

    /// Every lexicon family recognizes its own row's line marker as a
    /// header. A row wired to the wrong family stays silent on the
    /// mismatch, so pin all of them.
    #[test]
    fn header_lines_should_recognize_every_lexed_extension() {
        for (ext, lexicon) in super::super::families::LEXED_EXTENSIONS {
            let source = format!("{} note\nvalue = 1;\n", lexicon.line);
            assert_eq!(header_lines(&source, ext), 1, ".{ext}: its own marker");
        }
    }

    /// Unsupported extensions outside the lexicon and the `rs`/`cs`
    /// additions return no header.
    #[test]
    fn header_lines_should_return_zero_without_a_header_syntax() {
        for ext in ["md", "markdown", "txt", "json", "ini", "unknown", ""] {
            assert_eq!(header_lines("// anything\n", ext), 0, ".{ext}");
        }
    }
}
