//! The single authority deciding which pipeline ops may run for a
//! source-file extension.
//!
//! It also decides which line-comment prefixes the fix passes strip
//! within parser-verified comment runs. TEXT scanners own their lexicons
//! independently of these transformation prefixes.
//!
//! Every allowed extension is governed by a [`Profile`]:
//!
//! - `ops`: ops the extension may ever run
//! - `prefixes`: line-comment markers, longest first
//! - `default_ops`: ops that run when no include list narrows the run
//! - `backend`: whether an AST parser is registered for the extension
//! - `text_lints`: how the TEXT* text checks are sourced
//!
//! # Tiers
//!
//! - Markdown family (`md`, `markdown`, `txt`, `text`, `mdx`): the text ops
//!   `tables`, `fences`, `links` plus text-based `lints`
//! - Rust (`rs`): every op - `tables`, `fences`, `links`, `reorder`, `vis`,
//!   `lints` - with the `///`/`//!` doc prefixes
//! - C# (`cs`): `tables`, `fences` plus the AST ops `reorder`/`lints`; no
//!   `links`
//! - Python (`py`, `pyi`): `tables`, `fences`, and `lints` by default,
//!   with `#` prefixes. Its text checks use the backend's docstring and
//!   comment regions.
//! - Backendless code and configuration languages: comment `lints` only;
//!   transformations require parser-verified comment runs
//! - Unmapped extensions: no ops, even when explicitly selected
//! - Data formats (`ini`, `json`): no ops; never in
//!   [`DEFAULT_EXTENSIONS`]
//!
//! `reorder` and the parser-driven `lints` checks require `backend` in
//! addition to `ops` membership, so they stay dormant for extensions
//! without a parser.
//!
//! `vis` appears only in the Rust profile.
//!
//! # Text-lint tiers
//!
//! [`Profile::text_lints`] decides how a file's TEXT* text
//! checks are sourced:
//!
//! - [`TextLints::Prose`]: the markdown family measures the whole file
//!   as prose, no parser needed.
//! - [`TextLints::Ast`]: the language's backend parses the file. Its
//!   lint composition emits the text checks (`rs` from line-comment
//!   regions, `cs` from XML doc regions, `py`/`pyi` from docstring and
//!   `#`-comment regions).
//! - [`TextLints::Lexicon`]: the language module's fail-closed comment
//!   lexicon scans the raw source. Line and block comments measure,
//!   string content and code lines never do, and ambiguous sources
//!   produce no findings.
//! - Every comment-marker code family (`//`, `#`, `--`, `;`, `%`)
//!   carries the Lexicon tier; its `lints` op runs by default.
//! - [`TextLints::None`]: no text checks; data formats and unmapped
//!   extensions never produce text findings, and no extension outside
//!   the markdown family falls through to whole-file measurement.
//!
//! # Lookup
//!
//! [`profile_for`] matches extensions ASCII case-insensitively by binary
//! search over sorted static tables.
//!
//! `.MD` resolves like `.md`, matching [`crate::input::ext_in`].
//!
//! Lookups allocate nothing and never run per line or per item - at most
//! once per file.
//!
//! # Allowed extensions and gating
//!
//! [`allowed_extensions`] builds the one extension list a run allows: the
//! config `extensions:` key when it replaces the defaults, else
//! [`DEFAULT_EXTENSIONS`].
//!
//! The config `extra_extensions:` key and the CLI `--extension` flag add
//! on top of that base.
//!
//! Explicit paths, directory walks, and git-diff selection all consume that
//! single list.
//!
//! [`Profile::op_enabled`] then gates each op per file against the profile,
//! intersecting the user's rule selection with what the profile allows.
//! [`validate_extension`] is the shared shape check every user-supplied
//! extension must pass.

use anyhow::bail;
use core::cmp::Ordering;
use std::collections::HashSet;

