//! DOC011 lint: value-returning non-private methods with no `<returns>`
//! tag.

use super::{backend, codes, parse};

/// DOC011 warns on documented value-returning methods, reminds on
/// `bool` returns, and stays quiet on tagged, `void`, and private
/// members.
#[test]
fn doc011_checks_returns_tags_against_return_types() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>Returns the count.</summary>\n",
        "    public int GetCount() { return 0; }\n",
        "\n",
        "    /// <summary>Reports readiness.</summary>\n",
        "    public bool IsReady() { return true; }\n",
        "\n",
        "    /// <summary>Returns the count.</summary>\n",
        "    /// <returns>The stored count.</returns>\n",
        "    public int Tagged() { return 0; }\n",
        "\n",
        "    /// <summary>Clears the count.</summary>\n",
        "    public void Reset() { }\n",
        "\n",
        "    private int Hidden() { return 0; }\n",
        "}\n",
    );

    // Act.
    let parsed = parse(source);
    let doc011 = codes(&parsed, "DOC011");

    // Assert.
    assert_eq!(
        doc011,
        vec!["4:GetCount", "7:IsReady"],
        "warning and reminder findings only: {doc011:?}"
    );
    let severities: Vec<String> = backend()
        .lint(&parsed)
        .into_iter()
        .filter(|d| d.code == "DOC011")
        .map(|d| format!("{:?}:{}", d.severity, d.item_name.unwrap_or_default()))
        .collect();
    assert_eq!(
        severities,
        vec![
            "Warning:GetCount".to_string(),
            "Reminder:IsReady".to_string()
        ],
        "value returns warn, bool returns remind: {severities:?}"
    );
}
