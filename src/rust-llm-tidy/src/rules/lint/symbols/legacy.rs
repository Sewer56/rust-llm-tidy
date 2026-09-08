//! Adapt PERF001 lists to the shared invocation walker without changing messages.

use super::usage::Usage;
use crate::config::{
    CompiledSymbolRule, PerfHint, SymbolAction, SymbolLanguage, SymbolMatcher, SymbolTarget,
};
use crate::reporting::Severity;
use crate::rules::registry::CODE_PERF001;

/// Compile the resolved base and extra lists in their original precedence order.
///
/// Call once per run and language. Built-ins remain in the existing per-language
/// modules; the caller supplies them when no replacement list was configured.
pub(crate) fn compile_legacy_hints(
    language: SymbolLanguage,
    base: &[PerfHint],
    extra: &[PerfHint],
) -> Vec<CompiledSymbolRule> {
    base.iter()
        .chain(extra)
        .map(|hint| {
            let components: Box<[Box<str>]> = hint
                .pattern
                .split("::")
                .map(str::trim)
                .filter(|part| !part.is_empty() && !part.starts_with('<'))
                .map(Box::from)
                .collect();
            let constructor = match language {
                SymbolLanguage::Rust => {
                    components.last().is_some_and(|part| part.as_ref() == "new")
                }
                SymbolLanguage::Csharp => hint.pattern.ends_with("::new"),
            };

            CompiledSymbolRule {
                language: Some(language),
                extensions: [
                    language == SymbolLanguage::Rust,
                    language == SymbolLanguage::Csharp,
                ],
                array_kind: None,
                target: SymbolTarget::Usage,
                matcher: SymbolMatcher::Legacy {
                    pattern: Box::from(hint.pattern.as_ref()),
                    components,
                },
                action: SymbolAction::Hint,
                message: Some(Box::from(hint.message.as_ref())),
                scope: None,
                severity: Severity::Reminder,
                zero_arguments: constructor.then_some(true),
                no_initializer: (constructor && language == SymbolLanguage::Csharp).then_some(true),
                code: CODE_PERF001,
            }
        })
        .collect()
}

/// Preserve legacy name comparison; constructor constraints were compiled above.
pub(super) fn matches(
    pattern: &str,
    components: &[Box<str>],
    usage: &Usage<'_>,
    language: SymbolLanguage,
) -> bool {
    match (language, usage.kind) {
        (SymbolLanguage::Rust, "macro") => {
            pattern == usage.legacy_name || pattern.strip_suffix('!') == Some(usage.legacy_name)
        }
        (SymbolLanguage::Rust, "call") => {
            let mut remaining = components.len();
            if remaining == 0 {
                return false;
            }

            for part in usage.legacy_name.rsplit("::").map(str::trim) {
                if remaining == 0 {
                    return true;
                }
                if part.is_empty() || part.starts_with('<') {
                    continue;
                }
                if part != components[remaining - 1].as_ref() {
                    return false;
                }
                remaining -= 1;
            }
            remaining == 0
        }
        (SymbolLanguage::Csharp, "creation") => {
            pattern.strip_suffix("::new") == Some(usage.legacy_name)
        }
        (SymbolLanguage::Csharp, "call") => pattern == usage.legacy_name,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::backend_for;
    use crate::rules::lint::{csharp, rust, symbols};
    use rstest::rstest;

    #[rstest]
    #[case::rust(
        "rs",
        "fn f() { let a = std::vec::Vec::<u8>::new(); let b = Vec::new(1); takes(Vec::new()); value.to_string(); format!(\"x\"); }",
        SymbolLanguage::Rust
    )]
    #[case::csharp(
        "cs",
        "class C { void M() { Take(new System.Collections.Generic.List<int>()); var a = new List<int>(1); var b = new List<int> { 1 }; value.ToString(); } }",
        SymbolLanguage::Csharp
    )]
    fn shared_hints_should_render_like_legacy_reference(
        #[case] ext: &str,
        #[case] source: &str,
        #[case] language: SymbolLanguage,
    ) {
        let parsed = backend_for(ext).unwrap().parse(source).unwrap();
        let (base, mut reference) = match language {
            SymbolLanguage::Rust => {
                let base = rust::perf001_allocation_hints::default_hints();
                (
                    base,
                    rust::perf001_allocation_hints::check(&parsed, base, &[]),
                )
            }
            SymbolLanguage::Csharp => {
                let base = csharp::perf001_allocation_hints::default_hints();
                (
                    base,
                    csharp::perf001_allocation_hints::check(&parsed, base, &[]),
                )
            }
        };
        let rules = compile_legacy_hints(language, base, &[]);
        for diagnostic in &mut reference {
            diagnostic.severity = Severity::Reminder;
        }

        let shared = symbols::check(&parsed, ext, &rules).unwrap();
        let actual: Vec<_> = shared
            .hints
            .iter()
            .map(|hint| hint.diagnostic.to_string())
            .collect();
        let expected: Vec<_> = reference.iter().map(ToString::to_string).collect();

        assert_eq!(actual, expected);
    }
}
