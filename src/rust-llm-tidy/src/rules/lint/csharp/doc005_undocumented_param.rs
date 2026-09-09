//! `DOC005` - `<param>` tags that omit declared parameters.

use super::Declaration;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_UNDOCUMENTED_PARAM;

/// `DOC005` - `<param>` tags must name every declared parameter.
///
/// Fires on parameterized members whose `<param>` tags omit at least one
/// declared parameter name.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((params, tags)) = &decl.param_scan else {
        return Vec::new();
    };
    if tags.is_empty() {
        return Vec::new();
    }
    let missing: Vec<&str> = params
        .iter()
        .map(String::as_str)
        .filter(|p| !tags.iter().any(|tag| tag == p))
        .collect();
    if missing.is_empty() {
        return Vec::new();
    }

    vec![decl.diagnostic(
        Severity::Warning,
        CODE_UNDOCUMENTED_PARAM,
        "undocumented parameter",
        format!(
            "parameter(s) not documented in `<param>` tags: `{}`.\n\n\
             Why: Omitted parameters leave readers guessing how to supply those inputs.\n\n\
             Suggestions:\n\
             - Add a `<param name=\"...\">` tag for each listed parameter, describing \
             its role and any constraints supported by the existing contract.",
            missing.join("`, `")
        ),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;

    #[test]
    fn diagnostic_should_request_contract_docs_for_each_missing_parameter() {
        let parsed = parse(
            "class Cache {\n\
             /// <param name=\"key\">The lookup key.</param>\n\
             public void Load(string key, string format, int limit) {}\n\
             }",
        )
        .unwrap();

        let diagnostics = super::super::run(&parsed);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == CODE_UNDOCUMENTED_PARAM)
            .unwrap();

        assert_eq!(
            diagnostic.message,
            "parameter(s) not documented in `<param>` tags: `format`, `limit`.\n\n\
             Why: Omitted parameters leave readers guessing how to supply those inputs.\n\n\
             Suggestions:\n\
             - Add a `<param name=\"...\">` tag for each listed parameter, describing \
             its role and any constraints supported by the existing contract."
        );
    }
}
