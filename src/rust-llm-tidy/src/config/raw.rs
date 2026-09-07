//! Deserialized `.rust-llm-tidy.yml` model (`Config`) and its rule groups.

use super::{LinkConfig, ModuleSizeConfig, PassiveNarrationConfig, PostProcessStep};
use serde::Deserialize;

/// Raw serde view of `.rust-llm-tidy.yml`. Paths/globs are relative to the
/// config file's directory.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)] // Reject hallucinated config keys at parse time.
pub struct Config {
    /// Whitelist: for matched paths, run ONLY these rules.
    ///
    /// - Mutually exclusive with `exclude` (both present -> config-load error).
    /// - Empty/absent = not whitelist mode.
    #[serde(default)]
    pub include: Vec<RuleGroup>,
    /// Blacklist: for matched paths, never run these rules. Mutually exclusive
    /// with `include`.
    #[serde(default)]
    pub exclude: Vec<RuleGroup>,
    /// Skip ALL processing for files matching any pattern (was `exclude`).
    #[serde(default)]
    pub exclude_files: Vec<String>,
    /// Skip conventional license documents during discovery. Default: `true`.
    ///
    /// Setting `false` retains extension filters and `exclude_files` rules.
    #[serde(default = "default_true")]
    pub exclude_license_documents: bool,
    /// External commands run on every processed file after rust-llm-tidy.
    #[serde(default)]
    pub post_process: Vec<PostProcessStep>,
    /// Link-hoist threshold settings. Absent = always hoist (threshold 1).
    #[serde(default)]
    pub links: Option<LinkConfig>,
    /// Module-size threshold settings. Absent = the default threshold 500.
    #[serde(default)]
    pub module_size: Option<ModuleSizeConfig>,
    /// Full allowed-extension list, replacing the defaults when non-empty
    /// (empty keeps the defaults). No leading dot; case-insensitive.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Extra extensions allowed in addition to the effective base
    /// (`extensions` when non-empty, else the defaults).
    #[serde(default)]
    pub extra_extensions: Vec<String>,
    /// Settings under the top-level `passive_narration` key; absent keeps
    /// the section defaults (see [`PassiveNarrationConfig`]).
    #[serde(default)]
    pub passive_narration: Option<PassiveNarrationConfig>,
}

/// One entry under `include` or `exclude`: path globs plus the rule names to
/// (include|exclude) for files they match.
///
/// An omitted `paths` matches every file (implied `["**"]`).
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)] // Reject hallucinated config keys at parse time.
pub struct RuleGroup {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            exclude_files: Vec::new(),
            exclude_license_documents: true,
            post_process: Vec::new(),
            links: None,
            module_size: None,
            extensions: Vec::new(),
            extra_extensions: Vec::new(),
            passive_narration: None,
        }
    }
}

/// `serde` default helper: an absent key keeps its default `true` value.
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::Config;
    use crate::config::compiled::load::compile;

    /// YAML defaults keep license-document exclusion on until disabled.
    #[test]
    fn license_exclusion_should_default_to_enabled() {
        assert!(Config::default().exclude_license_documents);
        assert!(compile("{}\n", &[]).exclude_license_documents());
        assert!(compile("exclude_license_documents: true\n", &[]).exclude_license_documents());
        assert!(!compile("exclude_license_documents: false\n", &[]).exclude_license_documents());
    }
}
