//! `DOC002` - missing `<exception>` doc tag on members that can throw.

use super::Declaration;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_ERRORS;

/// `DOC002` - members that can throw need an `<exception>` doc tag.
///
/// Fires on non-private methods and constructors that can throw and
/// whose docs carry no `<exception>` tag.
///
/// Throwing evidence is recursive: a `throw` in the member's own body
/// or a call to a same-file member that can throw, transitively.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((tag_count, _)) = &decl.exception_scan else {
        return Vec::new();
    };
    if *tag_count != 0 {
        return Vec::new();
    }

    vec![
        decl.diagnostic(
            Severity::Error,
            CODE_MISSING_ERRORS,
            "member that can throw is missing an `<exception>` doc tag.\n\n\
         Why: Readers need to understand possible failures and when they occur.\n\n\
         Suggestions:\n\
         - Trace its throws and calls.\n\
         - Document each exception that can escape with `<exception cref=\"Type\">` and the specific condition that causes it.\n\
         - Do not invent exceptions or change behavior to satisfy this lint."
                .to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;

    #[test]
    fn diagnostic_should_request_actual_escaping_exceptions_and_conditions() {
        let parsed = parse(
            "class Cache { public void Load() { throw new System.InvalidOperationException(); } }",
        )
        .unwrap();

        let diagnostics = super::super::run(&parsed);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == CODE_MISSING_ERRORS)
            .unwrap();

        assert_eq!(
            diagnostic.message,
            "member that can throw is missing an `<exception>` doc tag.\n\n\
             Why: Readers need to understand possible failures and when they occur.\n\n\
             Suggestions:\n\
             - Trace its throws and calls.\n\
             - Document each exception that can escape with `<exception cref=\"Type\">` and the specific condition that causes it.\n\
             - Do not invent exceptions or change behavior to satisfy this lint."
        );
    }
}
