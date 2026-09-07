//! Module-size threshold settings under the top-level `module_size` key.

use serde::Deserialize;

/// Threshold applied when the `module_size` section or its `max_lines` key
/// is absent.
pub(crate) const DEFAULT_MODULE_SIZE_MAX_LINES: usize = 500;

/// Select files and counted lines for MOD001 under the `module_size` key.
///
/// The effective threshold is `max_lines`; an absent section or key keeps
/// the default 500. `max_lines` must be `>= 1`.
#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)] // Reject hallucinated `module_size` sub-keys at parse time.
pub struct ModuleSizeConfig {
    /// Maximum counted physical lines per eligible file before MOD001 warns.
    #[serde(default = "default_module_size_max_lines")]
    pub max_lines: usize,
    /// Include supported configuration, data, and prose files selected for the run.
    ///
    /// Defaults to false: only code files are checked. Does not broaden discovery
    /// or enable other operations for otherwise op-less formats.
    #[serde(default)]
    pub include_non_code: bool,
    /// Count Rust's top-level `#[cfg(test)]` mod regions instead of excluding them.
    ///
    /// Defaults to false; independent of `include_test_files`.
    /// Other languages always count all physical lines, including inline tests.
    #[serde(default)]
    pub include_in_file_tests: bool,
    /// Include Rust files with an exact `tests` directory component in their path.
    ///
    /// Defaults to false. `include_in_file_tests` still controls test-module regions.
    /// Other languages always include test files.
    #[serde(default)]
    pub include_test_files: bool,
}

impl Default for ModuleSizeConfig {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MODULE_SIZE_MAX_LINES,
            include_non_code: false,
            include_in_file_tests: false,
            include_test_files: false,
        }
    }
}

/// `serde` default helper: an absent `max_lines` keeps the default
/// module-size threshold.
fn default_module_size_max_lines() -> usize {
    DEFAULT_MODULE_SIZE_MAX_LINES
}

#[cfg(test)]
mod tests {
    use crate::config::compiled::load::compile;

    /// Threshold resolution: an absent or empty `module_size` section keeps
    /// the 500 default; an explicit `max_lines` wins.
    #[test]
    fn module_size_max_lines_should_default_to_500_until_configured() {
        let absent = compile("exclude_files: []\n", &[]);
        assert_eq!(absent.module_size_max_lines(), 500);

        let empty_section = compile("module_size: {}\n", &[]);
        assert_eq!(empty_section.module_size_max_lines(), 500);

        let configured = compile("module_size:\n  max_lines: 300\n", &[]);
        assert_eq!(configured.module_size_max_lines(), 300);
    }
}
