//! TEXT007 passive-narration enablement and release-note suppression settings.

use serde::Deserialize;

/// Settings under the top-level `passive_narration` key for the
/// TEXT007 passive-narration lint.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(deny_unknown_fields)] // Reject hallucinated sub-keys at parse time.
pub struct PassiveNarrationConfig {
    /// Run the TEXT007 passive-narration lint.
    ///
    /// - Default: `true`
    /// - `false`: disable TEXT007 in file processing
    /// - Findings use Reminder severity and default to changed lines
    /// - `--include TEXT007` runs it regardless of this setting
    /// - Pathless library text checks: unaffected
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Suppress TEXT007 narration markers in release and migration notes.
    ///
    /// - Default: `true`
    /// - Paths: `CHANGELOG*` or `MIGRATION*` basenames at any depth, or files
    ///   under a `releases` directory (case-insensitive)
    /// - Passive-voice findings: unaffected
    #[serde(default = "default_true")]
    pub suppress_in_release_notes: bool,
}

impl Default for PassiveNarrationConfig {
    /// An absent section enables the lint and suppression, matching a
    /// present section that omits both keys.
    fn default() -> Self {
        Self {
            enable: default_true(),
            suppress_in_release_notes: default_true(),
        }
    }
}

/// `serde` default helper: an absent key keeps its default `true` value.
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use crate::config::compiled::load::compile;
    use rstest::rstest;

    /// YAML defaults keep narration suppression on with and without the
    /// `passive_narration` section.
    #[test]
    fn suppression_should_default_to_enabled() {
        assert!(compile("{}\n", &[]).suppress_in_release_notes());
        assert!(compile("passive_narration:\n  enable: true\n", &[]).suppress_in_release_notes());
        assert!(
            !compile(
                "passive_narration:\n  suppress_in_release_notes: false\n",
                &[]
            )
            .suppress_in_release_notes()
        );
    }

    /// Omitted enablement keeps TEXT007 on; an explicit false disables it.
    #[rstest]
    #[case::absent("{}\n", true)]
    #[case::empty("passive_narration: {}\n", true)]
    #[case::suppression_only("passive_narration:\n  suppress_in_release_notes: false\n", true)]
    #[case::enabled("passive_narration:\n  enable: true\n", true)]
    #[case::disabled("passive_narration:\n  enable: false\n", false)]
    fn passive_narration_should_respect_enablement(#[case] yaml: &str, #[case] expected: bool) {
        let config = compile(yaml, &[]);

        assert_eq!(config.passive_narration(), expected);
    }
}
