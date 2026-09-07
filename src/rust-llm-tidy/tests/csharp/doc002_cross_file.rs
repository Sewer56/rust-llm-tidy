//! Cross-file DOC002/DOC003 through the lint index: resolvable chains,
//! merged callers, exception tags, and same-name member ownership.

use super::{backend, parse};
use rust_llm_tidy::reporting::Severity;

/// Qualified calls propagate throw evidence through files without guessing
/// value receivers.
#[test]
fn indexed_lints_should_follow_resolvable_cross_file_chains() {
    let cases = [
        (
            "static",
            "class A { public void Caller() { T.Helper(); } }",
            "class T { void Helper() { throw new E(); } }",
            "",
            true,
        ),
        (
            "partial",
            "partial class T { public void Caller() { Helper(); } }",
            "partial class T { void Helper() { throw new E(); } }",
            "",
            true,
        ),
        (
            "this",
            "partial class T { public void Caller() { this.Helper(); } }",
            "partial class T { void Helper() { throw new E(); } }",
            "",
            true,
        ),
        (
            "constructor",
            "class A { public void Caller() { new T(); } }",
            "class T { T() { throw new E(); } }",
            "",
            true,
        ),
        (
            "transitive",
            "class A { public void Caller() { T.Helper(); } }",
            "class T { void Helper() { U.End(); } }",
            "class U { void End() { throw new E(); } }",
            true,
        ),
        (
            "mixed",
            "class A { public void Caller() { Local(); } void Local() { T.Helper(); } }",
            "class T { void Helper() { throw new E(); } }",
            "",
            true,
        ),
        (
            "value_receiver",
            "class A { public void Caller() { obj.Helper(); } }",
            "class T { void Helper() { throw new E(); } }",
            "",
            false,
        ),
        (
            "framework",
            "class A { public void Caller() { System.IO.File.Open(); } }",
            "class T { void Helper() { throw new E(); } }",
            "",
            false,
        ),
        (
            "broken_foreign",
            "class A { public void Caller() { T.Helper(); } }",
            "class T { void Helper() { throw new E(); }",
            "",
            false,
        ),
        (
            "unseeded_cycle",
            "class A { public void Caller() { T.Helper(); } }",
            "class T { void Helper() { A.Caller(); } }",
            "",
            false,
        ),
        (
            "seeded_cycle",
            "class A { public void Caller() { T.Helper(); } }",
            "class T { void Helper() { A.Caller(); U.End(); } }",
            "class U { void End() { throw new E(); } }",
            true,
        ),
    ];

    for (label, caller, helper, end, expected) in cases {
        let parses = [parse(caller), parse(helper), parse(end)];
        let index = rust_llm_tidy::languages::CanThrowIndex::from_parses(&parses);

        let findings = backend().lint_indexed(&parses[0], &index);

        assert_eq!(
            findings
                .iter()
                .any(|d| d.code == "DOC002" && d.item_name.as_deref() == Some("Caller")),
            expected,
            "{label}"
        );
    }
}

/// Separate callers compose local hops with the same foreign evidence as a
/// complete index.
#[test]
fn indexed_lints_should_merge_separately_supplied_callers() {
    for (source, foreign_source) in [
        (
            "class A { public void Caller() { T.Helper(); } }",
            "class T { void Helper() { throw new E(); } }",
        ),
        (
            "class A { public void Caller() { Local(); } void Local() { T.Helper(); } }",
            "class T { void Helper() { throw new E(); } }",
        ),
        (
            "class A { public void Caller() { T.Helper(); } private static void End() { throw new E(); } }",
            "class T { void Helper() { A.End(); } }",
        ),
    ] {
        let helper = parse(foreign_source);
        let caller = parse(source);
        let foreign = rust_llm_tidy::languages::CanThrowIndex::from_parses([&helper]);
        let complete = rust_llm_tidy::languages::CanThrowIndex::from_parses([&caller, &helper]);

        let separate = backend().lint_indexed(&caller, &foreign);
        let together = backend().lint_indexed(&caller, &complete);

        assert!(
            separate
                .iter()
                .any(|d| d.code == "DOC002" && d.item_name.as_deref() == Some("Caller"))
        );
        assert_eq!(format!("{separate:?}"), format!("{together:?}"));
    }
}

/// Concrete exception types suppress findings; vague tags retain warning
/// severity across files.
#[test]
fn indexed_lints_should_respect_exception_tags() {
    let helper = parse("class T { void Helper() { throw new E(); } }");
    for (label, tag, expected) in [
        (
            "concrete",
            "<exception cref=\"E\">Failure.</exception>",
            None,
        ),
        ("vague", "<exception>Failure.</exception>", Some("DOC003")),
        (
            "missing",
            "<summary>Calls the helper.</summary>",
            Some("DOC002"),
        ),
    ] {
        let caller = parse(&format!(
            "class A {{\n/// {tag}\npublic void Caller() {{ T.Helper(); }}\n}}"
        ));
        let index = rust_llm_tidy::languages::CanThrowIndex::from_parses([&caller, &helper]);

        let findings: Vec<_> = backend()
            .lint_indexed(&caller, &index)
            .into_iter()
            .filter(|d| matches!(d.code, "DOC002" | "DOC003"))
            .collect();

        assert_eq!(findings.first().map(|d| d.code), expected, "{label}");
        assert_eq!(findings.len(), usize::from(expected.is_some()), "{label}");
        if let Some(diagnostic) = findings.first() {
            assert_eq!(diagnostic.line, 2);
            assert_eq!(
                diagnostic.severity,
                if expected == Some("DOC003") {
                    Severity::Warning
                } else {
                    Severity::Error
                }
            );
        }
    }
}

/// Splitting same-name methods across nested types preserves all diagnostic fields.
#[test]
fn lint_should_preserve_findings_when_same_name_members_have_different_owners() {
    let sources = [
        "class C {\n void Helper() { throw new E(); }\n void Helper(int x) {}\n /// <summary>Calls a helper.</summary>\n public void Caller() { obj.Helper(); }\n}",
        "class C { class Nested {\n void Helper() { throw new E(); } }\n void Helper(int x) {}\n /// <summary>Calls a helper.</summary>\n public void Caller() { obj.Helper(); }\n}",
    ];

    let findings: Vec<_> = sources
        .iter()
        .map(|source| {
            backend()
                .lint(&parse(source))
                .into_iter()
                .map(|diagnostic| {
                    (
                        diagnostic.code,
                        diagnostic.severity,
                        diagnostic.line,
                        diagnostic.item_kind,
                        diagnostic.item_name,
                        diagnostic.message,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();

    assert_eq!(findings[0], findings[1]);
    // The item rule alone: the single-sentence opener stays TEXT004-quiet.
    assert_eq!(findings[0].len(), 1);
    assert_eq!(findings[0][0].0, "DOC002");
    assert_eq!(findings[0][0].4.as_deref(), Some("Caller"));
}
