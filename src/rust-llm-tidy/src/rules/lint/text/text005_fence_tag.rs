//! TEXT005: untagged fenced code block over the plaintext analysis.

use super::bulleted;
use crate::reporting::diagnostic::{Diagnostic, Severity};
use crate::rules::registry::CODE_FENCE_TAG;
use crate::text::measurement::{Document, Fence};

/// TEXT005 diagnostics for `doc`: one Warning per opening fence whose
/// info string is empty or exactly `ignore`, in source order.
///
/// Fires wherever markdown prose is measured: whole-file sources and
/// doc/comment regions. XML doc regions have no fences and never fire it.
pub(super) fn diagnostics(doc: &Document) -> Vec<Diagnostic> {
    doc.fences
        .iter()
        .filter(|fence| fence.info.is_empty() || *fence.info == *"ignore")
        .map(fence_diagnostic)
        .collect()
}

/// TEXT005 Warning for one untagged fence, reported at its opening line.
fn fence_diagnostic(fence: &Fence) -> Diagnostic {
    let (summary, bullets) = if fence.info.is_empty() {
        (
            "fenced code block has no language tag.".to_string(),
            [
                "Tag the fence with its language, like ```text.".to_string(),
                "Untagged blocks get no syntax highlighting.".to_string(),
            ],
        )
    } else {
        (
            "fenced code block uses bare `ignore`.".to_string(),
            [
                "Name the language: ```rust,ignore hides but still tags.".to_string(),
                "Bare `ignore` drops syntax highlighting and tooling.".to_string(),
            ],
        )
    };
    Diagnostic {
        severity: Severity::Warning,
        code: CODE_FENCE_TAG,
        message: bulleted(&summary, &bullets),
        line: fence.line,
        item_kind: "file".to_string(),
        item_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::run_region_checks;
    use crate::rules::lint::run_text_checks;
    use crate::rules::lint::tests::codes;
    use crate::text::measurement::{Dialect, DocRegion};
    use indoc::indoc;

    // ── Firing ──

    // A bare opening fence warns at its line; trailing whitespace and a
    // longer marker run still leave the info string empty.
    #[test]
    fn text_checks_warn_on_fence_without_a_language_tag() {
        let source = concat!(
            "```\ncode\n```\n\n",
            "~~~\ncode\n~~~\n\n",
            "```\t\ncode\n```\n\n",
            "````\ncode\n````\n",
        );
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_FENCE_TAG);
        assert_eq!(found.len(), 4);
        assert_eq!(found[0].line, 1);
        assert_eq!(found[1].line, 5);
        assert_eq!(found[2].line, 9);
        assert_eq!(found[3].line, 13);
        assert_eq!(found[0].severity, Severity::Warning);
        assert!(
            found[0]
                .message
                .starts_with("fenced code block has no language tag.")
        );
        assert!(found[0].message.contains("```text"));
    }

    // An opening fence with info string exactly `ignore` warns at its
    // line with the bare-`ignore` summary.
    #[test]
    fn text_checks_warn_on_fence_tagged_bare_ignore() {
        let source = "```ignore\nlet x = 1;\n```\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_FENCE_TAG);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
        assert!(
            found[0]
                .message
                .starts_with("fenced code block uses bare `ignore`.")
        );
        assert!(found[0].message.contains("```rust,ignore"));
    }

    // A fence left open at end of input still warns at its opening line.
    #[test]
    fn text_checks_warn_on_unclosed_untagged_fence() {
        let source = "```text\ncode\n```\ntail\n\n```\n";
        let diags = run_text_checks(source, "md");
        let found = codes(&diags, CODE_FENCE_TAG);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 6);
    }

    // ── Silence ──

    // Tagged fences and multi-token or case-variant info strings stay
    // silent; closing fences never fire.
    #[test]
    fn text_checks_silent_on_tagged_and_multi_token_fences() {
        let source = indoc! {"
            ```rust
            let x = 1;
            ```

            ```ignore,foo
            a
            ```

            ```ignore no_run
            b
            ```

            ```Ignore
            c
            ```
        "};
        assert!(codes(&run_text_checks(source, "md"), CODE_FENCE_TAG).is_empty());
    }

    // An indented 4-space code block is not a fence and never fires.
    #[test]
    fn text_checks_silent_on_indented_code_block() {
        let source = indoc! {"
            prose

                code line
        "};
        assert!(codes(&run_text_checks(source, "md"), CODE_FENCE_TAG).is_empty());
    }

    // A fence-like line inside an open code block closes it; it never
    // fires as an opening fence.
    #[test]
    fn text_checks_silent_on_fence_line_inside_an_open_block() {
        let source = indoc! {"
            ```text
            content
            ```
            after
        "};
        assert!(codes(&run_text_checks(source, "md"), CODE_FENCE_TAG).is_empty());
    }

    // ── Region tiers ──

    // Region tiers record fences too: a bare fence in a markdown-dialect
    // region warns like a whole-file one.
    #[test]
    fn region_checks_warn_on_bare_fences() {
        let diags = run_region_checks(vec![DocRegion {
            dialect: Dialect::Markdown,
            lines: vec![
                crate::text::measurement::RegionLine {
                    number: 1,
                    text: "```".to_string(),
                    indented: false,
                },
                crate::text::measurement::RegionLine {
                    number: 2,
                    text: "code".to_string(),
                    indented: false,
                },
                crate::text::measurement::RegionLine {
                    number: 3,
                    text: "```".to_string(),
                    indented: false,
                },
            ],
        }]);
        let found = codes(&diags, CODE_FENCE_TAG);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);
    }

    // A whole-file `rs` call warns on a bare Rust doc fence; a tagged
    // one stays silent.

    // Built with `concat!` so the bare fence lines stay string
    // fragments, not measured comment lines.
    #[test]
    fn text_checks_warn_for_rust_source_with_bare_doc_fences() {
        let bare_fence = concat!("``", "`");
        let bare = format!("/// {bare_fence}\n/// let x = 1;\n/// {bare_fence}\nfn f() {{}}\n");
        let bare_diags = run_text_checks(&bare, "rs");
        let found = codes(&bare_diags, CODE_FENCE_TAG);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 1);

        let tagged = indoc! {"
            /// ```rust
            /// let x = 1;
            /// ```
            fn f() {}
        "};
        assert!(codes(&run_text_checks(tagged, "rs"), CODE_FENCE_TAG).is_empty());
    }
}
