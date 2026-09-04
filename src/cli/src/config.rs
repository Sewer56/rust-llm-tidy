//! Configuration: YAML config file parsing, glob compilation, and runtime
//! per-file policy computation for `rust-llm-tidy`.
//!
//! A config file (`.rust-llm-tidy.yml`) lets users exclude files from all
//! processing, whitelist or blacklist specific lint/fix rules per path, and run
//! external post-processing commands (e.g. `rustfmt`) on every processed file.
//!
//! All patterns are globs relative to the config file's directory and are
//! compiled with `literal_separator(true)`, so `*` does not cross `/` and
//! `**` recurses across directories.
//!
//! Files outside the config directory never match (the prefix strip fails).
//!
//! # Hard-fail policy
//!
//! Any config error - bad YAML, bad glob syntax, unknown rule name, a
//! `links` value below 1, a malformed `extensions`/`extra_extensions`
//! entry, or a pattern matching zero files - causes [`load_and_compile`]
//! to return `Err`.
//!
//! The CLI propagates that error as a non-zero exit on every command.
//!
//! The `--validate` flag exists for CI to check the config without processing
//! files.

use anyhow::{Context, anyhow, bail};
use glob::glob as fs_glob;
use globset::{GlobBuilder, GlobSet};
use rust_llm_tidy_lint::check::LINT_CODES;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// Fix/operation names that can be disabled/excluded. Kept in sync with the
/// per-file dispatch in `main.rs` (`fix_file`, `reorder_file`, `vis_file`,
/// `check_file`). `lints` gates the lint pass.
pub const KNOWN_FIX_OPS: &[&str] = &["tables", "fences", "links", "reorder", "vis", "lints"];

/// A loaded and validated config, ready to answer `policy_for` queries.
#[derive(Debug)]
pub struct CompiledConfig {
    /// Canonicalized directory of the config file. Patterns are resolved
    /// relative to this.
    config_dir: PathBuf,
    /// Matches `exclude_files` patterns.
    exclude_files_set: GlobSet,
    /// One group per `include` entry (whitelist mode).
    include_groups: Vec<CompiledRuleGroup>,
    /// One group per `exclude` entry (blacklist mode).
    exclude_groups: Vec<CompiledRuleGroup>,
    /// Stored so the CLI can run the post-processing pass without re-parsing.
    post_process: Vec<PostProcessStep>,
    /// Link-hoist threshold settings (`None` = always hoist at threshold 1).
    links: Option<LinkConfig>,
    /// Replacement list from the `extensions:` key; empty = the registry
    /// defaults stay admitted.
    extensions: Vec<String>,
    /// Additions from the `extra_extensions:` key, admitted on top of the
    /// effective base list.
    extra_extensions: Vec<String>,
}

/// Raw serde view of `.rust-llm-tidy.yml`. Paths/globs are relative to the
/// config file's directory.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)] // Reject hallucinated config keys at parse time.
pub struct Config {
    /// Whitelist: for matched paths, run ONLY these rules. Mutually exclusive
    /// with `exclude` (both present -> config-load error). Empty/absent = not
    /// whitelist mode.
    #[serde(default)]
    pub include: Vec<RuleGroup>,
    /// Blacklist: for matched paths, never run these rules. Mutually exclusive
    /// with `include`.
    #[serde(default)]
    pub exclude: Vec<RuleGroup>,
    /// Skip ALL processing for files matching any pattern (was `exclude`).
    #[serde(default)]
    pub exclude_files: Vec<String>,
    /// External commands run on every processed file after rust-llm-tidy.
    #[serde(default)]
    pub post_process: Vec<PostProcessStep>,
    /// Link-hoist threshold settings. Absent = always hoist (threshold 1).
    #[serde(default)]
    pub links: Option<LinkConfig>,
    /// The full admitted-extension list, replacing the registry defaults when
    /// non-empty (an empty or absent list keeps the defaults). Entries are
    /// written without the leading dot and matched case-insensitively.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Extra file extensions admitted in addition to the effective base
    /// (`extensions:` when non-empty, else the defaults). Entries follow the
    /// same rules as `extensions`.
    #[serde(default)]
    pub extra_extensions: Vec<String>,
}