/// Data formats: no ops.
const DATA: Profile = Profile {
    ops: &[],
    prefixes: &[],
    default_ops: &[],
    backend: false,
    text_lints: TextLints::None,
};
/// Data formats excluded by default, sorted; they resolve to the
/// no-op [`DATA`] profile and never appear in [`DEFAULT_EXTENSIONS`].
const DATA_EXTENSIONS: &[&str] = &["ini", "json"];
/// Extensions allowed by default: every language-table extension, sorted.
///
/// Derived from [`LANG_ENTRIES`], so it stays in lockstep with the registry;
/// the op-less data formats are absent by construction.
pub(crate) const DEFAULT_EXTENSIONS: &[&str] = &{
    let mut out = [""; LANG_ENTRIES.len()];
    let mut i = 0;
    while i < LANG_ENTRIES.len() {
        out[i] = LANG_ENTRIES[i].0;
        i += 1;
    }
    out
};
/// Extensions outside the language tables: no ops, even when selected.
const UNMAPPED: Profile = Profile {
    ops: &[],
    prefixes: &[],
    default_ops: &[],
    backend: false,
    text_lints: TextLints::None,
};
/// Extension-to-profile table, sorted by extension (ASCII) so binary search
/// applies. The sortedness test guards this invariant.
const LANG_ENTRIES: &[(&str, Profile)] = &[
    ("ada", COMMENT_LINTS),
    ("applescript", COMMENT_LINTS),
    ("bash", COMMENT_LINTS),
    ("bst", COMMENT_LINTS),
    ("bzl", COMMENT_LINTS),
    ("c", COMMENT_LINTS),
    ("cc", COMMENT_LINTS),
    ("clj", COMMENT_LINTS),
    ("cljc", COMMENT_LINTS),
    ("cls", COMMENT_LINTS),
    ("cmake", COMMENT_LINTS),
    ("conf", COMMENT_LINTS),
    ("cpp", COMMENT_LINTS),
    ("cs", C_SHARP),
    ("dart", COMMENT_LINTS),
    ("el", COMMENT_LINTS),
    ("elm", COMMENT_LINTS),
    ("erl", COMMENT_LINTS),
    ("fish", COMMENT_LINTS),
    ("go", COMMENT_LINTS),
    ("gql", COMMENT_LINTS),
    ("gradle", COMMENT_LINTS),
    ("graphql", COMMENT_LINTS),
    ("groovy", COMMENT_LINTS),
    ("h", COMMENT_LINTS),
    ("hpp", COMMENT_LINTS),
    ("hs", COMMENT_LINTS),
    ("java", COMMENT_LINTS),
    ("jl", COMMENT_LINTS),
    ("js", COMMENT_LINTS),
    ("json5", COMMENT_LINTS),
    ("jsonc", COMMENT_LINTS),
    ("ksh", COMMENT_LINTS),
    ("kt", COMMENT_LINTS),
    ("less", COMMENT_LINTS),
    ("lisp", COMMENT_LINTS),
    ("ltx", COMMENT_LINTS),
    ("lua", COMMENT_LINTS),
    ("m", COMMENT_LINTS),
    ("markdown", MARKDOWN),
    ("md", MARKDOWN),
    ("mdx", MARKDOWN),
    ("mjs", COMMENT_LINTS),
    ("nim", COMMENT_LINTS),
    ("php", COMMENT_LINTS),
    ("pl", COMMENT_LINTS),
    ("proto", COMMENT_LINTS),
    ("ps1", COMMENT_LINTS),
    ("psd1", COMMENT_LINTS),
    ("psm1", COMMENT_LINTS),
    ("purs", COMMENT_LINTS),
    ("py", PYTHON),
    ("pyi", PYTHON),
    ("r", COMMENT_LINTS),
    ("rb", COMMENT_LINTS),
    ("rs", RUST),
    ("scala", COMMENT_LINTS),
    ("scm", COMMENT_LINTS),
    ("scss", COMMENT_LINTS),
    ("sh", COMMENT_LINTS),
    ("sol", COMMENT_LINTS),
    ("sql", COMMENT_LINTS),
    ("sty", COMMENT_LINTS),
    ("sv", COMMENT_LINTS),
    ("swift", COMMENT_LINTS),
    ("tex", COMMENT_LINTS),
    ("text", MARKDOWN),
    ("thrift", COMMENT_LINTS),
    ("toml", COMMENT_LINTS),
    ("ts", COMMENT_LINTS),
    ("tsx", COMMENT_LINTS),
    ("txt", MARKDOWN),
    ("v", COMMENT_LINTS),
    ("vhd", COMMENT_LINTS),
    ("yaml", COMMENT_LINTS),
    ("yml", COMMENT_LINTS),
    ("zig", COMMENT_LINTS),
    ("zsh", COMMENT_LINTS),
];
/// Backendless languages: lint comments using the TEXT scanner's own lexicon.
const COMMENT_LINTS: Profile = Profile {
    ops: &["lints"],
    prefixes: &[],
    default_ops: &["lints"],
    backend: false,
    text_lints: TextLints::Lexicon,
};
/// C#: tables plus the backend-gated AST ops; no links - appended
/// `[text]: url` definitions are invalid C#.
const C_SHARP: Profile = Profile {
    ops: &["tables", "fences", "reorder", "lints"],
    prefixes: &["///", "//"],
    default_ops: &["tables", "fences", "reorder", "lints"],
    backend: true,
    text_lints: TextLints::Ast,
};
/// Markdown family: every text op plus text-based lints; no comment
/// prefixes, so tables, fences, and links treat the file as plain markdown.
const MARKDOWN: Profile = Profile {
    ops: &["tables", "fences", "links", "lints"],
    prefixes: &[],
    default_ops: &["tables", "fences", "links", "lints"],
    backend: false,
    text_lints: TextLints::Prose,
};
/// Python: `#` comments for the fix passes plus the tree-sitter-python
/// backend's docstring-dialect text checks (docstrings and `#`
/// comments); no AST ops.
const PYTHON: Profile = Profile {
    ops: &["tables", "fences", "lints"],
    prefixes: &["#"],
    default_ops: &["tables", "fences", "lints"],
    backend: true,
    text_lints: TextLints::Ast,
};
/// Rust: every op, with the `///`/`//!` doc markers longest first.
const RUST: Profile = Profile {
    ops: &["tables", "fences", "links", "reorder", "vis", "lints"],
    prefixes: &["///", "//!"],
    default_ops: &["tables", "fences", "links", "reorder", "vis", "lints"],
    backend: true,
    text_lints: TextLints::Ast,
};

