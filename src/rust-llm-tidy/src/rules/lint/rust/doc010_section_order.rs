//! `DOC010` - doc sections listed out of canonical order.
//!
//! [`check`] fires when a public item's doc comments contain at least two
//! recognized section headers whose canonical rank decreases anywhere.
//! Unknown headers never participate, and repeated headers of the same
//! section are allowed.

use super::ARGUMENTS_HEADERS;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_SECTION_ORDER;
use crate::source::{SourceItem, VisibilityTier};

/// Canonical section order: `# Arguments`, `# Returns`, `# Examples`,
/// `# Errors`, `# Panics`, `# Safety`, `# Remarks`.
const CANONICAL_ORDER: &str = "`# Arguments`, `# Returns`, `# Examples`, \
     `# Errors`, `# Panics`, `# Safety`, `# Remarks`";
/// Recognized section headers (beyond the argument aliases) with their
/// canonical ranks.
const SECTION_HEADERS: &[(&str, u8)] = &[
    ("# returns", 2),
    ("# return", 2),
    ("# examples", 3),
    ("# example", 3),
    ("# errors", 4),
    ("# panics", 5),
    ("# safety", 6),
    ("# remarks", 7),
    ("# notes", 7),
    ("# note", 7),
];

/// `DOC010` - public items' doc sections must follow canonical order.
///
/// Fires on public items of any kind when the doc comments list
/// recognized section headers and a later header ranks below an
/// earlier one.
///
/// Headers match case-insensitively on the whole trimmed line, aliases
/// included. Exactly one diagnostic is reported per violating item,
/// naming the first offending adjacent pair by canonical header names.
///
/// Unrecognized headers are transparent, and equal-rank neighbors pass.
///
/// # Arguments
///
/// - `item` - the parsed source item whose doc comments are checked.
pub(super) fn check(item: &SourceItem) -> Vec<Diagnostic> {
    if item.visibility() != Some(VisibilityTier::Pub) {
        return Vec::new();
    }

    // Track the previous recognized rank and stop at the first decrease;
    // the offending pair is (previous, current) by rank.
    let mut prev_rank: Option<u8> = None;
    for line in item.doc_comments() {
        let Some(rank) = section_rank(line) else {
            continue;
        };
        if let Some(prev) = prev_rank
            && rank < prev
        {
            return vec![Diagnostic {
                title: Some("doc sections out of canonical order".into()),
                severity: Severity::Error,
                code: CODE_SECTION_ORDER,
                message: format!(
                    "sections out of canonical order: found `{}` before `{}`.\n\n\
                     Why: A consistent section order is easier for the reader to \
                     review.\n\n\
                     Suggestions:\n\
                     - Move the sections into canonical order: {CANONICAL_ORDER}.",
                    canonical_header(prev),
                    canonical_header(rank),
                ),
                line: item.start_line(),
                item_kind: item.kind().to_string(),
                item_name: item.name().map(str::to_string),
            }];
        }
        prev_rank = Some(rank);
    }
    Vec::new()
}

/// The canonical header name for a section rank.
fn canonical_header(rank: u8) -> &'static str {
    match rank {
        1 => "# Arguments",
        2 => "# Returns",
        3 => "# Examples",
        4 => "# Errors",
        5 => "# Panics",
        6 => "# Safety",
        _ => "# Remarks",
    }
}

