//! `DOC010` - XML doc tags out of canonical order.

use super::Declaration;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_SECTION_ORDER;

/// Canonical tag order: `inheritdoc`, `summary`, `typeparam`, `param`,
/// `returns`, `value`, `exception`, `remarks`, `example`, `seealso`.
const CANONICAL_ORDER: &str = "`inheritdoc`, `summary`, `typeparam`, `param`, `returns`, \
     `value`, `exception`, `remarks`, `example`, `seealso`";

/// `DOC010` - doc tags must follow the canonical reading order.
///
/// Fires once on any non-private declaration whose recognized doc tags
/// decrease in canonical rank anywhere: `inheritdoc`, `summary`,
/// `typeparam`, `param`, `returns`, `value`, `exception`, `remarks`,
/// `example`, `seealso`.
///
/// Tags outside the vocabulary are transparent, and equal ranks
/// (repeated same-name tags) pass.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    if !decl.non_private {
        return Vec::new();
    }

    // The rank of each recognized opening tag, in doc order; the first
    // adjacent decrease names the offending pair.
    let mut previous: Option<u8> = None;
    for doc in &decl.docs {
        let Some(rank) = tag_rank(doc) else {
            continue;
        };
        if let Some(before) = previous
            && rank < before
        {
            return vec![decl.diagnostic(
                Severity::Error,
                CODE_SECTION_ORDER,
                "XML doc tags out of canonical order",
                format!(
                    "XML doc tags out of canonical order: found `<{later}>` before \
                     `<{earlier}>`.\n\n\
                     Why: A consistent tag order is easier for the reader to review.\n\n\
                     Suggestions:\n\
                     - Move the tags into canonical order: {CANONICAL_ORDER}.",
                    earlier = canonical_tag(rank),
                    later = canonical_tag(before),
                ),
            )];
        }
        previous = Some(rank);
    }

    Vec::new()
}

/// The canonical tag name for a rank.
fn canonical_tag(rank: u8) -> &'static str {
    match rank {
        1 => "inheritdoc",
        2 => "summary",
        3 => "typeparam",
        4 => "param",
        5 => "returns",
        6 => "value",
        7 => "exception",
        8 => "remarks",
        9 => "example",
        _ => "seealso",
    }
}

/// The canonical rank of the opening XML doc tag starting `line`, or
/// `None` when the line starts with no recognized tag.
///
/// A tag counts only when `<name` starts the trimmed line and the
/// next character is whitespace, `>`, or `/`. Closing tags and
/// mid-line inline markup never match.
fn tag_rank(line: &str) -> Option<u8> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix('<')?;

    // The name ends at the first boundary; no boundary before the
    // line ends (a bare `<param`) does not count as a tag.
    let name_end = rest.find(|c: char| c == '>' || c == '/' || c.is_whitespace())?;
    match &rest[..name_end] {
        "inheritdoc" => Some(1),
        "summary" => Some(2),
        "typeparam" => Some(3),
        "param" => Some(4),
        "returns" => Some(5),
        "value" => Some(6),
        "exception" => Some(7),
        "remarks" => Some(8),
        "example" => Some(9),
        "seealso" => Some(10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;

    // ── DOC010: tag order ──

    fn diagnostics(source: &str) -> Vec<Diagnostic> {
        let parsed = parse(source).expect("test source must parse");
        super::super::run(&parsed)
            .into_iter()
            .filter(|diagnostic| diagnostic.code == CODE_SECTION_ORDER)
            .collect()
    }

    #[test]
    fn diagnostic_should_fire_with_exact_message_when_exception_precedes_param() {
        let source = "\
class C {\
    /// <summary>Saves.</summary>
    /// <exception cref=\"E\">When full.</exception>
    /// <param name=\"key\">The key.</param>
    public void Save(string key) { }
}
";

        let found = diagnostics(source);

        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].message,
            "XML doc tags out of canonical order: found `<exception>` before `<param>`.\n\n\
             Why: A consistent tag order is easier for the reader to review.\n\n\
             Suggestions:\n\
             - Move the tags into canonical order: `inheritdoc`, `summary`, \
             `typeparam`, `param`, `returns`, `value`, `exception`, `remarks`, \
             `example`, `seealso`."
        );
    }

    #[test]
    fn check_should_stay_silent_when_tags_follow_canonical_order() {
        let source = "\
class C {\
    /// <summary>Loads.</summary>
    /// <param name=\"key\">The key.</param>
    /// <returns>The value.</returns>
    /// <exception cref=\"E\">When missing.</exception>
    public string Load(string key) { return \"\"; }
}
";

        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn check_should_stay_silent_when_only_one_tag_is_recognized() {
        let source =
            "class C {\n    /// <summary>Only one.</summary>\n    public void One() { }\n}";

        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn check_should_stay_silent_when_unknown_tags_sit_between_recognized() {
        let source = "\
class C {\
    /// <summary>Custom docs.</summary>
    /// <permission>Admins only.</permission>
    /// <returns>One.</returns>
    public int Count() { return 1; }
}
";

        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn check_should_stay_silent_when_inline_markup_is_mid_line() {
        let source = "\
class C {\
    /// <summary>Loads.</summary>
    /// <param name=\"key\">The <paramref name=\"key\"/> to load.</param>
    /// <returns>The value.</returns>
    public string Load(string key) { return \"\"; }
}
";

        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn check_should_stay_silent_when_declaration_is_private() {
        let source = "\
class C {\
    /// <exception cref=\"E\">When full.</exception>
    /// <param name=\"key\">The key.</param>
    void Save(string key) { }
}
";

        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn check_should_stay_silent_when_value_follows_summary_on_property() {
        let source = "\
class C {\
    /// <summary>The size.</summary>
    /// <value>Bytes used.</value>
    public int Size { get; set; }
}
";

        assert!(diagnostics(source).is_empty());
    }

    #[test]
    fn check_should_stay_silent_when_multiline_tag_bodies_precede_later_tags() {
        let source = "\
class C {\
    /// <summary>Explains.</summary>
    /// <remarks>
    /// Spans multiple lines
    /// of remarks.
    /// </remarks>
    /// <example>Use it.</example>
    public void Go() { }
}
";

        assert!(diagnostics(source).is_empty());
    }

    // A self-closing tag counts; a bare `<param` at line end does not,
    // and the first canonical row (`inheritdoc`) participates.
    #[test]
    fn tag_rank_should_count_self_closing_tags_and_reject_bare_names() {
        assert_eq!(tag_rank("<inheritdoc/>"), Some(1));
        assert_eq!(tag_rank(" <param name=\"key\"/>"), Some(4));
        assert_eq!(tag_rank(" See <param"), None);
    }

    // The suggested order must be exactly the enforced rank order.
    #[test]
    fn canonical_order_should_list_all_ranks_in_rank_order() {
        let derived = format!(
            "`{}`",
            (1u8..=10)
                .map(canonical_tag)
                .collect::<Vec<_>>()
                .join("`, `")
        );
        assert_eq!(CANONICAL_ORDER, derived);
    }
}
