//! The validated, glob-compiled config that answers runtime policy queries.
//!
//! # Module map
//!
//! - `load`: read, validate, and compile a config file into [`CompiledConfig`]

// `load` is `pub(super)` so sibling `config` unit tests can reuse its
// `compile` fixture helper; `compiled` itself stays private.
use super::{
    FilePolicy, LinkConfig, MethodLengthConfig, ModuleSizeConfig, PassiveNarrationConfig, PerfCode,
    PostProcessStep, ReportingScope,
};
use globset::GlobSet;
pub use load::load_and_compile;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use symbol_rules::CompiledSymbolRule;

pub(super) mod load;
pub(crate) mod symbol_rules;

/// A loaded and validated config, ready to answer `policy_for` queries.
#[derive(Debug)]
pub struct CompiledConfig {
    /// Canonicalized directory of the config file. Patterns are resolved
    /// relative to this.
    config_dir: PathBuf,
    /// Matches `exclude_files` patterns.
    exclude_files_set: GlobSet,
    /// Skip conventional license documents during discovery.
    exclude_license_documents: bool,
    /// One group per `include` entry (whitelist mode).
    include_groups: Vec<CompiledRuleGroup>,
    /// One group per `exclude` entry (blacklist mode).
    exclude_groups: Vec<CompiledRuleGroup>,
    /// Stored so file execution can run post-processing without re-parsing.
    post_process: Vec<PostProcessStep>,
    /// Link-hoist threshold settings (`None` = always hoist at threshold 1).
    links: Option<LinkConfig>,
    /// Module-size threshold settings (`None` = default threshold 500).
    module_size: Option<ModuleSizeConfig>,
    /// Method-length threshold settings (`None` = default threshold 100).
    method_length: Option<MethodLengthConfig>,
    /// Replacement list from the `extensions:` key; empty = keep the defaults.
    extensions: Vec<String>,
    /// Additions from the `extra_extensions:` key, allowed on top of the
    /// effective base list.
    extra_extensions: Vec<String>,
    /// Resolved `passive_narration` settings (section defaults when the
    /// top-level key is absent).
    passive_narration: PassiveNarrationConfig,
    /// Selected built-in families; absent enables every built-in family.
    configured_perf_hints: Option<Vec<PerfCode>>,
    /// Validated overrides, separate from lint enablement.
    lint_scopes: HashMap<String, ReportingScope>,
    /// Symbol patterns compiled once when the configuration is loaded.
    symbol_rules: Vec<CompiledSymbolRule>,
}

/// A compiled `include`/`exclude` group: one glob set plus its rule names.
#[derive(Debug)]
struct CompiledRuleGroup {
    set: GlobSet,
    rules: Vec<String>,
}

impl CompiledConfig {
    /// Configured reporting override; absent leaves the severity default intact.
    pub(crate) fn scope_for(&self, code: &str) -> Option<ReportingScope> {
        self.lint_scopes.get(code).copied()
    }

    /// Compiled symbol policies in configuration order.
    pub(crate) fn symbol_rules(&self) -> &[CompiledSymbolRule] {
        &self.symbol_rules
    }

    /// Whether discovery skips conventional license documents.
    pub(crate) fn exclude_license_documents(&self) -> bool {
        self.exclude_license_documents
    }

    /// Whether to apply `passive_narration.suppress_in_release_notes`.
    pub(crate) fn suppress_in_release_notes(&self) -> bool {
        self.passive_narration.suppress_in_release_notes
    }

    /// Whether `passive_narration.enable` permits TEXT007 by default.
    pub(crate) fn passive_narration(&self) -> bool {
        self.passive_narration.enable
    }

    /// Resolve omitted selection to all built-in families.
    pub(crate) fn perf_hints(&self) -> &[PerfCode] {
        self.configured_perf_hints
            .as_deref()
            .unwrap_or(PerfCode::ALL)
    }

    /// Borrow the post-processing steps so the pipeline can run them after the
    /// per-file loop.
    pub fn post_process_steps(&self) -> &[PostProcessStep] {
        &self.post_process
    }

    /// The replacement extension list from the `extensions:` key; empty =
    /// keep the defaults.
    pub fn extension_override(&self) -> &[String] {
        &self.extensions
    }

    /// The user-added extensions from the `extra_extensions:` key, allowed
    /// in addition to the effective base list.
    pub fn extra_extensions(&self) -> &[String] {
        &self.extra_extensions
    }

    /// Effective link-hoist threshold for files with extension `ext` (no
    /// leading dot).
    ///
    /// Lookup order: `by_extension[ext]`, else the global `min_occurrences`,
    /// else 1. `ext` is matched exactly against the config's extension keys.
    pub fn links_min_occurrences_for(&self, ext: &str) -> usize {
        match &self.links {
            None => 1,
            Some(links) => links
                .by_extension
                .get(ext)
                .copied()
                .unwrap_or(links.min_occurrences),
        }
    }

