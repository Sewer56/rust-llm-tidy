//! DOC003 lint: `<exception>` tags without a concrete `cref`, for
//! direct and indirect throwers.

use super::{backend, codes, parse};
use rust_llm_tidy::reporting::Severity;

/// DOC003 follows the recursive gate: a caller with no `throw` of its
/// own whose only `<exception>` tag lacks a `cref` is warned about, not
/// silent.
#[test]
fn doc003_warns_on_indirect_throwers_with_vague_crefs() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Throws.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">Always.</exception>\n",
        "    private void Thrower() { throw new System.InvalidOperationException(); }\n",
        "\n",
        "    /// <summary>Calls the thrower, vaguely.</summary>\n",
        "    /// <exception>On failure.</exception>\n",
        "    public void Vague() { Thrower(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC003");

    assert_eq!(found, vec!["8:Vague"], "the indirect caller: {found:?}");
    assert!(
        backend()
            .lint(&parsed)
            .iter()
            .filter(|d| d.code == "DOC003")
            .all(|d| d.severity == Severity::Warning),
        "indirect DOC003 findings keep warning severity"
    );
}

/// DOC003 warns when `<exception>` tags exist but none carries a concrete
/// `cref`; any non-empty `cref` satisfies it.
#[test]
fn doc003_warns_on_vague_exception_crefs() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Vague.</summary>\n",
        "    /// <exception>On failure.</exception>\n",
        "    public void Vague() { throw new System.Exception(); }\n",
        "    /// <summary>Concrete.</summary>\n",
        "    /// <exception cref=\"System.InvalidOperationException\">No state.</exception>\n",
        "    public void Concrete() { throw new System.Exception(); }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC003");

    assert_eq!(
        found,
        vec!["4:Vague"],
        "only the cref-less tag set: {found:?}"
    );
}
