//! Link-hoist threshold settings under the top-level `links` key.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Link-hoist threshold settings under the top-level `links` key.
///
/// The effective per-file threshold is `by_extension[ext]`, else the global
/// `min_occurrences`, else 1.
///
/// Extension keys are free-form so future languages need no schema change;
/// keys for extensions the pipeline does not process are inert. Values must
/// be `>= 1`.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)] // Reject hallucinated `links` sub-keys at parse time.
pub struct LinkConfig {
    /// Global minimum occurrences before a pair is hoisted. Default 1 = always
    /// hoist, unchanged behavior.
    #[serde(default = "default_one")]
    pub min_occurrences: usize,
    /// Per-extension thresholds, applied before the global setting.
    #[serde(default)]
    pub by_extension: BTreeMap<String, usize>,
}

/// `serde` default helper: an absent `min_occurrences` means threshold 1
/// (always hoist).
fn default_one() -> usize {
    1
}

#[cfg(test)]
mod tests {
    use crate::config::compiled::load::compile;
    use crate::config::load_and_compile;

    // ── links.min_occurrences + links.by_extension ──

    #[test]
    fn absent_links_defaults_threshold_to_one_for_any_extension() {
        let cc = compile("exclude_files: []\n", &[("src/lib.rs", "fn x() {}\n")]);
        for ext in ["rs", "md", "py"] {
            assert_eq!(
                cc.links_min_occurrences_for(ext),
                1,
                "absent `links` must hoist at threshold 1 for {ext}"
            );
        }
    }

    #[test]
    fn global_min_occurrences_applies_to_all_extensions() {
        let cc = compile("links:\n  min_occurrences: 2\n", &[]);
        for ext in ["rs", "md"] {
            assert_eq!(
                cc.links_min_occurrences_for(ext),
                2,
                "global `min_occurrences: 2` must apply to {ext}"
            );
        }
    }

    #[test]
    fn by_extension_overrides_only_the_named_extension() {
        let cc = compile(
            "links:\n  min_occurrences: 4\n  by_extension:\n    rs: 3\n",
            &[],
        );
        assert_eq!(cc.links_min_occurrences_for("rs"), 3, "rs override wins");
        assert_eq!(
            cc.links_min_occurrences_for("md"),
            4,
            "md falls back to the global threshold"
        );
    }

    #[test]
    fn by_extension_without_global_falls_back_to_one() {
        // `min_occurrences` is absent, so a non-overridden extension falls back
        // to its default of 1 while `rs` uses the explicit override.
        let cc = compile("links:\n  by_extension:\n    rs: 3\n", &[]);
        assert_eq!(cc.links_min_occurrences_for("rs"), 3);
        assert_eq!(cc.links_min_occurrences_for("md"), 1);
    }

    #[test]
    fn unknown_extension_keys_are_accepted_and_stored() {
        // Free-form by_extension keys for future/unprocessed languages are
        // accepted and stored without a parse error.
        let cc = compile("links:\n  by_extension:\n    py: 2\n    go: 3\n", &[]);
        assert_eq!(cc.links_min_occurrences_for("py"), 2);
        assert_eq!(cc.links_min_occurrences_for("go"), 3);
        assert_eq!(cc.links_min_occurrences_for("rs"), 1, "unlisted falls back");
    }

    #[test]
    fn min_occurrences_zero_is_rejected() {
        let dir = std::env::temp_dir().join(format!("rlt-cfg-min0-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, "links:\n  min_occurrences: 0\n").unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        assert!(
            format!("{err:#}").contains("links.min_occurrences must be >= 1"),
            "zero threshold must be rejected: {err:#}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn by_extension_zero_is_rejected() {
        let dir = std::env::temp_dir().join(format!("rlt-cfg-bext0-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, "links:\n  by_extension:\n    rs: 0\n").unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        assert!(
            format!("{err:#}").contains("links.by_extension.rs must be >= 1"),
            "zero per-extension threshold must be rejected: {err:#}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_integer_links_value_is_rejected() {
        // A non-integer value fails YAML deserialization before the >= 1 check.
        let dir = std::env::temp_dir().join(format!("rlt-cfg-nonint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, "links:\n  min_occurrences: many\n").unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        assert!(
            format!("{err:#}").contains("failed to parse YAML config"),
            "non-integer threshold must fail at parse: {err:#}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
