//! DOC004 lint: parameterized documented members with no `<param>` tags.
//!
//! The same checks also cover DOC005: `<param>` sets that omit a declared
//! parameter.

use super::{backend, codes, parse};

/// DOC004 fires when a parameterized documented member has no `<param>`
/// tags at all; DOC005 when the tags omit a declared parameter.
///
/// Constructors and indexers carry real parameter lists into both checks.
#[test]
fn doc004_and_doc005_check_param_tags_against_real_parameters() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Untagged.</summary>\n",
        "    public void Untagged(string key, int count) { }\n",
        "\n",
        "    /// <summary>Partial.</summary>\n",
        "    /// <param name=\"key\">The key.</param>\n",
        "    public void Partial(string key, int count) { }\n",
        "\n",
        "    /// <summary>Complete.</summary>\n",
        "    /// <param name=\"key\">The key.</param>\n",
        "    /// <param name=\"count\">The count.</param>\n",
        "    public void Complete(string key, int count) { }\n",
        "\n",
        "    /// <summary>Untagged constructor.</summary>\n",
        "    public C(string seed) { }\n",
        "\n",
        "    /// <summary>Untagged indexer.</summary>\n",
        "    public int this[string key] => 0;\n",
        "}\n",
    );
    let parsed = parse(source);
    let doc004 = codes(&parsed, "DOC004");
    let doc005 = codes(&parsed, "DOC005");

    assert_eq!(
        doc004,
        vec!["4:Untagged", "16:C", "19:"],
        "methods, constructors, and indexers without tags: {doc004:?}"
    );
    assert_eq!(
        doc005,
        vec!["7:Partial"],
        "only the partial method: {doc005:?}"
    );
    let messages: Vec<String> = backend()
        .lint(&parsed)
        .into_iter()
        .filter(|d| d.code == "DOC005")
        .map(|d| d.message)
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("`count`")),
        "DOC005 must name the undocumented parameter: {messages:?}"
    );
}
