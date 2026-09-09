//! `DOC003` - `<exception>` tags naming no concrete exception type.

use super::Declaration;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_VAGUE_ERRORS;

/// `DOC003` - members that can throw need `<exception>` tags with a
/// concrete `cref` type.
///
/// Fires on non-private members that can throw, directly or through
/// same-file calls, whose `<exception>` tags all lack a concrete
/// `cref` value.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((tag_count, crefs)) = &decl.exception_scan else {
        return Vec::new();
    };
    if *tag_count == 0 || crefs.iter().any(|cref| !cref.trim().is_empty()) {
        return Vec::new();
    }

    vec![
        decl.diagnostic(
            Severity::Warning,
            CODE_VAGUE_ERRORS,
            "vague `# Errors` section",
            "`<exception>` doc tags name no concrete exception type (`cref`).\n\n\
         Why: Concrete exception types help readers connect failure conditions to handling code.\n\n\
         Suggestions:\n\
         - Verify the member and its calls.\n\
         - Set each `cref` to the actual exception type that can escape.\n\
         - Describe the specific condition that causes each exception.\n\
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
    fn diagnostic_should_request_verified_exception_types_and_conditions() {
        let parsed = parse(
            "class Cache {\n\
             /// <exception>Always thrown.</exception>\n\
             public void Load() { throw new System.InvalidOperationException(); }\n\
             }",
        )
        .unwrap();

        let diagnostics = super::super::run(&parsed);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == CODE_VAGUE_ERRORS)
            .unwrap();

        assert_eq!(
            diagnostic.message,
            "`<exception>` doc tags name no concrete exception type (`cref`).\n\n\
             Why: Concrete exception types help readers connect failure conditions to handling code.\n\n\
             Suggestions:\n\
             - Verify the member and its calls.\n\
             - Set each `cref` to the actual exception type that can escape.\n\
             - Describe the specific condition that causes each exception.\n\
             - Do not invent exceptions or change behavior to satisfy this lint."
        );
    }
}