/// One extension's profile data: the ops it may run and the comment
/// prefixes its fix passes use.
///
/// All members are static: profiles are compile-time table rows, never built
/// at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Profile {
    /// Ops this extension may ever run, as rule names accepted by
    /// `--include`/`--exclude`, in [`crate::config::KNOWN_FIX_OPS`] order.
    pub ops: &'static [&'static str],
    /// Line-comment markers stripped and re-applied around tables and
    /// fences, longest first.
    ///
    /// A `///` marker must precede `//`; the list is empty for prose and
    /// profiles without transformations. TEXT scanners do not use this list.
    pub prefixes: &'static [&'static str],
    /// Ops that run when no explicit include list narrows the run; always a
    /// subset of `ops`.
    pub default_ops: &'static [&'static str],
    /// Whether an AST parser is registered for the extension.
    ///
    /// `reorder` and the parser-driven `lints` checks require this in
    /// addition to appearing in `ops`, as does the [`TextLints::Ast`]
    /// tier's doc-region producer.
    pub backend: bool,
    /// How the TEXT* text checks are sourced for this profile.
    pub text_lints: TextLints,
}

/// How a profile's TEXT* text checks are sourced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextLints {
    /// Whole-file prose measurement over the raw source: the markdown
    /// family's producer, no parser needed.
    Prose,
    /// Text regions from the language's AST backend: the backend's parse
    /// feeds its lint composition, which emits the text checks.
    Ast,
    /// Text regions from the language module's fail-closed comment lexicon:
    /// a linear scan over line and block comments.
    ///
    /// String content and code lines never measure, and ambiguous sources
    /// produce no findings.
    Lexicon,
    /// No text checks: data formats and unmapped extensions produce no
    /// text findings.
    None,
}

impl Profile {
    /// Whether `op` is in the profile's `ops` list.
    #[inline]
    pub(crate) fn allows(&self, op: &str) -> bool {
        self.ops.contains(&op)
    }

    /// Whether `op` runs for a file with this profile under the active rule
    /// selection.
    ///
    /// `enabled` whitelist / `disabled` blacklist, exactly as
    /// [`crate::pipeline`] resolves them.
    ///
    /// Whitelist mode intersects the whitelist with the profile's `ops`;
    /// default mode runs the profile's `default_ops` minus the disabled
    /// names. Either way an op the profile never allows stays refused -
    /// `links` outside the markdown family and Rust.
    ///
    /// The AST ops (`reorder`, `vis`, parser-driven `lints`) also
    /// require [`Profile::backend`]; that gate applies where they dispatch.
    pub(crate) fn op_enabled(
        &self,
        op: &str,
        enabled: &Option<HashSet<String>>,
        disabled: &HashSet<String>,
    ) -> bool {
        match enabled {
            Some(set) => set.contains(op) && self.allows(op),
            None => self.default_ops.contains(&op) && !disabled.contains(op),
        }
    }
}