/// Runtime policy for a single file: whether to skip it entirely, which ops are
/// enabled, and (for blacklist/default mode) which rules are disabled.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FilePolicy {
    /// Matched by an `exclude_files` pattern.
    pub skip: bool,
    /// Ops/rules enabled for this file (whitelist mode) or `None` for the
    /// blacklist/default mode (caller disables via `disabled`).
    pub enabled: Option<HashSet<String>>,
    /// Union of `rules` from all matched `exclude` groups (blacklist/default
    /// mode). Empty in whitelist mode.
    pub disabled: HashSet<String>,
}

/// A compiled `include`/`exclude` group: one glob set plus its rule names.
#[derive(Debug)]
struct CompiledRuleGroup {
    set: GlobSet,
    rules: Vec<String>,
}

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

/// One external post-processing step. The processed file path is appended as
/// the last argument by the CLI's `run_post_process` (see `main.rs`).
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)] // Reject hallucinated config keys at parse time.
pub struct PostProcessStep {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Empty = run on every file regardless of extension.
    #[serde(default)]
    pub extensions: Vec<String>,
}

/// One entry under `include` or `exclude`: a list of path globs and the rule
/// names to (include|exclude) for files they match. An omitted `paths` matches
/// every file (implied `["**"]`).
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)] // Reject hallucinated config keys at parse time.
pub struct RuleGroup {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
}

impl CompiledConfig {
    /// Borrow the post-processing steps so the CLI can run them after the
    /// per-file loop.
    pub fn post_process_steps(&self) -> &[PostProcessStep] {
        &self.post_process
    }

    /// The replacement extension list from the `extensions:` key. Empty means
    /// the registry defaults stay admitted.
    pub fn extension_override(&self) -> &[String] {
        &self.extensions
    }

    /// The user-added extensions from the `extra_extensions:` key, admitted
    /// in addition to the effective base list.
    pub fn extra_extensions(&self) -> &[String] {
        &self.extra_extensions
    }

    /// Effective link-hoist threshold for files with extension `ext` (no
    /// leading dot): `by_extension[ext]`, else the global `min_occurrences`,
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
    let cwd = std::env::current_dir().ok()?;
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

/// Read, parse, validate, and compile the config at `path`.
///
/// Steps:
/// 1. Read the file and `serde_yml::from_str` it (YAML error -> `Err`).
/// 2. Canonicalize the config directory (patterns resolve relative to it).
/// 3. Reject `include` + `exclude` co-presence (xor).
/// 4. Validate every `extensions` and `extra_extensions` entry shape via
///    [`crate::langs::validate_extension`].
/// 5. Validate every rule name against `known_rules()`; compile each group's
///    patterns into a `GlobSet` with `literal_separator(true)`. An
///    empty/missing `paths` in a group is treated as `["**"]`.
/// 6. Compile the `exclude_files` patterns into one `GlobSet`.
/// 7. Semantic check: expand each pattern via `glob::glob()` joined with
///    `config_dir`; a pattern yielding zero results is stale -> `Err`.
///
/// # Arguments
///
/// - `path`: the path to the `.rust-llm-tidy.yml` config file to load and
///   compile.
///
/// # Errors
///
/// Returns `anyhow::Error` if:
/// - The file cannot be read or parsed as YAML.
/// - The config path has no parent directory.
/// - The config directory cannot be canonicalized.
/// - `include` and `exclude` are both non-empty.
/// - Any `extensions` or `extra_extensions` entry is empty or contains a dot,
///   a path separator, or whitespace.
/// - Any rule name is not in [`known_rules()`].
/// - Any glob pattern has invalid syntax.
/// - Any pattern matches zero files under the config directory.
///
/// On success, returns a [`CompiledConfig`] ready for `policy_for`.
pub fn load_and_compile(path: &Path) -> anyhow::Result<CompiledConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    let config: Config = serde_yml::from_str(&raw)
        .with_context(|| format!("failed to parse YAML config {}", path.display()))?;

