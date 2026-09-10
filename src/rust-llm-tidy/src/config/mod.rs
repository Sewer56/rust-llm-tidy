//! Configuration: YAML config file parsing, glob compilation, and runtime
//! per-file policy computation for `rust-llm-tidy`.
//!
//! A config file (`.rust-llm-tidy.yml`) lets users exclude files from all
//! processing, whitelist or blacklist specific lint/fix rules per path, and run
//! external post-processing commands (e.g. `rustfmt`) on every processed file.
//!
//! File patterns are globs relative to the config file's directory. They are
//! compiled with `literal_separator(true)`, so `*` does not cross `/` and
//! `**` recurses across directories.
//!
//! Files outside the config directory never match (the prefix strip fails).
//!
//! # Hard-fail policy
//!
//! Any config error causes [`load_and_compile`] to return `Err`.
//!
//! Errors include bad YAML, bad glob syntax, unknown rule name, or a
//! `links`, `module_size`, or `method_length` value below 1. They also
//! include a malformed `extensions`/`extra_extensions` entry or a pattern
//! matching zero files.
//!
//! Duplication thresholds require at least one meaningful line and two sites.
//!
//! Symbol rules validate their shape and bounded regex
//! compilation, not whether any source symbol currently matches them.
//!
//! The CLI propagates that error as a non-zero exit on every command.
//!
//! The `--validate` flag exists for CI to check the config without processing
//! files.
//!
//! # Module map
//!
//! - `raw`: the deserialized `Config` model and its `RuleGroup` entries
//! - `compiled`: the validated [`CompiledConfig`] answering `policy_for`
//!   queries, plus `load_and_compile` (in `compiled::load`)
//! - `file_policy`: the runtime [`FilePolicy`] computed per file
//!
//! Rule settings:
//!
//! - `link_config`: `links` hoist-threshold settings
//! - `duplication_config`: run-wide textual-duplication settings
//! - `method_length_config`: `method_length` threshold settings
//! - `module_size_config`: `module_size` threshold settings
//! - `passive_narration_config`: TEXT007 enablement and suppression settings
//! - `perf_code`: built-in symbol-family selection
//! - `symbol_rule`: syntax-only symbol hints and declaration exclusions
//! - `lint_scope`: shared reporting boundaries
//! - `post_process_step`: one external post-processing command

use crate::rules::lint::LINT_CODES;
pub use crate::rules::registry::KNOWN_FIX_OPS;
pub use compiled::CompiledConfig;
pub use compiled::load_and_compile;
pub(crate) use compiled::symbol_rules::compile_symbol_rules;
pub(crate) use compiled::symbol_rules::{CompiledSymbolRule, SymbolMatcher};
pub use duplication_config::DuplicationConfig;
pub use file_policy::FilePolicy;
pub use forbidden_character_rule::{ForbiddenCharacterRule, ForbiddenCharacterScope};
pub use link_config::LinkConfig;
pub use lint_scope::ReportingScope;
pub use method_length_config::MethodLengthConfig;
pub use module_size_config::ModuleSizeConfig;
pub use passive_narration_config::PassiveNarrationConfig;
pub use perf_code::PerfCode;
pub use post_process_step::PostProcessStep;
pub use raw::{Config, RuleGroup};
use std::env;
use std::path::{Path, PathBuf};
pub use symbol_rule::{
    ArrayKind, DeclarationLintExclusion, RegexComments, SymbolAction, SymbolLanguage, SymbolRule,
    SymbolTarget,
};

mod compiled;
mod duplication_config;
mod file_policy;
pub(crate) mod forbidden_character_rule;
mod link_config;
mod lint_scope;
mod method_length_config;
mod module_size_config;
mod passive_narration_config;
mod perf_code;
mod post_process_step;
mod raw;
mod symbol_rule;

/// Resolve the config file path.
///
/// - `no_config == true` -> `None`.
/// - Explicit `arg` -> that path (used as-is).
/// - Else walk up from `std::env::current_dir()` towards the filesystem root.
///   At each level checked (including the starting dir), look for
///   `.rust-llm-tidy.yml`; the first one found wins. Stop at the first ancestor
///   that contains a `.git` entry (the repo root) if no config appeared there;
///   if no `.git` is found, continue to the filesystem root. Returns `None`
///   when no config file is found.
///
/// # Arguments
///
/// - `arg`: an explicit config path from `--config`, or `None` to use
///   auto-discovery.
/// - `no_config`: when `true`, disables discovery and loading entirely and
///   returns `None`.
pub fn discover_config_path(arg: Option<&Path>, no_config: bool) -> Option<PathBuf> {
    if no_config {
        return None;
    }
    if let Some(p) = arg {
        return Some(p.to_path_buf());
    }
    let cwd = env::current_dir().ok()?;
    let mut dir: &Path = &cwd;
    loop {
        let candidate = dir.join(".rust-llm-tidy.yml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if dir.join(".git").exists() {
            // Reached the repo root without finding a config; stop walking up.
            return None;
        }
        dir = dir.parent()?;
    }
}

/// Return every rule name accepted by `include.rules`, `exclude.rules`,
/// `--include`, and `--exclude`.
///
/// The list holds lint codes followed by fix/operation names; the CLI
/// validates rule names against it.
pub fn known_rules() -> Vec<&'static str> {
    let mut rules: Vec<&'static str> = LINT_CODES.to_vec();
    rules.extend_from_slice(KNOWN_FIX_OPS);
    rules
}

#[cfg(test)]
mod tests {
    use super::known_rules;

    #[test]
    fn known_rules_lists_every_code_and_op() {
        let rules = known_rules();
        // Sample lint codes from every prefix group (DOC, TEXT, TEST, and
        // MOD), plus the six fix/operation names (including lints).
        for code in [
            "DOC001", "DOC002", "DOC003", "DOC004", "DOC005", "DOC006", "DOC008", "DOC009",
            "TEXT001", "TEXT002", "TEXT003", "TEXT004", "TEST001", "MOD002", "MOD003", "SYM",
        ] {
            assert!(rules.contains(&code), "missing lint code {code}");
        }
        // DOC007 stays retired; DOC008 is now a live rule name (D1).
        assert!(
            !rules.contains(&"DOC007"),
            "retired code DOC007 must not resolve as a rule name"
        );
        for op in ["tables", "fences", "links", "reorder", "vis", "lints"] {
            assert!(rules.contains(&op), "missing fix/operation {op}");
        }
    }
}
