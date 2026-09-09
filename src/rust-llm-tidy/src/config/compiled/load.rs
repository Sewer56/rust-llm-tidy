//! Read, parse, validate, and compile a config file into a `CompiledConfig`.

use super::symbol_rules::compile_symbol_rules;
use super::{CompiledConfig, CompiledRuleGroup};
use crate::config::forbidden_character_rule::validate;
use crate::config::{Config, RuleGroup, known_rules};
use crate::languages::registry;
use crate::rules::registry::LINT_CODES;
use anyhow::{Context, anyhow, bail};
use glob::glob as fs_glob;
use globset::{GlobBuilder, GlobSet};
use std::fs;
#[cfg(test)]
use std::io::Write;
use std::path::{Path, PathBuf};

/// Shared unit-test counter backing [`compile`]'s unique temp dirs.
#[cfg(test)]
static COMPILE_COUNTER: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Read, parse, validate, and compile the config at `path`.
///
/// Steps:
/// 1. Read the file and `serde_yml::from_str` it (YAML error -> `Err`).
/// 2. Canonicalize the config directory (patterns resolve relative to it).
/// 3. Reject `include` + `exclude` co-presence (xor).
/// 4. Validate every `extensions` and `extra_extensions` entry shape via
///    the language registry's extension validation.
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
/// - The config path has no parent directory or cannot be canonicalized.
/// - `include` and `exclude` are both non-empty.
/// - An extension entry is empty or contains a dot, path separator, or whitespace.
/// - `module_size.max_lines`, `method_length.max_lines`, a link occurrence
///   threshold, or `duplication.min_meaningful_lines` is below 1, or
///   `duplication.min_occurrences` is below 2.
/// - `perf_hints` contains anything other than `PERF001` or `PERF002` codes.
/// - A forbidden-character entry has no characters, a blank title or message,
///   or a character appears more than once across the entries.
///
/// Symbol policy errors:
///
/// - Any rule name is not in [`known_rules()`].
/// - A `lint_scopes` key is not a registered lint code.
/// - A symbol rule has both or neither `symbol` and `regex`, a blank pattern,
///   or an invalid literal path.
/// - Regex compilation rejects syntax or exceeds dependency-default limits.
/// - A symbol hint lacks a nonblank title or message, or an exclusion has a
///   title, message, scope, usage target, or usage constraint.
/// - A hint has `exclude_lints`, `exclude_edits`, or `exclude_post_process`.
/// - `exclude_lints` lists a code absent from the registered lint codes.
///
/// Symbol selector errors:
///
/// - A declaration hint has an argument, initializer, or array-kind constraint.
/// - Symbol languages or extensions are empty, malformed, or unsupported.
/// - A usage regex has languages, argument, initializer, or array-kind constraints.
/// - A literal or declaration matcher has comment selection.
///
/// File pattern errors:
///
/// - Any glob pattern has invalid syntax.
/// - Any pattern matches zero files under the config directory.
///
/// On success, returns a [`CompiledConfig`] ready for `policy_for`.
pub fn load_and_compile(path: &Path) -> anyhow::Result<CompiledConfig> {
    let (config, config_dir) = read_config_and_canonical_dir(path)?;

    // XOR: include + exclude both present -> error.
    if !config.include.is_empty() && !config.exclude.is_empty() {
        bail!("cannot use `include` (whitelist) and `exclude` (blacklist) together; pick one");
    }

    // Extension-list entries must be shaped like real path extensions; a
    // malformed entry fails the run instead of being silently ignored.
    for ext in config.extensions.iter().chain(&config.extra_extensions) {
        registry::validate_extension(ext)?;
    }

    validate_thresholds(&config)?;
    if let Some(rules) = &config.forbidden_characters {
        validate(rules)?;
    }

    for code in config.lint_scopes.keys() {
        if !LINT_CODES.contains(&code.as_str()) {
            bail!("unknown lint code `{code}` in lint_scopes; use a registered lint code");
        }
    }
    let symbol_rules = compile_symbol_rules(&config.symbol_rules)?;

    let valid = known_rules();

    let include_groups =
        compile_rule_groups(&config.include, &valid, &config_dir, "include.rules")?;
    let exclude_groups =
        compile_rule_groups(&config.exclude, &valid, &config_dir, "exclude.rules")?;

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
        forbidden_characters: config.forbidden_characters,
        config_dir,
        exclude_files_set,
        exclude_license_documents: config.exclude_license_documents,
        include_groups,
        exclude_groups,
        post_process: config.post_process,
        links: config.links,
        module_size: config.module_size,
        method_length: config.method_length,
        duplication: config.duplication,
        extensions: config.extensions,
        extra_extensions: config.extra_extensions,
        passive_narration: config.passive_narration.unwrap_or_default(),
        configured_perf_hints: config.perf_hints,
        lint_scopes: config.lint_scopes,
        symbol_rules,
    })
}

