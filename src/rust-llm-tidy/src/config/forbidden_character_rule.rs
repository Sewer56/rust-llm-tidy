//! Character blacklists with user-owned diagnostic titles and messages.

use anyhow::bail;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::LazyLock;

/// One TEXT009 blacklist entry. Each character shares the title and message.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenCharacterRule {
    /// Unicode scalar values to reject in prose, not strings or sequences.
    pub characters: Vec<char>,
    /// Nonblank diagnostic title.
    pub title: Box<str>,
    /// Complete nonblank rewrite guidance, preserved verbatim.
    pub message: Box<str>,
}

/// Default style policy; custom lists replace it, including an empty list.
pub(crate) fn defaults() -> &'static [ForbiddenCharacterRule] {
    static RULES: LazyLock<Vec<ForbiddenCharacterRule>> = LazyLock::new(|| {
        vec![ForbiddenCharacterRule {
            characters: vec!['\u{2014}'],
            title: "Use natural phrasing without em dashes".into(),
            message: concat!(
                "Why: Direct sentences and a conversational rhythm make writing easier to follow.\n",
                "Suggestions:\n",
                "- Write like a person talking to another person, not an AI composing a response.\n",
                "- Avoid em dashes. Use direct sentences and a natural, conversational rhythm.\n",
                "- Rewrite the sentence, using a comma, colon, parentheses, or full stop where it fits the meaning.\n",
                "- Do not mechanically replace every dash with the same punctuation. Preserve the meaning and technical precision."
            ).into(),
        }]
    });
    &RULES
}

/// Validate entries before file processing so matches have unambiguous guidance.
///
/// Errors on empty character lists, blank titles/messages, or duplicate
/// characters.
pub(super) fn validate(rules: &[ForbiddenCharacterRule]) -> anyhow::Result<()> {
    let mut seen = HashSet::new();
    for (index, rule) in rules.iter().enumerate() {
        if rule.characters.is_empty()
            || rule.title.trim().is_empty()
            || rule.message.trim().is_empty()
        {
            bail!(
                "forbidden_characters[{index}] requires nonempty characters and nonblank title and message"
            );
        }

        for &character in &rule.characters {
            if !seen.insert(character) {
                bail!(
                    "duplicate character U+{:04X} in forbidden_characters; list each character once",
                    character as u32
                );
            }
        }
    }
    Ok(())
}
