//! `DOC011` - missing `<returns>` tags on value-returning methods.

use super::Declaration;
use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MISSING_RETURNS;
use crate::source::ReturnKind;

/// `DOC011` - non-private methods returning a value need a `<returns>`
/// tag.
///
/// Fires on non-private methods whose declared return type is not
/// `void` and whose docs carry no `<returns>` tag. `bool` returns only
/// remind: a good summary often already covers both outcomes.
pub(super) fn check(decl: &Declaration<'_>) -> Vec<Diagnostic> {
    let Some((kind, has_tag)) = decl.returns else {
        return Vec::new();
    };
    if has_tag {
        return Vec::new();
    }

    let (severity, message) = match kind {
        ReturnKind::Bool => (
            Severity::Reminder,
            "Method returns `bool` but has no `<returns>` tag.\n\n\
             Why: Readers should not have to inspect the implementation to understand what `true` \
             and `false` mean.\n\n\
             Suggestions:\n\
             - No change is needed if the method name or summary already makes both outcomes \
             clear.\n\
             - Otherwise, consider a short `<returns>` tag explaining when the method returns \
             `true` and when it returns `false`."
                .to_string(),
        ),
        ReturnKind::Value => (
            Severity::Warning,
            "Method returns a value but has no `<returns>` tag.\n\n\
             Why: Readers need to understand what the returned value represents, not just its \
             type.\n\n\
             Suggestions:\n\
             - Add a `<returns>` tag explaining what the value represents and any special cases \
             callers need to handle.\n\
             - Describe only existing behavior. Do not invent guarantees or change the \
             implementation to satisfy this lint."
                .to_string(),
        ),
        ReturnKind::NoValue | ReturnKind::SelfValue | ReturnKind::ResultUnit => return Vec::new(),
    };

    vec![decl.diagnostic(
        severity,
        CODE_MISSING_RETURNS,
        "missing `<returns>` tag",
        message,
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::csharp::parse::parse;

    /// The DOC011 finding for `source`, if any.
    fn doc011(source: &str) -> Option<Diagnostic> {
        let parsed = parse(source).unwrap();
        super::super::run(&parsed)
            .into_iter()
            .find(|diagnostic| diagnostic.code == CODE_MISSING_RETURNS)
    }

    // ── warnings ──

    #[test]
    fn value_returns_should_warn_when_returns_tag_is_missing() {
        let diagnostic =
            doc011("class C { public int Get() { return 0; } }").expect("value return warns");

        assert_eq!(diagnostic.severity, Severity::Warning);
        assert_eq!(diagnostic.title.as_deref(), Some("missing `<returns>` tag"));
        assert_eq!(
            diagnostic.message,
            "Method returns a value but has no `<returns>` tag.\n\n\
             Why: Readers need to understand what the returned value represents, not just its \
             type.\n\n\
             Suggestions:\n\
             - Add a `<returns>` tag explaining what the value represents and any special cases \
             callers need to handle.\n\
             - Describe only existing behavior. Do not invent guarantees or change the \
             implementation to satisfy this lint."
        );
    }

    // ── reminders ──

    #[test]
    fn bool_returns_should_remind_when_returns_tag_is_missing() {
        let diagnostic = doc011("class C { public bool Ok() { return true; } }").unwrap();

        assert_eq!(diagnostic.severity, Severity::Reminder);
        assert_eq!(
            diagnostic.message,
            "Method returns `bool` but has no `<returns>` tag.\n\n\
             Why: Readers should not have to inspect the implementation to understand what `true` \
             and `false` mean.\n\n\
             Suggestions:\n\
             - No change is needed if the method name or summary already makes both outcomes \
             clear.\n\
             - Otherwise, consider a short `<returns>` tag explaining when the method returns \
             `true` and when it returns `false`."
        );
    }

    // ── clean members ──

    #[test]
    fn returns_tag_present_should_stay_quiet_when_tagged() {
        assert!(doc011(
            "class C {\n/// <summary>Gets.</summary>\n/// <returns>Zero.</returns>\npublic int Get() { return 0; }\n}"
        )
        .is_none());
    }

    #[test]
    fn void_returns_should_stay_quiet_when_method_returns_nothing() {
        assert!(doc011("class C { public void Go() { } }").is_none());
    }

    #[test]
    fn private_methods_should_stay_quiet_when_invisible() {
        assert!(doc011("class C { private int Get() { return 0; } }").is_none());
    }

    #[test]
    fn constructors_should_stay_quiet_when_declared() {
        assert!(doc011("class C { public C() { } }").is_none());
    }
}