    let config_parent = path
        .parent()
        .with_context(|| format!("config path {} has no parent", path.display()))?;
    let config_dir = if config_parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        config_parent
    }
    .canonicalize()
    .with_context(|| format!("failed to canonicalize config dir {}", path.display()))?;

    // XOR: include + exclude both present -> error.
    if !config.include.is_empty() && !config.exclude.is_empty() {
        bail!("cannot use `include` (whitelist) and `exclude` (blacklist) together; pick one");
    }

    // Extension-list entries must be shaped like real path extensions; a
    // malformed entry fails the run instead of being silently ignored.
    for ext in config.extensions.iter().chain(&config.extra_extensions) {
        crate::langs::validate_extension(ext)?;
    }

    // Link thresholds: every value must be >= 1 (a missing `min_occurrences`
    // already defaults to 1). A non-integer value fails YAML deserialization
    // above, so only a literal 0 reaches this check.
    if let Some(links) = &config.links {
        if links.min_occurrences < 1 {
            bail!(
                "links.min_occurrences must be >= 1, got {}",
                links.min_occurrences
            );
        }
        for (ext, &count) in &links.by_extension {
            if count < 1 {
                bail!("links.by_extension.{ext} must be >= 1, got {count}");
            }
        }
    }

    let valid = known_rules();

    // Validate rule names + compile include groups.
    let mut include_groups: Vec<CompiledRuleGroup> = Vec::with_capacity(config.include.len());
    for rule in &config.include {
        for r in &rule.rules {
            if !valid.contains(&r.as_str()) {
                bail!(
                    "unknown rule `{r}` in include.rules; valid rules: {}",
                    valid.join(", ")
                );
            }
        }
        let paths = if rule.paths.is_empty() {
            vec!["**".to_string()]
        } else {
            rule.paths.clone()
        };
        let set = compile_glob_set(&paths, &config_dir)?;
        include_groups.push(CompiledRuleGroup {
            set,
            rules: rule.rules.clone(),
        });
    }

    // Validate rule names + compile exclude groups.
    let mut exclude_groups: Vec<CompiledRuleGroup> = Vec::with_capacity(config.exclude.len());
    for rule in &config.exclude {
        for r in &rule.rules {
            if !valid.contains(&r.as_str()) {
                bail!(
                    "unknown rule `{r}` in exclude.rules; valid rules: {}",
                    valid.join(", ")
                );
            }
        }
        let paths = if rule.paths.is_empty() {
            vec!["**".to_string()]
        } else {
            rule.paths.clone()
        };
        let set = compile_glob_set(&paths, &config_dir)?;
        exclude_groups.push(CompiledRuleGroup {
            set,
            rules: rule.rules.clone(),
        });
    }

    let exclude_files_set = compile_glob_set(&config.exclude_files, &config_dir)?;

    // Semantic check: every pattern must match at least one file when expanded
    // against the filesystem from `config_dir`.
    for pat in &config.exclude_files {
        check_pattern_matches(&config_dir, pat)?;
    }
    for group in &config.include {
        for pat in &group.paths {
            check_pattern_matches(&config_dir, pat)?;
        }
    }
    for group in &config.exclude {
        for pat in &group.paths {
            check_pattern_matches(&config_dir, pat)?;
        }
    }

    Ok(CompiledConfig {
        config_dir,
        exclude_files_set,
        include_groups,
        exclude_groups,
        post_process: config.post_process,
        links: config.links,
        extensions: config.extensions,
        extra_extensions: config.extra_extensions,
    })
}

/// Return every rule name accepted by `include.rules`, `exclude.rules`,
/// `--include`, and `--exclude`: lint codes followed by fix/operation names.
/// The CLI validates rule names against this list.
pub fn known_rules() -> Vec<&'static str> {
    let mut rules: Vec<&'static str> = LINT_CODES.to_vec();
    rules.extend_from_slice(KNOWN_FIX_OPS);
    rules
}

/// Expand `pattern` joined with `config_dir` via `glob::glob()` and require at
/// least one match. Descends only the pattern's prefix subtree, so cost scales
/// with the number/depth of patterns, not repo size.
fn check_pattern_matches(config_dir: &Path, pattern: &str) -> anyhow::Result<()> {
    let full = config_dir.join(pattern);
    let full_str = full.to_string_lossy().into_owned();
    let mut matches = fs_glob(&full_str)
        .map_err(|e| anyhow!("invalid glob pattern `{pattern}`: {e}"))?
        .filter_map(Result::ok);
    if matches.next().is_none() {
        bail!(
            "config pattern `{pattern}` matched no files under {}",
            config_dir.display()
        );
    }
    Ok(())
}

