//! Character blacklists with user-owned diagnostic titles and messages.

use anyhow::bail;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::LazyLock;

/// One TEXT009 blacklist entry. Each character shares the title and message.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenCharacterRule {
    /// Unicode scalar values to reject in prose, not strings or sequences.
    pub characters: Vec<char>,
    /// Nonblank diagnostic title.
    pub title: Box<str>,
    /// Complete nonblank rewrite guidance, preserved verbatim.
    pub message: Box<str>,
    /// Select documentation and ordinary comments independently.
    #[serde(default)]
    pub scope: ForbiddenCharacterScope,
}

/// TEXT009 entry applicability; omitted fields enable their category.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ForbiddenCharacterScope {
    /// Documentation files, doc comments, docstrings, and doc attributes.
    pub docs: bool,
    /// Ordinary standalone, trailing, and block comments.
    pub comments: bool,
}

impl Default for ForbiddenCharacterScope {
    fn default() -> Self {
        Self {
            docs: true,
            comments: true,
        }
    }
}

/// Default style policy; custom lists replace it, including an empty list.
pub(crate) fn defaults() -> &'static [ForbiddenCharacterRule] {
    static RULES: LazyLock<Vec<ForbiddenCharacterRule>> = LazyLock::new(|| {
        vec![ForbiddenCharacterRule {
            scope: ForbiddenCharacterScope::default(),
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
        }, ForbiddenCharacterRule {
            characters: vec!['\u{2013}'],
            title: "Use plain punctuation instead of en dashes".into(),
            message: "Why: Plain punctuation keeps source text consistent.\nSuggestions:\n- Express ranges with 'to', or choose ASCII punctuation that preserves the meaning.".into(),
            scope: ForbiddenCharacterScope::default(),
        }, ForbiddenCharacterRule {
            characters: vec!['\u{2018}', '\u{2019}'],
            title: "Use straight single quotes and apostrophes".into(),
            message: "Why: Straight quotes keep source text consistent.\nSuggestions:\n- Use ASCII single quotes or apostrophes. Preserve the quoted words and meaning.".into(),
            scope: ForbiddenCharacterScope::default(),
        }, ForbiddenCharacterRule {
            characters: vec!['\u{201C}', '\u{201D}'],
            title: "Use straight double quotes".into(),
            message: "Why: Straight quotes keep source text consistent.\nSuggestions:\n- Use ASCII double quotes. Preserve the quoted words exactly.".into(),
            scope: ForbiddenCharacterScope::default(),
        }, ForbiddenCharacterRule {
            characters: vec!['\u{2026}'],
            title: "Use plain punctuation instead of the ellipsis character".into(),
            message: "Why: Plain punctuation keeps source text consistent.\nSuggestions:\n- Use three ASCII periods if an ellipsis is needed; otherwise rewrite without it. Preserve the meaning.".into(),
            scope: ForbiddenCharacterScope::default(),
        }]
    });
    &RULES
}

/// Validate entries before file processing so matches have unambiguous guidance.
///
/// # Errors
/// Rejects empty character lists, blank titles/messages, repeated characters
/// within an entry, or characters whose entries enable overlapping scopes.
pub(super) fn validate(rules: &[ForbiddenCharacterRule], base_len: usize) -> anyhow::Result<()> {
    let mut seen = HashSet::new();
    for (index, rule) in rules.iter().enumerate() {
        let (setting, index) = if index < base_len {
            ("forbidden_characters", index)
        } else {
            ("extra_forbidden_characters", index - base_len)
        };
        if rule.characters.is_empty()
            || rule.title.trim().is_empty()
            || rule.message.trim().is_empty()
        {
            bail!("{setting}[{index}] requires nonempty characters and nonblank title and message");
        }

        let mut entry = HashSet::with_capacity(rule.characters.len());
        for &character in &rule.characters {
            if !entry.insert(character)
                || (rule.scope.docs && !seen.insert((character, true)))
                || (rule.scope.comments && !seen.insert((character, false)))
            {
                bail!(
                    "duplicate character U+{:04X} in {setting}[{index}]; list each character once per entry and use non-overlapping scopes",
                    character as u32
                );
            }
        }
    }
    Ok(())
}