/// Canonical rank of a doc-comment section header line, or `None` when
/// the line is not a recognized header.
///
/// The whole trimmed line must equal a known header string, matched
/// case-insensitively. Aliases share their section's rank; the six
/// `# Arguments` variants come from [`ARGUMENTS_HEADERS`].
fn section_rank(line: &str) -> Option<u8> {
    let t = line.trim();
    if ARGUMENTS_HEADERS.iter().any(|h| t.eq_ignore_ascii_case(h)) {
        return Some(1);
    }
    SECTION_HEADERS
        .iter()
        .find(|(header, _)| t.eq_ignore_ascii_case(header))
        .map(|(_, rank)| *rank)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::lint::rust::tests::parse_one;

    /// A documented `pub fn` whose docs contain `sections`.
    fn documented_fn(sections: &str) -> String {
        format!("/// Loads.\n///\n{sections}\npub fn load() {{}}\n")
    }

    /// Run DOC010 over the first item of `source`.
    fn lint(source: &str) -> Vec<Diagnostic> {
        check(&parse_one(source))
    }

    // ── DOC010: section order ──

    // Out-of-order pair fires with the exact message.
    #[test]
    fn check_should_report_offending_pair_when_errors_precedes_arguments() {
        let source = documented_fn(
            "/// # Errors\n///\n/// Fails sometimes.\n///\n/// # Arguments\n///\n/// - none",
        );

        let diags = lint(&source);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_SECTION_ORDER);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].line, parse_one(&source).start_line());
        assert_eq!(
            diags[0].message,
            "sections out of canonical order: found `# Errors` before `# Arguments`.\n\n\
             Why: A consistent section order is easier for the reader to review.\n\n\
             Suggestions:\n\
             - Move the sections into canonical order: `# Arguments`, `# Returns`, \
             `# Examples`, `# Errors`, `# Panics`, `# Safety`, `# Remarks`."
        );
    }

    // Canonical order (full run) is silent.
    #[test]
    fn check_should_stay_silent_when_sections_in_canonical_order() {
        let source = documented_fn(
            "/// # Arguments\n///\n/// - none\n///\n/// # Returns\n///\n/// Nothing.\n///\n\
             /// # Examples\n///\n/// Nothing.\n///\n/// # Errors\n///\n/// Never.\n///\n\
             /// # Panics\n///\n/// Never.\n///\n/// # Safety\n///\n/// Safe.\n///\n\
             /// # Remarks\n///\n/// None.",
        );
        assert!(lint(&source).is_empty());
    }

    // A subset in rank order is silent.
    #[test]
    fn check_should_stay_silent_when_optional_sections_are_ordered() {
        let source = documented_fn(
            "/// # Errors\n///\n/// Fails sometimes.\n///\n/// # Remarks\n///\n/// None.",
        );
        assert!(lint(&source).is_empty());
    }

    // Aliases map to their section's rank.
    #[test]
    fn check_should_use_alias_rank_when_comparing_sections() {
        // `# Params` (rank 1) before `# Errors` (rank 4): ordered.
        let source =
            documented_fn("/// # Params\n///\n/// - none\n///\n/// # Errors\n///\n/// Fails.");
        assert!(lint(&source).is_empty());

        // `# Errors` before `# Params`: fires, reporting the canonical name.
        let source =
            documented_fn("/// # Errors\n///\n/// Fails.\n///\n/// # Params\n///\n/// - none");
        let diags = lint(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.starts_with(
            "sections out of canonical order: found `# Errors` before `# Arguments`."
        ));
    }

    // Repeated headers of the same section never decrease.
    #[test]
    fn check_should_stay_silent_when_section_repeats() {
        let source = documented_fn(
            "/// # Errors\n///\n/// Fails.\n///\n/// # Errors\n///\n/// Fails again.",
        );
        assert!(lint(&source).is_empty());
    }

    // Unknown headers between recognized ones are ignored.
    #[test]
    fn check_should_stay_silent_when_unknown_header_lies_between_sections() {
        let source =
            documented_fn("/// # Errors\n///\n/// Fails.\n///\n/// # Ordering\n///\n/// Whatever.");
        assert!(lint(&source).is_empty());
    }

    // Private items are exempt.
    #[test]
    fn check_should_stay_silent_when_item_is_private() {
        let source =
            "/// # Errors\n///\n/// Fails.\n///\n/// # Arguments\n///\n/// - none\nfn load() {}\n";
        assert!(lint(source).is_empty());
    }

    // Any public item kind participates, not just fns.
    #[test]
    fn check_should_fire_when_pub_struct_docs_are_out_of_order() {
        let source = "/// A record.\n///\n/// # Remarks\n///\n/// None.\n///\n/// # Examples\n///\n/// None.\npub struct Record;\n";
        let diags = lint(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.starts_with(
            "sections out of canonical order: found `# Remarks` before `# Examples`."
        ));
    }

    // Header matching is case-insensitive.
    #[test]
    fn check_should_fire_when_headers_differ_only_in_case() {
        let source =
            documented_fn("/// # ERRORS\n///\n/// Fails.\n///\n/// # arguments\n///\n/// - none");
        assert_eq!(lint(&source).len(), 1);
    }

    // A single recognized header never violates the order.
    #[test]
    fn check_should_stay_silent_when_only_one_section_present() {
        let source = documented_fn("/// # Errors\n///\n/// Fails sometimes.");
        assert!(lint(&source).is_empty());
    }

    // The suggested order must be exactly the enforced rank order.
    #[test]
    fn canonical_order_should_list_all_ranks_in_rank_order() {
        let derived = format!(
            "`{}`",
            (1u8..=7)
                .map(canonical_header)
                .collect::<Vec<_>>()
                .join("`, `")
        );
        assert_eq!(CANONICAL_ORDER, derived);
    }
}
