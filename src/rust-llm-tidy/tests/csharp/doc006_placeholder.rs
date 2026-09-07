//! DOC006 lint: whole-word placeholder terms in doc comments.

use super::{codes, parse};

/// DOC006 fires on whole-word placeholders in doc comments, not inside
/// longer words.
#[test]
fn doc006_flags_placeholder_words_only() {
    let source = concat!(
        "/// <summary>Container.</summary>\n",
        "public class C\n",
        "{\n",
        "    /// <summary>TODO: write this.</summary>\n",
        "    public void Todo() { }\n",
        "    /// <summary>Mentions todolist shapes.</summary>\n",
        "    public void Todolist() { }\n",
        "}\n",
    );
    let parsed = parse(source);
    let found = codes(&parsed, "DOC006");

    assert_eq!(found, vec!["4:Todo"], "whole words only: {found:?}");
}