/// Write a YAML config and a sibling matching file under a temp dir, then
/// load+compile.
///
/// - Returns the `CompiledConfig`.
/// - The temp dir is NOT cleaned up here so callers can exercise `policy_for`
///   on existing files.
#[cfg(test)]
pub(crate) fn compile(yaml: &str, files: &[(&str, &str)]) -> CompiledConfig {
    let dir = std::env::temp_dir().join(format!(
        "rlt-cfg-unit-{}-{}",
        std::process::id(),
        COMPILE_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed,),
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

/// Expand `pattern` joined with `config_dir` via `glob::glob()` and require
/// at least one match.
///
/// Descends only the pattern's prefix subtree, so cost scales with the
/// number/depth of patterns, not repo size.
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

/// Validate each group's rule names against `valid` and compile its path
/// patterns into one `CompiledRuleGroup`.
///
/// `section` names the config key the groups came from (`include.rules` or
/// `exclude.rules`) in the unknown-rule error. An empty/missing `paths` in a
/// group is treated as `["**"]`.
fn compile_rule_groups(
    groups: &[RuleGroup],
    valid: &[&'static str],
    config_dir: &Path,
    section: &str,
) -> anyhow::Result<Vec<CompiledRuleGroup>> {
    let mut compiled: Vec<CompiledRuleGroup> = Vec::with_capacity(groups.len());
    for rule in groups {
        for r in &rule.rules {
            if !valid.contains(&r.as_str()) {
                bail!(
                    "unknown rule `{r}` in {section}; valid rules: {}",
                    valid.join(", ")
                );
            }
        }
        let paths = if rule.paths.is_empty() {
            vec!["**".to_string()]
        } else {
            rule.paths.clone()
        };
        let set = compile_glob_set(&paths, config_dir)?;
        compiled.push(CompiledRuleGroup {
            set,
            rules: rule.rules.clone(),
        });
    }
    Ok(compiled)
}

/// Read and parse the YAML config at `path`, returning it with the
/// canonicalized config directory its patterns resolve against.
///
/// A config path with an empty parent component (`rust-llm-tidy.yml` in the
/// working directory) resolves that directory to `.` before canonicalizing.
fn read_config_and_canonical_dir(path: &Path) -> anyhow::Result<(Config, PathBuf)> {
    let raw = fs::read_to_string(path)
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
    Ok((config, config_dir))
}

/// Reject thresholds below the minimum meaningful count for each rule.
fn validate_thresholds(config: &Config) -> anyhow::Result<()> {
    config.duplication.validate()?;

    // Link thresholds: every value must be >= 1.
    //
    // A missing `min_occurrences` already defaults to 1; a non-integer value
    // fails YAML deserialization during config parsing, so only a literal 0
    // reaches this check.
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

    // Module-size threshold: the value must be >= 1.
    //
    // A missing `max_lines` already defaults to 500; a non-integer value
    // fails YAML deserialization during config parsing, so only a literal 0
    // reaches this check.
    if let Some(module_size) = &config.module_size
        && module_size.max_lines < 1
    {
        bail!(
            "module_size.max_lines must be >= 1, got {}",
            module_size.max_lines
        );
    }

    // Method-length threshold: the value must be >= 1.
    //
    // A missing `max_lines` already defaults to 100; a non-integer value
    // fails YAML deserialization during config parsing, so only a literal 0
    // reaches this check.
    if let Some(method_length) = &config.method_length
        && method_length.max_lines < 1
    {
        bail!(
            "method_length.max_lines must be >= 1, got {}",
            method_length.max_lines
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

#[cfg(test)]
mod tests {
    use super::compile;
    use super::load_and_compile;
    use crate::config::PerfCode;
    use rstest::rstest;

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

    /// The removed MOD003 threshold key must fail as unknown, not load
    /// as a silent no-op.
    #[test]
    fn removed_qualified_paths_key_is_rejected() {
        let dir = std::env::temp_dir().join(format!("rlt-cfg-qp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, "qualified_paths:\n  repeat_threshold: 2\n").unwrap();
        let err = load_and_compile(&cfg_path).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("qualified_paths"),
            "the removed key should be reported as unknown: {msg}"
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

    #[rstest]
    #[case::omitted("{}", PerfCode::ALL)]
    #[case::both("perf_hints: [PERF001, PERF002]", PerfCode::ALL)]
    #[case::capacity("perf_hints: [PERF001]", &[PerfCode::Capacity])]
    #[case::array("perf_hints: [PERF002]", &[PerfCode::ArrayInitialization])]
    #[case::disabled("perf_hints: []", &[])]
    fn perf_hints_should_select_builtin_families(
        #[case] yaml: &str,
        #[case] expected: &[PerfCode],
    ) {
        let config = compile(yaml, &[]);

        assert_eq!(config.perf_hints(), expected);
    }

    /// Load a YAML config from a fresh temp dir and return the rendered
    /// error, if any.
    fn load_error(yaml: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "rlt-cfg-hint-{}-{}",
            std::process::id(),
            super::COMPILE_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed,)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, yaml).unwrap();
        let result = load_and_compile(&cfg_path);
        let _ = std::fs::remove_dir_all(&dir);
        match result {
            Ok(_) => panic!("config should not compile: {yaml}"),
            Err(err) => format!("{err:#}"),
        }
    }

    #[rstest]
    #[case::unknown_code("NOPE")]
    #[case::fix_operation("tables")]
    #[case::lint_group("lints")]
    #[case::retired_capacity_code("PERF001")]
    #[case::retired_array_code("PERF002")]
    fn lint_scopes_should_reject_non_lint_keys(#[case] code: &str) {
        let error = load_error(&format!("lint_scopes: {{{code}: all}}"));

        assert!(error.contains("unknown lint code") && error.contains(code));
    }

    #[rstest]
    #[case::unknown_code("perf_hints: [PERF999]", "unknown variant `PERF999`")]
    #[case::lint_code("perf_hints: [SYM]", "unknown variant `SYM`")]
    #[case::old_object(
        "perf_hints: [{pattern: run, message: reminder}]",
        "unknown variant `pattern`"
    )]
    #[case::removed_extras("extra_perf_hints: []", "unknown field: extra_perf_hints")]
    #[case::retired_include("include: [{rules: [PERF001]}]", "unknown rule `PERF001`")]
    #[case::retired_exclude("exclude: [{rules: [PERF002]}]", "unknown rule `PERF002`")]
    fn config_should_reject_obsolete_or_unknown_performance_controls(
        #[case] yaml: &str,
        #[case] expected: &str,
    ) {
        let error = load_error(yaml);

        assert!(error.contains(expected), "{error}");
    }
}
