//! Fail-closed comment lexicon: doc regions for the DOC007/DOC008 text
//! checks of the `//`, `#`, `--`, `;`, and `%` comment families.
//!
//! [`text_checks`] walks the raw source once, tracking line-comment
//! markers, block-comment pairs, and the family's multi-line string
//! forms, and emits two kinds of [`DocRegion`] for the lint crate's
//! measuring core:
//!
//! - contiguous runs of standalone line comments, measured as markdown
//!   prose with the full marker run (`///`, `##`, `;;;`) and one space
//!   stripped;
//! - every block comment (`/* */`, `--[[ ]]`, `{- -}`, `#| |#`, and
//!   MATLAB's line-alone `%{ %}`), measured with the block doc dialect
//!   (`*` continuations stripped, `@tag` names exempt).
//!
//! String content, heredoc payload, and code lines never measure: they
//! are not comments. A trailing comment (code before the marker) is its
//! own region, so fragments on consecutive lines never pool into one
//! paragraph.
//!
//! # Fail-closed
//!
//! Anything ambiguous rejects the whole scan: zero findings, never
//! guessed measurement ([`text_checks`] returns empty). Ambiguous
//! sources:
//!
//! - nested literals inside a template `${...}` hole, or a hole reaching
//!   the line's end;
//! - a nested block opener inside an open block comment (Swift,
//!   Haskell, Elm, and Scheme nest; C, Lua, SQL, and MATLAB do not);
//! - a bare Ruby `<<word` heredoc opener (ambiguous with `arr << item`),
//!   a Ruby percent literal, a PHP `<<<` heredoc, a C++ `R"` raw string,
//!   or a Swift `#"""` raw text block;
//! - a PostgreSQL dollar-quoted string (`$$`, `$tag$`), a Lua long
//!   bracket (`[[`, `[=[`), a Haskell quasiquote (`[name|`, `[|`), or
//!   an Erlang `$%`/`$\%` character literal;
//! - a Lisp datum comment (`#;`) or semicolon character literal
//!   (`#\;`, `?;`, `?\;`, `\;`), or TeX verbatim material (`\verb`,
//!   verbatim-like environments);
//! - a file ending inside an open block comment, backtick literal,
//!   triple-quoted string, carried quote, or heredoc.
//!
//! An unterminated quote whose literal legally spans the line
//! (script-family strings, or a backslash-newline continuation) carries
//! its state: the lines until the closing quote stay string content.
//!
//! A file ending inside a carried quote rejects the scan. Any other
//! unterminated quote is invalid source and closes at the line's end:
//! its desync never crosses the line.
//!
//! The word-start families (POSIX `#` rules, Ruby after a token) open
//! comments only at the start of a word, so regex literals and mid-word
//! markers stay code.
//!
//! Regex literals are otherwise unmodeled: a `//`-family pattern
//! carrying comment-looking text can misattribute its tail.
//!
//! The `--` marker follows PostgreSQL and always comments in SQL;
//! MySQL `#` comments and `a--b` double negation are not modeled. An
//! unbalanced `'` in the single-quote families (SQL, Lua, MATLAB,
//! Erlang) hides its own line's trailing comment: silence, never
//! measurement.
//!
//! [`DocRegion`]: rust_llm_tidy_lint::check::DocRegion

//!
//! # Layout
//!
//! - `families` - the per-family lexical tables, the fail-closed
//!   reject predicates, and the extension lookup.
//! - `scan` - the fail-closed scanner.

use families::{LEXED_EXTENSIONS, Lexicon};
use rust_llm_tidy_lint::Diagnostic;
use rust_llm_tidy_lint::check::run_region_checks;
use std::cmp::Ordering;

mod families;
mod scan;

/// Whether `ext` has a lexicon entry: the `//`, `#`, `--`, `;`, and `%`
/// comment families.
///
/// The admission registry's `Lexicon` tier and this table must agree per
/// extension; consumers use this to pin the two in lockstep.
///
/// # Arguments
///
/// - `ext`: a path extension without the leading dot, matched
///   ASCII case-insensitively.
#[inline]
pub fn covers(ext: &str) -> bool {
    lexicon_for(ext).is_some()
}

