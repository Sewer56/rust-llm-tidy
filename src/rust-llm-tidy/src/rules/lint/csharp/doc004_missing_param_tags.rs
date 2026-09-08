//! `DOC004` - missing `<param>` doc tags on parameterized members.

use super::Declaration;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_ARGUMENTS;

/// `DOC004` - members with parameters need `<param name="...">` tags.
///
/// Fires on non-private methods, constructors, and indexers that declare
/// parameters and whose docs carry no `<param>` tag.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((_, tags)) = &decl.param_scan else {
        return Vec::new();
    };
    if !tags.is_empty() {
        return Vec::new();
    }

    vec![
        decl.diagnostic(
            Severity::Warning,
            CODE_MISSING_ARGUMENTS,
            "member with parameters is missing `<param>` doc tags.\n\n\
         Why: Readers need parameter roles and constraints to supply appropriate inputs.\n\n\
         Suggestions:\n\
         - Add a `<param name=\"...\">` tag for each declared parameter, describing \
         its role and any constraints supported by the existing contract."
                .to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;

    #[test]
    fn diagnostic_should_request_parameter_roles_and_existing_constraints() {
        let parsed = parse("class Cache { public void Load(string key) {} }").unwrap();

        let diagnostics = super::super::run(&parsed);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == CODE_MISSING_ARGUMENTS)
            .unwrap();

        assert_eq!(
            diagnostic.message,
            "member with parameters is missing `<param>` doc tags.\n\n\
             Why: Readers need parameter roles and constraints to supply appropriate inputs.\n\n\
             Suggestions:\n\
             - Add a `<param name=\"...\">` tag for each declared parameter, describing \
             its role and any constraints supported by the existing contract."
        );
    }
}