    /// Effective MOD001 line budget.
    ///
    /// Lookup order: `module_size.max_lines`, else the default 500.
    pub fn module_size_max_lines(&self) -> usize {
        self.module_size().max_lines
    }

    /// Resolve the file-size policy, retaining defaults for an absent section.
    pub(crate) fn module_size(&self) -> ModuleSizeConfig {
        self.module_size.unwrap_or_default()
    }

    /// Resolve the method-length policy, retaining defaults for an absent
    /// section.
    pub(crate) fn method_length(&self) -> MethodLengthConfig {
        self.method_length.unwrap_or_default()
    }

    /// Test-only accessor for the canonicalized config directory. Used by the
    /// unit tests to reconstruct canonical paths matching `policy_for`.
    #[cfg(test)]
    pub fn config_dir_canonical_for_test(&self) -> &Path {
        &self.config_dir
    }

    /// Compute the [`FilePolicy`] for `file`.
    ///
    /// `file` is canonicalized, the `config_dir` prefix is stripped, and the
    /// relative path is tested against every compiled glob set. A file outside
    /// `config_dir` (prefix strip fails) returns an empty policy.
    pub fn policy_for(&self, file: &Path) -> FilePolicy {
        let Ok(canon) = file.canonicalize() else {
            return FilePolicy::default();
        };
        let Some(rel) = canon.strip_prefix(&self.config_dir).ok() else {
            return FilePolicy::default();
        };
        let rel_str = rel.to_string_lossy();
        let mut policy = FilePolicy::default();
        if self.exclude_files_set.is_match(&*rel_str) {
            policy.skip = true;
        }
        let matched_include: HashSet<String> = self
            .include_groups
            .iter()
            .filter(|g| g.set.is_match(&*rel_str))
            .flat_map(|g| g.rules.iter().cloned())
            .collect();
        let matched_exclude: HashSet<String> = self
            .exclude_groups
            .iter()
            .filter(|g| g.set.is_match(&*rel_str))
            .flat_map(|g| g.rules.iter().cloned())
            .collect();
        if !self.include_groups.is_empty() {
            // Whitelist mode: a file matching NO include group runs nothing.
            policy.enabled = Some(matched_include);
        } else {
            // Blacklist/default mode: disable matched_exclude rules.
            policy.disabled = matched_exclude;
            policy.enabled = None;
        }
        policy
    }
}

#[cfg(test)]
mod tests {
    use super::load::compile;

    /// `MOD003` resolves as a rule name in both whitelist and blacklist
    /// groups.
    #[test]
    fn mod003_should_be_valid_in_rule_groups() {
        let include = compile(
            "include:\n  - paths: [\"lib.rs\"]\n    rules: [\"MOD003\"]\n",
            &[("lib.rs", "pub fn x() {}\n")],
        );
        let dir = include.config_dir_canonical_for_test();
        let policy = include.policy_for(&dir.join("lib.rs"));
        assert!(
            policy
                .enabled
                .as_ref()
                .is_some_and(|rules| rules.contains("MOD003")),
            "include should enable MOD003: {policy:?}"
        );

        let exclude = compile(
            "exclude:\n  - paths: [\"lib.rs\"]\n    rules: [\"MOD003\"]\n",
            &[("lib.rs", "pub fn x() {}\n")],
        );
        let dir = exclude.config_dir_canonical_for_test();
        let policy = exclude.policy_for(&dir.join("lib.rs"));
        assert!(
            policy.disabled.contains("MOD003"),
            "exclude should disable MOD003: {policy:?}"
        );
    }

    #[test]
    fn policy_for_matches_relative_path() {
        let cc = compile(
            "exclude_files:\n  - \"src/lib.rs\"\nexclude:\n  - paths: [\"src/lib.rs\"]\n    rules: [\"links\"]\n",
            &[("src/lib.rs", "pub fn example() {}\n")],
        );
        // Re-open the same path the compile helper used to canonicalize.
        let dir = cc.config_dir_canonical_for_test();
        let lib = dir.join("src").join("lib.rs");
        let policy = cc.policy_for(&lib);
        assert!(policy.skip, "exclude_files should mark the file skipped");
        assert!(
            policy.disabled.contains("links"),
            "exclude should disable `links`: {policy:?}"
        );
    }

    #[test]
    fn file_outside_config_dir_returns_empty_policy() {
        let cc = compile(
            "exclude_files:\n  - \"**\"\n",
            &[("src/lib.rs", "pub fn example() {}\n")],
        );
        // A file that exists but is outside the config dir yields an empty
        // policy via the strip_prefix failure path (canonicalize succeeds).
        let outside_dir =
            std::env::temp_dir().join(format!("rlt-cfg-outside-dir-{}", std::process::id()));
        std::fs::create_dir_all(&outside_dir).unwrap();
        let outside = outside_dir.join("outside.rs");
        std::fs::write(&outside, "pub fn x() {}\n").unwrap();
        let policy = cc.policy_for(&outside);
        assert!(!policy.skip);
        assert!(policy.disabled.is_empty());
        let _ = std::fs::remove_dir_all(&outside_dir);
    }
}