/// Runs the DOC007/DOC008 text checks over `source`'s comments, as lexed
/// for `ext`'s comment family.
///
/// Extensions without a lexicon entry and ambiguous sources (see the
/// module docs) produce no findings.
///
/// # Arguments
///
/// - `source`: the file's raw text.
/// - `ext`: the file extension without the leading dot, matched
///   ASCII case-insensitively.
///
/// # Returns
///
/// Diagnostics in source order: DOC007 per over-limit paragraph, then
/// DOC008 per over-limit line.
pub fn text_checks(source: &str, ext: &str) -> Vec<Diagnostic> {
    match lexicon_for(ext).and_then(|lex| scan::scan(source, lex)) {
        Some(regions) => run_region_checks(regions),
        None => Vec::new(),
    }
}

/// The lexicon for `ext`, ASCII case-insensitively (`.JS` resolves like
/// `.js`), or `None` outside the lexicon families.
fn lexicon_for(ext: &str) -> Option<&'static Lexicon> {
    let idx = LEXED_EXTENSIONS
        .binary_search_by(|(probe, _)| cmp_ext(probe, ext))
        .ok()?;
    Some(LEXED_EXTENSIONS[idx].1)
}

/// ASCII case-insensitive ordering, matching the CLI admission registry's
/// extension comparisons.
fn cmp_ext(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_llm_tidy_lint::check::{CODE_LINE_LENGTH, CODE_PARAGRAPH_SIZE};

    /// The diagnostics carrying `code`.
    fn codes<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
        diags.iter().filter(|d| d.code == code).collect()
    }

    /// Five `marker`-prefixed lines whose joined prose exceeds the 240
    /// budget while each line stays under 80.
    fn long_comment(marker: &str) -> String {
        let line = "filler words pad the paragraph past the two hundred forty limit";
        (0..5).map(|_| format!("{marker} {line}\n")).collect()
    }

    /// Block-doc lines whose joined prose (stars stripped) exceeds the
    /// 240 budget while each raw line stays under 80.
    fn long_block(open: &str, close: &str) -> String {
        let line = "* filler words pad the paragraph past the two hundred forty limit";
        let mut out = format!("{open}\n");
        for _ in 0..5 {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(close);
        out.push('\n');
        out
    }

    // ── True positives ──

    /// Standalone `//`, `#`, `--`, `;`, and `%` comment prose measures
    /// as one paragraph at the paragraph's first line, for every family
    /// extension.
    #[test]
    fn line_comment_prose_measures_per_family() {
        for (marker, ext) in [
            ("//", "js"),
            ("//", "mjs"),
            ("//", "tsx"),
            ("#", "py"),
            ("#", "pyi"),
            ("#", "bash"),
            ("--", "sql"),
            ("--", "lua"),
            ("--", "hs"),
            ("--", "elm"),
            ("--", "ada"),
            (";", "el"),
            (";", "clj"),
            ("%", "tex"),
            ("%", "erl"),
            ("%", "m"),
        ] {
            let diags = text_checks(&long_comment(marker), ext);
            let found = codes(&diags, CODE_PARAGRAPH_SIZE);
            assert_eq!(found.len(), 1, ".{ext}: exactly the comment paragraph");
            assert_eq!(found[0].line, 1, ".{ext}: the first comment line");
        }
    }

    /// Block comments measure with the block doc dialect: `*`
    /// continuations strip, in doc (`/**`) and plain (`/*`) blocks alike.
    #[test]
    fn block_comments_measure_with_the_block_dialect() {
        for open in ["/**", "/*"] {
            let source = format!("{}\nlet quiet = 1;\n", long_block(open, "*/"));
            let diags = text_checks(&source, "js");
            let found = codes(&diags, CODE_PARAGRAPH_SIZE);
            assert_eq!(found.len(), 1, "{open}: the block paragraph fires");
            assert_eq!(found[0].line, 2, "{open}: the first prose line");
        }
    }

    /// The dash, semicolon, and percent families' block forms measure
    /// with the block doc dialect:
    /// SQL `/* */`, Lua `--[[ ]]`, Haskell and Elm `{- -}`, Lisp `#| |#`,
    /// and MATLAB `%{ %}`.
    #[test]
    fn dash_semi_percent_block_forms_measure_with_the_block_dialect() {
        for (ext, open, close) in [
            ("sql", "/*", "*/"),
            ("lua", "--[[", "]]"),
            ("hs", "{-", "-}"),
            ("elm", "{-", "-}"),
            ("scm", "#|", "|#"),
            ("m", "%{", "%}"),
        ] {
            let source = format!("{}\nquiet = 1;\n", long_block(open, close));
            let diags = text_checks(&source, ext);
            let found = codes(&diags, CODE_PARAGRAPH_SIZE);
            assert_eq!(found.len(), 1, ".{ext}: the block paragraph fires");
            assert_eq!(found[0].line, 2, ".{ext}: the first prose line");
        }
    }

    /// Marker runs (`----`, `;;`, `%%`) open one comment whose prose
    /// measures, not a nested or escaped form.
    #[test]
    fn doubled_markers_stay_one_comment() {
        for (marker, ext) in [("----", "ada"), (";;", "el"), ("%%", "erl")] {
            let diags = text_checks(&long_comment(marker), ext);
            let found = codes(&diags, CODE_PARAGRAPH_SIZE);
            assert_eq!(found.len(), 1, ".{ext}: exactly the comment paragraph");
            assert_eq!(found[0].line, 1, ".{ext}: the first comment line");
        }
    }

    /// MATLAB block markers comment only alone on their lines: a
    /// mid-line or non-alone `%{` is an ordinary `%` comment (the code
    /// lines after it never measure), and a mid-line `%}` does not
    /// close a real block.
    #[test]
    fn matlab_block_markers_comment_only_alone() {
        let tail = "m".repeat(85);
        let midline = format!("x = 1; %{{ opener note\ny = 2 + {tail};\n%}}\n");
        let nonalone = format!("%{{ header note text\ny = 2 + {tail};\n%}}\n");
        for source in [midline, nonalone] {
            assert!(
                text_checks(&source, "m").is_empty(),
                "code after a non-alone `%{{` must never measure"
            );
        }

        // An alone `%{` opens the block, and only an alone `%}` closes.
        let block = format!("%{{\n* note with a %}} that must not close\n* {tail}\n%}}\n");
        let diags = text_checks(&block, "m");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(
            found.len(),
            1,
            "the block measures and runs to the alone closer"
        );
        assert_eq!(found[0].line, 3, "the over-long prose line keeps its line");
    }

    /// A long line comment warns on DOC008 at its own line.
    #[test]
    fn long_comment_line_warns_doc008() {
        let source = format!("// {}\n", "x".repeat(81));
        let diags = text_checks(&source, "go");
        let found = codes(&diags, CODE_LINE_LENGTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
    }

    /// Trailing comment fragments never join: consecutive long fragments
    /// pool no paragraph.
    #[test]
    fn trailing_comment_fragments_never_join_paragraphs() {
        let tail = "t".repeat(130);
        let source = format!("let a = 1; // {tail}\nlet b = 2; // {tail}\n");
        let diags = text_checks(&source, "js");
        assert!(
            codes(&diags, CODE_PARAGRAPH_SIZE).is_empty(),
            "fragments must not pool: {diags:?}"
        );
    }

    /// CRLF endings strip from the measured text: an exactly-80 comment
    /// line stays quiet where a surviving CR would tip it over.
    #[test]
    fn crlf_endings_strip_from_measured_text() {
        let source = format!("// {}\r\n", "y".repeat(80));
        let diags = text_checks(&source, "js");
        assert!(codes(&diags, CODE_LINE_LENGTH).is_empty());
    }

    // ── String immunity ──

    /// Single-line string content carrying comment markers never measures.
    #[test]
    fn marker_text_inside_strings_stays_quiet() {
        let tail = "s".repeat(85);
        let cases = [
            ("js", format!("const s = \"// not a comment: {tail}\";\n")),
            ("sh", format!("echo \"# not a comment: {tail}\"\n")),
            ("rb", format!("puts '# not a comment: {tail}'\n")),
            (
                "c",
                format!("const char* s = \"// not a comment: {tail}\";\n"),
            ),
        ];
        for (ext, source) in cases {
            assert!(
                text_checks(&source, ext).is_empty(),
                ".{ext}: string content must stay quiet"
            );
        }
    }

    /// Comment-marker text inside the dash, semicolon, and percent
    /// families' string literals and atoms never measures.
    #[test]
    fn marker_text_in_dash_semi_percent_strings_stays_quiet() {
        let tail = "s".repeat(85);
        let cases = [
            (
                "sql",
                format!("SELECT '-- not a comment: {tail}' FROM t;\n"),
            ),
            ("lua", format!("local s = \"-- not a comment: {tail}\"\n")),
            ("hs", format!("s = \"-- not a comment: {tail}\"\n")),
            ("elm", format!("s = \"-- not a comment: {tail}\"\n")),
            ("m", format!("s = '-- not a comment: {tail}';\n")),
            ("erl", format!("S = \"% not a comment: {tail}\".\n")),
            ("erl", format!("A = '% not a comment: {tail}'.\n")),
            ("clj", format!("(def s \"; not a comment: {tail}\")\n")),
            ("el", format!("(setq s \"; not a comment: {tail}\")\n")),
        ];
        for (ext, source) in cases {
            assert!(
                text_checks(&source, ext).is_empty(),
                ".{ext}: string content must stay quiet"
            );
        }
    }

    /// Quoted strings that legally span lines stay string content: native
    /// spans in the script families, backslash-newline continuations, and
    /// Zig line strings.
    #[test]
    fn multi_line_strings_stay_quiet() {
        let payload = concat!(
            "# payload-looking span line padding far past both the line and paragraph budget limits\n",
            "# payload-looking span line padding far past both the line and paragraph budget limits\n",
            "# payload-looking span line padding far past both the line and paragraph budget limits\n",
        );
        let cases = [
            ("sh", format!("MSG=\"usage: tool takes a file\n{payload}  end\";\n")),
            ("sh", format!("MSG='usage: tool takes a file\n{payload}  end';\n")),
            ("rb", format!("msg = \"usage: tool takes a file\n{payload}  end\"\n")),
            (
                "js",
                concat!(
                    "const s = \"first part of the string continues \\\\\n",
                    "// second line of the same string reached through a backslash newline continuation tail\n",
                    "third part\";\n",
                )
                .to_string(),
            ),
            (
                "zig",
                concat!(
                    "const s =\n",
                    "    \\\\ see the // marker plus a long tail of line string padding that runs past the eighty char limit\n",
                    "    \\\\ past the eighty char limit for the whole line budget of the check\n",
                )
                .to_string(),
            ),
        ];
        for (ext, source) in cases {
            assert!(
                text_checks(&source, ext).is_empty(),
                ".{ext}: spanned string content must stay quiet"
            );
        }
    }

    /// Strings that legally span lines stay string content in the
    /// dash and semicolon families too: Elm and Lisp native spans,
    /// and a Haskell string gap (a `\` at the line's end).
    #[test]
    fn spanned_dash_and_semi_strings_stay_quiet() {
        let payload: &str = "-- payload-looking span line padding far past both the line and paragraph budget limits\n";
        let cases = [
            ("elm", format!("s = \"\"\"starts here\n{payload}  ends here\"\"\"\n")),
            ("clj", format!("(def s \"starts here\n{payload}  ends here\")\n")),
            (
                "hs",
                concat!(
                    "s = \"starts here \\\n",
                    "-- continued string payload padding far past both the line and paragraph budgets\n",
                    "  ends here\"\n",
                )
                .to_string(),
            ),
        ];
        for (ext, source) in cases {
            assert!(
                text_checks(&source, ext).is_empty(),
                ".{ext}: spanned string content must stay quiet"
            );
        }
    }

    /// A spanned string closes on its own line and measurement resumes:
    /// the comment paragraph after it still fires.
    #[test]
    fn multi_line_string_close_resumes_measurement() {
        let span = concat!(
            "MSG=\"usage: tool takes a file\n",
            "# payload-looking span line padding far past both the line and paragraph budget limits\n",
            "  end\";\n",
        );
        let source = format!("{span}{}", long_comment("#"));
        let diags = text_checks(&source, "sh");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1, "only the real comment paragraph");
        assert_eq!(found[0].line, 4);
    }

    /// Mid-word `#` and regex literals stay code in the word-start
    /// families, while a word-start comment still measures.
    #[test]
    fn mid_word_hash_stays_code() {
        let tail = "b".repeat(85);
        let quiet = [
            ("sh", format!("echo file#{tail}\n")),
            (
                "rb",
                format!("HASHTAG = /#[a-z0-9_]+(?:-[a-z0-9_]+)*$/.match({tail}) if tag\n"),
            ),
        ];
        for (ext, source) in quiet {
            assert!(
                text_checks(&source, ext).is_empty(),
                ".{ext}: mid-word marker must stay code"
            );
        }
        let tail = "w".repeat(85);
        let measured = format!("value = 1 # a real trailing comment {tail}\n");
        let diags = text_checks(&measured, "sh");
        assert_eq!(
            codes(&diags, CODE_LINE_LENGTH).len(),
            1,
            "a word-start comment still measures"
        );
    }

    /// Apostrophes and Lisp `?`-suffixed names are punctuation, not
    /// string opens or character literals, so the trailing comment
    /// stays measurable.
    #[test]
    fn punctuation_apostrophes_keep_trailing_comments_measurable() {
        let tail = "t".repeat(85);
        let cases = [
            ("ada", format!("Y := X'First; -- trailing note {tail}\n")),
            ("hs", format!("x' = x' + 1 -- trailing note {tail}\n")),
            ("clj", format!("(def y '(1 2 3)) ; trailing note {tail}\n")),
            ("clj", format!("(odd?; trailing note {tail})\n")),
        ];
        for (ext, source) in cases {
            let diags = text_checks(&source, ext);
            assert_eq!(
                codes(&diags, CODE_LINE_LENGTH).len(),
                1,
                ".{ext}: the trailing comment must still measure"
            );
        }
    }

    /// A `\\` pair mid-line is not a Zig line string: the trailing
    /// comment on that line still measures.
    #[test]
    fn mid_line_backslashes_keep_trailing_comments_measurable() {
        let tail = "c".repeat(85);
        let source = format!("echo a\\\\ # {tail}\n");
        let diags = text_checks(&source, "sh");
        assert_eq!(
            codes(&diags, CODE_LINE_LENGTH).len(),
            1,
            "the trailing comment after a mid-line backslash pair must still measure"
        );
    }

    /// Template literal content is immune, while real comments in the
    /// same file still measure.
    #[test]
    fn template_literal_content_stays_quiet() {
        let source = format!(
            "const t = `// {}\n// {}`;\nconst g = `Hello ${{name}}!`;\n{}\n",
            "a".repeat(70),
            "a".repeat(70),
            long_comment("//")
        );
        let diags = text_checks(&source, "js");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1, "only the real comment paragraph");
        assert_eq!(
            found[0].line, 4,
            "the benign ${{}} hole must not reject the scan"
        );
    }

    /// Heredoc payload is immune across the marked shell and Ruby forms,
    /// while real comments in the same file still measure.
    #[test]
    fn heredoc_payload_stays_quiet() {
        let cases = [
            (
                "sh",
                concat!(
                    "cat <<EOF\n",
                    "# filler words pad the payload far past the budget limit here\n",
                    "# more filler words padding the payload past the budget\n",
                    "EOF\n",
                ),
            ),
            (
                "sh",
                concat!(
                    "cat <<-'MSG'\n",
                    "# filler words pad the payload far past the budget limit here\n",
                    "MSG\n",
                ),
            ),
            (
                "sh",
                concat!(
                    "cat << \"MSG\"\n",
                    "# filler words pad the payload far past the budget limit here\n",
                    "MSG\n",
                ),
            ),
            (
                "rb",
                concat!(
                    "text = <<~PAYLOAD\n",
                    "# filler words pad the payload far past the budget limit here\n",
                    "PAYLOAD\n",
                ),
            ),
        ];
        for (ext, heredoc) in cases {
            let source = format!("{heredoc}{}", long_comment("#"));
            let diags = text_checks(&source, ext);
            let found = codes(&diags, CODE_PARAGRAPH_SIZE);
            assert_eq!(found.len(), 1, ".{ext}: only the real comment paragraph");
        }
    }

    /// Triple-quoted string content is immune and `<<` stays an operator,
    /// while real comments in the same file still measure.
    #[test]
    fn triple_quoted_strings_stay_quiet() {
        let payload = concat!(
            "s = \"\"\"\n",
            "# filler words pad the payload far past the budget limit\n",
            "\"\"\"\n",
            "t = '''\n",
            "# filler words pad the payload far past the budget limit\n",
            "'''\n",
            "mask = 1 << n\n",
        );
        let source = format!("{payload}{}", long_comment("#"));
        let diags = text_checks(&source, "py");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1, "only the real comment paragraph");
        assert_eq!(found[0].line, 8);
    }

    /// `$#` and `${#x}` are parameter syntax: a long line carrying them
    /// warns nowhere.
    #[test]
    fn dollar_hash_is_parameter_syntax_not_a_comment() {
        let tail = "p".repeat(85);
        let source = format!("if [ $# -eq 0 ]; then echo \"{tail}\"; fi\nn=${{#{tail}}}\n");
        let diags = text_checks(&source, "sh");
        assert!(
            codes(&diags, CODE_LINE_LENGTH).is_empty(),
            "parameter syntax must stay quiet: {diags:?}"
        );
    }

    // ── Fail-closed ──

    /// Ambiguous sources reject the scan: an over-budget comment in the
    /// same file produces zero findings.
    #[test]
    fn ambiguous_sources_fail_closed() {
        let cases = [
            // Nested literals inside template holes.
            ("js", "const t = `${f(\"inner\")}`;\n"),
            ("js", "const t = `${g(`nested`)}`;\n"),
            // A hole reaching the line's end.
            ("js", "const t = `${value\n."),
            // A nested block-comment opener.
            ("c", "/* outer /* inner\n*/\n"),
            // Unterminated multi-line literals and heredocs.
            ("c", "/* never closed\n"),
            ("js", "const t = `never closed\n"),
            ("py", "s = \"\"\"\n"),
            ("sh", "cat <<EOF\npayload\n"),
            // Unterminated quotes whose literal spans lines.
            ("sh", "MSG=\"never closed\n"),
            ("rb", "msg = \"never closed\n"),
            ("js", "const s = \"continued \\\n"),
            // Ruby percent literals.
            ("rb", "words = %w[a b]\n"),
            ("rb", "q = %Q(padded text)\n"),
        ];
        for (ext, probe) in cases {
            let source = format!(
                "{probe}{}",
                long_comment(if ext == "js" || ext == "c" { "//" } else { "#" })
            );
            assert!(
                text_checks(&source, ext).is_empty(),
                ".{ext}: ambiguous source must fail closed: {probe:?}"
            );
        }
    }

    /// C++ raw strings, PHP heredocs, Swift raw text blocks, and bare
    /// Ruby heredoc openers reject the scan.
    #[test]
    fn raw_literal_families_fail_closed() {
        let cases = [
            ("cpp", "auto s = R\"(// raw)\";\n"),
            ("php", "$s = <<<EOT\npayload\nEOT;\n"),
            ("swift", "let s = #\"\"\"\nraw\n\"\"\"#\n"),
            ("rb", "text = <<EOS\npayload\nEOS\n"),
        ];
        for (ext, probe) in cases {
            let marker = if ext == "rb" { "#" } else { "//" };
            let source = format!("{probe}{}", long_comment(marker));
            assert!(
                text_checks(&source, ext).is_empty(),
                ".{ext}: raw literal must fail closed: {probe:?}"
            );
        }
    }

    /// The dash, semicolon, and percent families' unmodeled forms
    /// reject the scan: SQL dollar quotes, Lua long brackets, Haskell
    /// quasiquotes, nested block comments, Lisp reader semicolons,
    /// TeX verbatim material, and the Erlang percent character.
    #[test]
    fn dash_semi_percent_ambiguities_fail_closed() {
        let cases = [
            ("sql", "SELECT $$\npayload\n$$;\n"),
            ("sql", "SELECT $tag$\npayload\n$tag$;\n"),
            ("lua", "local s = [[payload]]\n"),
            ("hs", "v = [q|payload|]\n"),
            ("hs", "{- outer {- inner -} -}\n"),
            ("elm", "{- outer {- inner -} -}\n"),
            ("scm", "#;(payload)\n"),
            ("el", "(char-upcase #\\;)\n"),
            ("el", "(setq sep ?\\;) tail\n"),
            ("el", "(setq sep ?;) tail\n"),
            ("el", "(setq q '?;) tail\n"),
            ("clj", "(def c \\;) tail\n"),
            ("el", "#| outer #| inner |# |#\n"),
            ("tex", "\\begin{verbatim}\npayload\n\\end{verbatim}\n"),
            ("tex", "x = \\verb|%| y\n"),
            ("erl", "C = $%,\n"),
            ("erl", "C = $\\%, tail\n"),
        ];
        for (ext, probe) in cases {
            let marker = match ext {
                "sql" | "lua" | "hs" | "elm" | "ada" => "--",
                "tex" | "erl" | "m" => "%",
                _ => ";",
            };
            let source = format!("{probe}{}", long_comment(marker));
            assert!(
                text_checks(&source, ext).is_empty(),
                ".{ext}: ambiguous source must fail closed: {probe:?}"
            );
        }
    }

    /// TeX: an odd-length backslash run escapes the marker (it prints
    /// literally), while an even-length run is `\\` commands and the
    /// marker still comments.
    #[test]
    fn backslash_runs_escape_the_tex_marker() {
        let tail = "e".repeat(85);
        let escaped = format!("The rate is 100\\% of {tail}\n");
        assert!(
            text_checks(&escaped, "tex").is_empty(),
            "an escaped marker must not open a comment"
        );
        let commented = format!("x \\\\ % trailing note {tail}\n");
        assert_eq!(
            codes(&text_checks(&commented, "tex"), CODE_LINE_LENGTH).len(),
            1,
            "an even backslash run must still comment"
        );
    }

    /// Ruby's push operator and bit shifts are not heredocs: the scan
    /// survives them and comments still measure.
    #[test]
    fn ruby_operators_do_not_reject_the_scan() {
        let source = format!(
            "list << item\nmask = n << 3 if n < 4\n{}",
            long_comment("#")
        );
        let diags = text_checks(&source, "rb");
        let found = codes(&diags, CODE_PARAGRAPH_SIZE);
        assert_eq!(found.len(), 1, "the comment paragraph still fires");
        assert_eq!(found[0].line, 3);
    }

    // ── Table coverage ──

    /// The table covers the five comment families case-insensitively and
    /// nothing else.
    #[test]
    fn covers_matches_the_extension_table() {
        for (ext, _) in LEXED_EXTENSIONS {
            assert!(covers(ext), ".{ext} must be covered");
        }
        assert!(covers("JS"), "lookup is case-insensitive");
        assert!(covers("SQL"), "dash-family lookup is case-insensitive");
        for ext in ["rs", "cs", "md", "org", ""] {
            assert!(!covers(ext), ".{ext} must not be covered");
        }
    }
}
