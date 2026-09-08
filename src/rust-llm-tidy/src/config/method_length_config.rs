//! Method-length threshold settings under the top-level `method_length` key.

use serde::Deserialize;

/// Threshold applied when the `method_length` section or its `max_lines` key
/// is absent.
///
/// 75 centers the 40-100 non-comment-line cluster of mainstream linter
/// defaults and equals SonarQube S138, the only size-based rule among them.
pub(crate) const DEFAULT_METHOD_LENGTH_MAX_LINES: usize = 75;

/// Cap measured body lines per function under the `method_length` key.
///
/// The effective threshold is `max_lines`; an absent section or key keeps
/// the default 75. `max_lines` must be `>= 1`.
#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)] // Reject hallucinated `method_length` sub-keys at parse time.
pub struct MethodLengthConfig {
    /// Maximum counted body lines per function before LEN001 warns.
    ///
    /// Counted lines are the non-blank, non-comment-only lines strictly
    /// between the body braces; the signature line never counts.
    #[serde(default = "default_method_length_max_lines")]
    pub max_lines: usize,
}

impl Default for MethodLengthConfig {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_METHOD_LENGTH_MAX_LINES,
        }
    }
}

/// `serde` default helper: an absent `max_lines` keeps the default
/// method-length threshold.
fn default_method_length_max_lines() -> usize {
    DEFAULT_METHOD_LENGTH_MAX_LINES
}

#[cfg(test)]
mod tests {
    use crate::config::compiled::load::compile;
    use crate::config::load_and_compile;

    /// Threshold resolution: an absent or empty `method_length` section
    /// keeps the 75 default; an explicit `max_lines` wins.
    #[test]
    fn method_length_max_lines_should_default_to_75_until_configured() {
        let absent = compile("exclude_files: []\n", &[]);
        assert_eq!(absent.method_length().max_lines, 75);

        let empty_section = compile("method_length: {}\n", &[]);
        assert_eq!(empty_section.method_length().max_lines, 75);

        let configured = compile("method_length:\n  max_lines: 40\n", &[]);
        assert_eq!(configured.method_length().max_lines, 40);
    }

    /// A literal 0 reaches the compile-time validation and is rejected.
    #[test]
    fn method_length_max_lines_zero_should_be_rejected() {
        let dir = std::env::temp_dir().join(format!("rlt-cfg-mlen0-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, "method_length:\n  max_lines: 0\n").unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        assert!(
            format!("{err:#}").contains("method_length.max_lines must be >= 1"),
            "zero threshold must be rejected: {err:#}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `deny_unknown_fields` rejects hallucinated sub-keys at parse time.
    #[test]
    fn method_length_unknown_subkey_should_be_rejected() {
        let dir = std::env::temp_dir().join(format!("rlt-cfg-mlenkey-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, "method_length:\n  max_lines: 10\n  budget: 5\n").unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        assert!(
            format!("{err:#}").contains("failed to parse YAML config"),
            "unknown sub-key must fail at parse: {err:#}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