/// The extensions one run allows.
///
/// The base is the config `extensions:` key when non-empty (replacing the
/// registry defaults wholesale), else [`DEFAULT_EXTENSIONS`]. The config
/// `extra_extensions:` key and the CLI `--extension` flag add on top.
///
/// Built once per run; explicit paths, directory walks, and git-diff
/// selection all consume this single list, so the allowed set is identical
/// across input modes.
///
/// Appended entries may repeat the base or each other - membership checks
/// are idempotent. Per-file op gating always re-resolves the profile
/// from the file's own extension.
pub(crate) fn allowed_extensions<'a>(
    config: Option<&'a crate::config::CompiledConfig>,
    extensions: &'a [String],
) -> Vec<&'a str> {
    // A non-empty `extensions:` list replaces the defaults wholesale.
    let mut exts: Vec<&str> = match config.filter(|c| !c.extension_override().is_empty()) {
        Some(config) => config
            .extension_override()
            .iter()
            .map(String::as_str)
            .collect(),
        None => DEFAULT_EXTENSIONS.to_vec(),
    };
    if let Some(config) = config {
        exts.extend(config.extra_extensions().iter().map(String::as_str));
    }
    exts.extend(extensions.iter().map(String::as_str));
    exts
}

/// The profile governing `ext`, ASCII case-insensitively (`.MD` resolves like
/// `.md`).
///
/// Extensions outside the language table resolve to the no-op
/// [`UNMAPPED`] profile, except the data formats, which resolve to the
/// no-op [`DATA`] profile.
///
/// # Arguments
///
/// - `ext`: a path extension without the leading dot; an empty string
///   resolves to [`UNMAPPED`].
#[inline]
pub(crate) fn profile_for(ext: &str) -> &'static Profile {
    if let Ok(i) = LANG_ENTRIES.binary_search_by(|probe| cmp_ext(probe.0, ext)) {
        return &LANG_ENTRIES[i].1;
    }
    if DATA_EXTENSIONS
        .binary_search_by(|probe| cmp_ext(probe, ext))
        .is_ok()
    {
        return &DATA;
    }
    &UNMAPPED
}

/// Validate one user-supplied extension from the config `extensions:` or
/// `extra_extensions:` key or the CLI `--extension` flag.
///
/// # Arguments
///
/// - `ext`: the extension exactly as the user wrote it, without a leading
///   dot.
///
/// # Errors
///
/// Returns an error when `ext` is empty, starts with a dot, or contains
/// an inner dot, a path separator (`/` or `\`), or whitespace.
///
/// None of those can match a real path extension, so they fail the run
/// instead of being silently ignored.
pub(crate) fn validate_extension(ext: &str) -> anyhow::Result<()> {
    let shape_ok = !ext.is_empty()
        && !ext.starts_with('.')
        && !ext.contains(['.', '/', '\\'])
        && !ext.chars().any(char::is_whitespace);
    if !shape_ok {
        bail!(
            "invalid extension `{ext}`: write it without a leading dot and with no inner \
             dot, path separator, or whitespace"
        );
    }
    Ok(())
}

