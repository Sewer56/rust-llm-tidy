//! `DOC001` - missing doc comments on non-private declarations.

use super::{DOCUMENTABLE, Declaration};
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_DOCS;

/// `DOC001` - non-private documentable declarations need a `///` doc
/// comment.
///
/// Fires on `public`, `internal`, and `protected`-family declarations of
/// documentable kinds that carry no `///` doc comment.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    if !decl.non_private || !DOCUMENTABLE.contains(&decl.kind) || !decl.docs.is_empty() {
        return Vec::new();
    }

    vec![
        decl.diagnostic(
            Severity::Error,
            CODE_MISSING_DOCS,
            "missing documentation",
            "non-private item is missing a doc comment.\n\n\
         Why: Readers need its purpose and contract without tracing the implementation.\n\n\
         Suggestions:\n\
         - Add `/// <summary>` docs describing its purpose and supported contract."
                .to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;

    #[test]
    fn diagnostic_should_request_purpose_and_supported_contract() {
        let parsed = parse("public class Cache {}").unwrap();

        let diagnostics = super::super::run(&parsed);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == CODE_MISSING_DOCS)
            .unwrap();

        assert_eq!(
            diagnostic.message,
            "non-private item is missing a doc comment.\n\n\
             Why: Readers need its purpose and contract without tracing the implementation.\n\n\
             Suggestions:\n\
             - Add `/// <summary>` docs describing its purpose and supported contract."
        );
    }
}
