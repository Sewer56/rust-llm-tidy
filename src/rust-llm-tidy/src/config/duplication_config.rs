//! Run-wide thresholds and whitespace comparison for DUP001.

use serde::Deserialize;

/// Default minimum meaningful lines in one repeated sequence.
pub(crate) const DEFAULT_MIN_MEANINGFUL_LINES: usize = 5;
/// Default minimum distinct, non-overlapping occurrences in one file.
pub(crate) const DEFAULT_MIN_OCCURRENCES: usize = 3;

/// Configure textual-duplication reminders under `duplication`.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct DuplicationConfig {
    /// Minimum lines containing a Unicode letter or number; must be at least 1.
    pub min_meaningful_lines: usize,
    /// Minimum non-overlapping sites, including the query; must be at least 2.
    pub min_occurrences: usize,
    /// Preserve edge whitespace and line endings instead of normalizing them.
    pub exact_whitespace: bool,
}

impl DuplicationConfig {
    /// Reject thresholds that cannot describe a nonempty repeated sequence.
    pub(crate) fn validate(self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.min_meaningful_lines > 0,
            "duplication.min_meaningful_lines must be >= 1"
        );
        anyhow::ensure!(
            self.min_occurrences >= 2,
            "duplication.min_occurrences must be >= 2"
        );
        Ok(())
    }
}

impl Default for DuplicationConfig {
    fn default() -> Self {
        Self {
            min_meaningful_lines: DEFAULT_MIN_MEANINGFUL_LINES,
            min_occurrences: DEFAULT_MIN_OCCURRENCES,
            exact_whitespace: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DuplicationConfig;
    use crate::config::{compiled::load::compile, load_and_compile};
    use rstest::rstest;

    #[rstest]
    #[case::absent("{}")]
    #[case::empty("duplication: {}")]
    fn configuration_should_keep_defaults(#[case] yaml: &str) {
        let config = compile(yaml, &[]);

        assert_eq!(config.duplication(), DuplicationConfig::default());
    }

    #[test]
    fn configuration_should_resolve_run_wide_settings() {
        let expected = DuplicationConfig {
            min_meaningful_lines: 2,
            min_occurrences: 4,
            exact_whitespace: true,
        };
        let config = compile(
            "duplication: {min_meaningful_lines: 2, min_occurrences: 4, exact_whitespace: true}",
            &[],
        );

        assert_eq!(config.duplication(), expected);
    }

    #[rstest]
    #[case::empty_sequence("min_meaningful_lines: 0", "min_meaningful_lines must be >= 1")]
    #[case::no_sites("min_occurrences: 0", "min_occurrences must be >= 2")]
    #[case::one_site("min_occurrences: 1", "min_occurrences must be >= 2")]
    #[case::negative("min_meaningful_lines: -1", "failed to parse YAML")]
    #[case::fractional("min_occurrences: 2.5", "failed to parse YAML")]
    #[case::unknown("by_extension: {}", "failed to parse YAML")]
    #[case::whitespace_type("exact_whitespace: other", "failed to parse YAML")]
    fn configuration_should_reject_invalid_settings(#[case] settings: &str, #[case] error: &str) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.yml");
        std::fs::write(&path, format!("duplication: {{{settings}}}")).unwrap();

        let result = load_and_compile(&path).unwrap_err();

        assert!(format!("{result:#}").contains(error), "{result:#}");
    }
}
