//! TEXT007 passive-narration opt-in and release-note suppression settings.

use serde::Deserialize;

/// Settings under the top-level `passive_narration` key for the opt-in
/// TEXT007 passive-narration lint.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(deny_unknown_fields)] // Reject hallucinated sub-keys at parse time.
pub struct PassiveNarrationConfig {
    /// Run the TEXT007 passive-narration lint.
    ///
    /// - Default: `false` (the lint is prone to false positives)
    /// - `true`: report TEXT007 hints in file processing
    /// - `--include TEXT007` runs it regardless of this setting
    /// - Pathless library text checks: unaffected
    #[serde(default)]
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
    /// An absent section keeps the lint off but suppression on, matching a
    /// present section that omits both keys.
    fn default() -> Self {
        Self {
            enable: false,
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
    use crate::config::Config;
    use crate::config::compiled::load::compile;

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

    /// The opt-in TEXT007 switch defaults off until the section enables it.
    #[test]
    fn passive_narration_should_default_off_until_configured() {
        assert_eq!(Config::default().passive_narration, None);
        assert!(!compile("{}\n", &[]).passive_narration());
        assert!(
            !compile(
                "passive_narration:\n  suppress_in_release_notes: false\n",
                &[]
            )
            .passive_narration()
        );
        assert!(compile("passive_narration:\n  enable: true\n", &[]).passive_narration());
    }
}
