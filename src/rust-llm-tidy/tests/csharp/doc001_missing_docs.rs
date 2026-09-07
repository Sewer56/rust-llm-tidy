//! DOC001 lint: undocumented non-private members of documentable kinds.

use super::{codes, parse};

/// DOC001 fires on undocumented non-private members of every documentable
/// kind and stays silent for private and documented ones.
#[test]
fn doc001_flags_undocumented_non_private_members() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    public void Public() { }\n",
        "    internal int Internal { get; set; }\n",
        "    protected void Protected() { }\n",
        "    private void Private() { }\n",
        "    void Default() { }\n",
        "    /// <summary>Documented.</summary>\n",
        "    public void Documented() { }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC001");

    assert!(
        found.contains(&"4:Public".to_string()),
        "public method: {found:?}"
    );
    assert!(
        found.contains(&"5:Internal".to_string()),
        "internal property: {found:?}"
    );
    assert!(
        found.contains(&"6:Protected".to_string()),
        "protected method: {found:?}"
    );
    assert!(
        !found.iter().any(|f| f.contains("Private")),
        "private: {found:?}"
    );
    assert!(
        !found.iter().any(|f| f.contains("Default")),
        "no modifier: {found:?}"
    );
    assert!(
        !found.iter().any(|f| f.contains("Documented")),
        "documented: {found:?}"
    );
    assert_eq!(
        found.len(),
        3,
        "exactly the three non-private gaps: {found:?}"
    );
}