/// ASCII case-insensitive ordering, matching [`crate::input::ext_in`]
/// comparisons.
#[inline]
fn cmp_ext(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::collections::BTreeSet;

    /// Build a rule set from names, for whitelist/blacklist gating cases.
    fn rules(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    /// The approved matrix's markdown-family extensions.
    const MD_FAMILY: &[&str] = &["md", "markdown", "txt", "text", "mdx"];

    /// The approved matrix's code-language extensions, grouped by comment
    /// marker family.
    const CODE_FAMILIES: &[&[&str]] = &[
        &[
            "c", "h", "cpp", "cc", "hpp", "java", "js", "mjs", "ts", "tsx", "go", "swift", "kt",
            "php", "dart", "scala", "zig", "gradle", "groovy", "proto", "thrift", "sol", "scss",
            "less", "jsonc", "json5", "v", "sv",
        ],
        &[
            "rb", "sh", "bash", "zsh", "r", "pl", "jl", "nim", "conf", "toml", "bzl", "ksh",
            "yaml", "yml", "ps1", "psm1", "psd1", "graphql", "gql", "fish", "cmake",
        ],
        &[
            "lua",
            "sql",
            "hs",
            "elm",
            "ada",
            "vhd",
            "purs",
            "applescript",
        ],
        &["el", "lisp", "clj", "cljc", "scm"],
        &["tex", "erl", "m", "sty", "cls", "ltx", "bst"],
    ];

    /// Assert `ext` resolves to `profile` so failures name the extension.
    fn assert_profile(ext: &str, profile: &Profile) {
        assert_eq!(profile_for(ext), profile, "wrong tier for .{ext}");
    }

    /// Uppercase and mixed-case extensions resolve identically to their
    /// lowercase forms.
    #[test]
    fn lookup_matches_extensions_ascii_case_insensitively() {
        let cases = [
            ("MD", "md"),
            ("Rs", "rs"),
            ("CS", "cs"),
            ("PY", "py"),
            ("LUA", "lua"),
            ("JSON", "json"),
            ("ORG", "org"),
        ];

        for (upper, lower) in cases {
            assert_eq!(
                profile_for(upper),
                profile_for(lower),
                ".{upper} must resolve like .{lower}"
            );
        }
    }

    /// Data formats resolve to the no-op profile; `toml`/`yaml`/`yml`
    /// left the data tier for the lexicon tier.
    #[test]
    fn data_formats_resolve_to_the_no_op_profile() {
        for ext in DATA_EXTENSIONS {
            assert_profile(ext, &DATA);
        }
        for ext in ["toml", "yaml", "yml"] {
            assert_eq!(profile_for(ext).text_lints, TextLints::Lexicon, ".{ext}");
        }
    }

    /// The registry lists exactly the approved matrix: no missing, extra, or
    /// duplicated extensions.
    #[test]
    fn registry_lists_exactly_the_approved_matrix() {
        let mut expected: BTreeSet<&str> = MD_FAMILY.iter().copied().collect();
        expected.extend(["rs", "cs", "py", "pyi"]);
        for exts in CODE_FAMILIES {
            expected.extend(exts.iter().copied());
        }

        let actual: BTreeSet<&str> = LANG_ENTRIES.iter().map(|(ext, _)| *ext).collect();

        assert_eq!(actual, expected);
        assert_eq!(
            LANG_ENTRIES.len(),
            actual.len(),
            "duplicate table keys would collapse in the set"
        );
    }

    /// Binary search requires the sortedness of both static tables.
    #[test]
    fn registry_tables_stay_sorted_for_binary_search() {
        for pair in LANG_ENTRIES.windows(2) {
            assert!(
                cmp_ext(pair[0].0, pair[1].0) == Ordering::Less,
                "`{}` must sort before `{}`",
                pair[0].0,
                pair[1].0
            );
        }

        for pair in DATA_EXTENSIONS.windows(2) {
            assert!(
                cmp_ext(pair[0], pair[1]) == Ordering::Less,
                "`{}` must sort before `{}`",
                pair[0],
                pair[1]
            );
        }
    }

    /// Every profile's ops are known rule names, and its defaults stay a
    /// subset of its allowed ops.
    #[test]
    fn ops_are_known_rules_with_defaults_a_subset() {
        let mut all = vec![&UNMAPPED, &DATA];
        all.extend(LANG_ENTRIES.iter().map(|(_, profile)| profile));

        for profile in all {
            for op in profile.ops {
                assert!(
                    crate::config::KNOWN_FIX_OPS.contains(op),
                    "`{op}` is not a known rule name"
                );
            }
            for op in profile.default_ops {
                assert!(
                    profile.ops.contains(op),
                    "default op `{op}` must also be allowed"
                );
            }
        }
    }

    /// A comment marker that extends another marker must precede it, so the
    /// longest match wins during prefix stripping.
    #[test]
    fn comment_markers_order_longest_first() {
        let mut all = vec![&UNMAPPED, &DATA];
        all.extend(LANG_ENTRIES.iter().map(|(_, profile)| profile));

        for profile in all {
            for (long, short) in profile
                .prefixes
                .iter()
                .flat_map(|long| profile.prefixes.iter().map(move |short| (long, short)))
                .filter(|(long, short)| long != short && long.starts_with(*short))
            {
                let long_idx = profile.prefixes.iter().position(|p| p == long).unwrap();
                let short_idx = profile.prefixes.iter().position(|p| p == short).unwrap();
                assert!(
                    long_idx < short_idx,
                    "`{long}` must precede `{short}` in {:?}",
                    profile.prefixes
                );
            }
        }
    }

    // ── Per-file op gating ──

    /// Explicit selection cannot enable operations outside a profile's capabilities.
    #[rstest]
    #[case::rust("rs", &["tables", "fences", "links", "reorder", "vis", "lints"])]
    #[case::csharp("cs", &["tables", "fences", "reorder", "lints"])]
    #[case::python("py", &["tables", "fences", "lints"])]
    #[case::python_stub("pyi", &["tables", "fences", "lints"])]
    #[case::markdown("md", &["tables", "fences", "links", "lints"])]
    #[case::slash_comments("js", &["lints"])]
    #[case::hash_comments("rb", &["lints"])]
    #[case::dash_comments("sql", &["lints"])]
    #[case::semicolon_comments("el", &["lints"])]
    #[case::percent_comments("tex", &["lints"])]
    #[case::toml("toml", &["lints"])]
    #[case::yaml("yaml", &["lints"])]
    #[case::unknown("org", &[])]
    #[case::empty("", &[])]
    #[case::data("json", &[])]
    fn operations_should_respect_profile_capabilities(
        #[case] ext: &str,
        #[values(false, true)] explicit: bool,
        #[case] expected: &[&str],
    ) {
        let profile = profile_for(ext);
        let enabled = explicit.then(|| rules(crate::config::KNOWN_FIX_OPS));
        let disabled = rules(&[]);

        let actual: Vec<_> = crate::config::KNOWN_FIX_OPS
            .iter()
            .copied()
            .filter(|op| profile.op_enabled(op, &enabled, &disabled))
            .collect();

        assert_eq!(actual, expected);
    }

    /// The `backend` column must agree with the lang-crate backend registry
    /// per extension and per AST op.
    ///
    /// Dispatch composes both tables, and a language updated on only one
    /// side silently gains or loses AST ops.
    ///
    /// The Ast text tier also requires the column: its doc-region
    /// producer dispatches through the backend.
    ///
    /// `lints` allows both the parser-driven codes and the text checks, so
    /// a backend implementing parser-driven codes implies `lints` is
    /// allowed: never dead backend work.
    ///
    /// `lints` without parser-driven codes is legitimate exactly for the
    /// doc-regions-only backends (`py`/`pyi`), whose `lints` op is the
    /// text tier alone.
    #[test]
    fn backend_column_matches_the_backend_registry() {
        for (ext, profile) in LANG_ENTRIES {
            let backend = crate::languages::backend_for(ext);
            assert_eq!(
                profile.backend,
                backend.is_some(),
                ".{ext}: profile column and backend registry disagree"
            );
            if profile.text_lints == TextLints::Ast {
                assert!(
                    profile.backend,
                    ".{ext}: the Ast text tier needs the backend column"
                );
            }
            if let Some(backend) = backend {
                for op in ["reorder", "vis"] {
                    assert_eq!(
                        profile.allows(op),
                        backend.ast_ops().contains(&op),
                        ".{ext}: {op} availability disagrees"
                    );
                }
                assert!(
                    !backend.ast_ops().contains(&"lints") || profile.allows("lints"),
                    ".{ext}: the backend's parser-driven lints must be allowed"
                );
            }
        }
    }

    /// Op gating intersects the rule selection with the profile.
    ///
    /// Default mode runs the profile defaults minus disabled names;
    /// whitelist mode intersects the whitelist with the allowed ops.
    #[test]
    fn op_enabled_combines_rule_selection_with_profile_ops() {
        let none = None;
        let empty = rules(&[]);

        // Default mode: Python runs its text ops; the markdown
        // family and Rust run every fix op.
        assert!(profile_for("py").op_enabled("tables", &none, &empty));
        assert!(profile_for("py").op_enabled("lints", &none, &empty));
        assert!(profile_for("py").op_enabled("fences", &none, &empty));
        for ext in MD_FAMILY.iter().chain(["rs"].iter()) {
            for op in ["tables", "fences", "links"] {
                assert!(
                    profile_for(ext).op_enabled(op, &none, &empty),
                    ".{ext} must run {op} by default"
                );
            }
        }

        // Default mode minus a disabled op, and data formats allow nothing.
        assert!(!profile_for("md").op_enabled("tables", &none, &rules(&["tables"])));
        assert!(profile_for("md").op_enabled("fences", &none, &rules(&["tables"])));
        assert!(!profile_for("json").op_enabled("tables", &none, &empty));

        // Whitelist mode: an explicit include reaches a code language's
        // fences and every code family's text checks.
        //
        // Links outside the markdown family and Rust stay refused, and
        // so does an op the profile never carries.
        //
        // The loop's second assertion is default mode: every code
        // family's `lints` runs with no include list.
        assert!(profile_for("py").op_enabled("fences", &Some(rules(&["fences"])), &empty));
        assert!(!profile_for("py").op_enabled("links", &Some(rules(&["links"])), &empty));
        assert!(!profile_for("md").op_enabled("reorder", &Some(rules(&["reorder"])), &empty));
        for ext in ["js", "rb", "py", "sql", "el", "tex"] {
            assert!(
                profile_for(ext).op_enabled("lints", &Some(rules(&["lints"])), &empty),
                ".{ext}: an explicit include must reach the text checks"
            );
            assert!(
                profile_for(ext).op_enabled("lints", &none, &empty),
                ".{ext}: lints must run in the default run"
            );
        }
    }

    /// Every new mapping measures comments by default.
    #[rstest]
    #[case::applescript("applescript")]
    #[case::bst("bst")]
    #[case::bzl("bzl")]
    #[case::cls("cls")]
    #[case::cmake("cmake")]
    #[case::fish("fish")]
    #[case::gql("gql")]
    #[case::gradle("gradle")]
    #[case::graphql("graphql")]
    #[case::groovy("groovy")]
    #[case::json5("json5")]
    #[case::jsonc("jsonc")]
    #[case::ksh("ksh")]
    #[case::less("less")]
    #[case::ltx("ltx")]
    #[case::proto("proto")]
    #[case::ps1("ps1")]
    #[case::psd1("psd1")]
    #[case::psm1("psm1")]
    #[case::purs("purs")]
    #[case::scss("scss")]
    #[case::sol("sol")]
    #[case::sty("sty")]
    #[case::sv("sv")]
    #[case::thrift("thrift")]
    #[case::toml("toml")]
    #[case::v("v")]
    #[case::vhd("vhd")]
    #[case::yaml("yaml")]
    #[case::yml("yml")]
    fn lints_should_run_by_default_for_new_mappings(#[case] ext: &str) {
        let none = None;
        let empty = rules(&[]);

        assert!(profile_for(ext).op_enabled("lints", &none, &empty));
    }

    /// Every registry extension resolves to exactly one text-lint tier.
    ///
    /// Markdown prose covers the markdown family only. `rs`/`cs`/`py`/`pyi`
    /// use AST regions, and every comment-marker code family uses the
    /// lexicon tier.
    ///
    /// No registry extension is left without a producer, and nothing
    /// outside the markdown family falls through to whole-file
    /// measurement.
    #[test]
    fn text_lint_tiers_cover_every_extension_exactly_once() {
        for ext in MD_FAMILY {
            assert_eq!(
                profile_for(ext).text_lints,
                TextLints::Prose,
                ".{ext} must measure whole-file prose"
            );
        }
        for ext in ["rs", "cs", "py", "pyi"] {
            assert_eq!(
                profile_for(ext).text_lints,
                TextLints::Ast,
                ".{ext} must source text regions from its backend"
            );
        }
        for exts in CODE_FAMILIES {
            for ext in *exts {
                assert_eq!(
                    profile_for(ext).text_lints,
                    TextLints::Lexicon,
                    ".{ext} must scan comments with the lexicon"
                );
                assert_eq!(profile_for(ext).ops, &["lints"], ".{ext}");
                assert_eq!(profile_for(ext).default_ops, &["lints"], ".{ext}");
            }
        }
        for (ext, profile) in LANG_ENTRIES {
            match profile.text_lints {
                TextLints::Prose => assert!(
                    MD_FAMILY.contains(ext),
                    ".{ext} is outside the markdown family and must not be Prose"
                ),
                TextLints::Ast => assert!(
                    *ext == "rs" || *ext == "cs" || *ext == "py" || *ext == "pyi",
                    ".{ext} has no registered doc-region producer"
                ),
                TextLints::Lexicon => assert!(
                    CODE_FAMILIES.iter().any(|exts| exts.contains(ext)),
                    ".{ext} is outside the lexicon families"
                ),
                // All 78 registry extensions carry a producer tier; the
                // None tier belongs to data formats and unmapped
                // extensions only, both outside the registry.
                TextLints::None => panic!(
                    ".{ext} resolves to no producer tier; every registry \
                     extension must carry exactly one"
                ),
            }
        }
        for ext in ["org", "", "json"] {
            assert_eq!(
                profile_for(ext).text_lints,
                TextLints::None,
                ".{ext} must run no text checks"
            );
        }
    }

    /// The Lexicon tier and the language module's lexicon table agree per
    /// extension.
    ///
    /// A tier without a lexicon entry would silently emit nothing, and a
    /// lexicon entry without the tier would never run.
    #[test]
    fn lexicon_tiers_match_the_lang_crate_lexicon() {
        for (ext, profile) in LANG_ENTRIES {
            assert_eq!(
                profile.text_lints == TextLints::Lexicon,
                crate::text::comments::covers(ext),
                ".{ext}: tier and lexicon coverage disagree"
            );
        }
        for ext in ["org", "", "json"] {
            assert!(
                !crate::text::comments::covers(ext),
                ".{ext} must have no lexicon entry"
            );
        }
    }

    // ── Run-level allowed extensions ──

    /// Write `yaml` as a config and return the allowed list for it plus the
    /// given CLI `--extension` values.
    fn allowed_for(yaml: &str, cli_exts: &[&str]) -> Vec<String> {
        static SEQ: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rlt-langs-allow-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg_path = dir.join(".rust-llm-tidy.yml");
        std::fs::write(&cfg_path, yaml).unwrap();
        let config = crate::config::load_and_compile(&cfg_path).unwrap();
        let extensions: Vec<String> = cli_exts.iter().map(|ext| (*ext).to_string()).collect();
        let allowed = allowed_extensions(Some(&config), &extensions);
        let _ = std::fs::remove_dir_all(&dir);
        allowed.into_iter().map(String::from).collect()
    }

    /// A non-empty `extensions:` list replaces the defaults wholesale.
    #[test]
    fn extensions_key_replaces_default_extensions() {
        let allowed = allowed_for("extensions: [\"log\"]\n", &[]);

        assert!(allowed.contains(&"log".to_string()), "log must be allowed");
        for ext in DEFAULT_EXTENSIONS {
            assert!(
                !allowed.contains(&ext.to_string()),
                ".{ext} must be dropped by the replacement"
            );
        }
    }

    /// An empty or absent `extensions:` list keeps the defaults, and
    /// `extra_extensions:` plus `--extension` add on top of them.
    #[test]
    fn extra_extensions_add_to_default_extensions() {
        let allowed = allowed_for(
            "extensions: []\nextra_extensions: [\"log\"]\n",
            &["org", "MD"],
        );

        for ext in DEFAULT_EXTENSIONS {
            assert!(allowed.contains(&ext.to_string()), ".{ext} missing");
        }
        for ext in ["log", "org", "MD"] {
            assert!(allowed.contains(&ext.to_string()), "addition {ext} missing");
        }
    }

    /// `extra_extensions:` and `--extension` add on top of a replaced base.
    #[test]
    fn extra_extensions_add_on_top_of_replaced_base() {
        let allowed = allowed_for(
            "extensions: [\"rs\"]\nextra_extensions: [\"log\"]\n",
            &["org"],
        );

        let expected = ["rs", "log", "org"];
        assert_eq!(allowed, expected.to_vec());
    }

    /// Extension validation accepts well-formed values and rejects the
    /// shapes that can never match a real path extension.
    #[test]
    fn validate_extension_rejects_unmatchable_shapes() {
        for ext in ["rs", "MD", "c++"] {
            assert!(validate_extension(ext).is_ok(), "`{ext}` should be valid");
        }
        for ext in ["", ".rs", "a.md", "src/rs", "a\\b", "a b"] {
            assert!(
                validate_extension(ext).is_err(),
                "`{ext}` should be rejected"
            );
        }
    }
}