/// Build a `GlobSet` from `patterns`, each compiled with `literal_separator(true)`.
fn compile_glob_set(patterns: &[String], _config_dir: &Path) -> anyhow::Result<GlobSet> {
    let mut builder = GlobSet::builder();
    for p in patterns {
        let g = GlobBuilder::new(p)
            .literal_separator(true)
            .build()
            .with_context(|| format!("invalid glob pattern `{p}`"))?;
        builder.add(g);
    }
    builder
        .build()
        .map_err(|e| anyhow!("failed to build glob set: {e}"))
}

/// `serde` default helper: an absent `min_occurrences` means threshold 1
/// (always hoist).
fn default_one() -> usize {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    static COMPILE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    /// Write a YAML config and a sibling matching file under a temp dir, then
    /// load+compile. Returns the `CompiledConfig`. The temp dir is NOT cleaned
    /// up here so callers can exercise `policy_for` on existing files.
    fn compile(yaml: &str, files: &[(&str, &str)]) -> CompiledConfig {
        let dir = std::env::temp_dir().join(format!(
            "rlt-cfg-unit-{}-{}",
            std::process::id(),
            COMPILE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed,),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(body.as_bytes()).unwrap();
        }
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, yaml).unwrap();
        load_and_compile(&cfg_path).expect("config should compile")
    }

    #[test]
    fn empty_config_compiles_to_no_op() {
        let cc = compile(
            "exclude_files: []\n",
            &[("src/lib.rs", "pub fn example() {}\n")],
        );
        // Use a file that actually exists inside the config dir so
        // canonicalize succeeds and the no-pattern-match path is exercised.
        let dir = cc.config_dir_canonical_for_test();
        let policy = cc.policy_for(&dir.join("src").join("lib.rs"));
        assert!(!policy.skip);
        assert!(policy.disabled.is_empty());
        assert_eq!(policy.enabled, None);
    }

    #[test]
    fn bad_glob_syntax_is_rejected() {
        // `[` opens an unclosed character class across both `globset` and
        // `glob`, so this fails at compile time.
        let dir = std::env::temp_dir().join(format!("rlt-cfg-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The pattern must be invalid regardless of matching files.
        std::fs::write(dir.join("a.rs"), "pub fn x() {}\n").unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, "exclude_files:\n  - \"[unclosed\"\n").unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("invalid glob pattern") || msg.contains("glob"),
            "bad glob syntax should surface as an error: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_rule_is_rejected() {
        let dir = std::env::temp_dir().join(format!("rlt-cfg-rule-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("lib.rs"), "pub fn x() {}\n").unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(
            &cfg_path,
            "exclude:\n  - paths: [\"lib.rs\"]\n    rules: [\"BOGUS\"]\n",
        )
        .unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("unknown rule") && msg.contains("BOGUS"),
            "unknown rule should be reported: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_matching_pattern_is_rejected() {
        let dir = std::env::temp_dir().join(format!("rlt-cfg-nomatch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, "exclude_files:\n  - \"nope/**\"\n").unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("matched no files"),
            "non-matching pattern should be reported: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn literal_separator_star_does_not_cross_slash() {
        // `*.rs` must match a file directly under the config dir, but NOT a
        // file nested under a subdirectory (because `*` does not cross `/`).
        let cc = compile(
            "exclude_files:\n  - \"*.rs\"\n",
            &[
                ("top.rs", "pub fn top() {}\n"),
                ("sub/nested.rs", "pub fn nested() {}\n"),
            ],
        );
        let dir = cc.config_dir_canonical_for_test();
        let top = dir.join("top.rs");
        let nested = dir.join("sub").join("nested.rs");
        assert!(
            cc.policy_for(&top).skip,
            "*.rs should match a top-level .rs file"
        );
        assert!(
            !cc.policy_for(&nested).skip,
            "*.rs must NOT cross / and match a nested file"
        );
    }

    #[test]
    fn known_rules_lists_every_code_and_op() {
        let rules = known_rules();
        // The nine lint codes plus the six fix/operation names (including lints).
        for code in [
            "DOC001", "DOC002", "DOC003", "DOC004", "DOC005", "DOC006", "DOC007", "DOC008",
            "TEST001",
        ] {
            assert!(rules.contains(&code), "missing lint code {code}");
        }
        for op in ["tables", "fences", "links", "reorder", "vis", "lints"] {
            assert!(rules.contains(&op), "missing fix/operation {op}");
        }
    }

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
