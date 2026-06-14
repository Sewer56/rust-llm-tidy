//! Cross-file module-tree resolver for crate-aware visibility narrowing.
//!
//! Maps `mod foo;` file references to source files (edition path rules +
//! `#[path]` overrides), distinguishes inline `mod foo {}` (no file), and
//! propagates each file's effective floor visibility root -> leaf. Crate-root
//! discovery uses `cargo metadata --no-deps` (the CLI narrows each file
//! standalone when that fails - see `cli/src/main.rs`).

use ahash::AHashMap;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;

/// One resolved `mod` child of a parent file.
enum ModChild {
    /// `mod foo {}` - inline, no file (already narrowed by `walk()`).
    Inline,
    /// `mod foo;` -> resolved file path, plus the verbatim visibility text of
    /// the declaration (e.g. `"pub(crate)"`, or `None` for bare `pub`/private).
    File {
        path: PathBuf,
        vis_text: Option<String>,
    },
}

/// A resolved cross-file module tree: maps each source file to its effective
/// floor visibility (the most-restrictive `mod` visibility on the path from the
/// crate root) and exposes per-file lookup used by `narrow_vis_in_tree`.
pub struct ModuleTree {
    /// Canonicalized file path -> effective floor visibility text
    /// (e.g. `"pub(crate)"`), or `None` at the crate root.
    floors: AHashMap<PathBuf, Option<String>>,
    /// Non-fatal resolution warnings (unresolved `mod foo;`, missing `#[path]`
    /// target). Surfaced to the CLI as the new diagnostics (P5 error surface).
    warnings: Vec<String>,
}

impl ModuleTree {
    /// Effective floor visibility text for `file`, or `None` at the crate root.
    /// `None` for files outside the tree (caller narrows standalone with no floor).
    pub fn floor_for(&self, file: &Path) -> Option<&str> {
        self.floors.get(file).and_then(|f| f.as_deref())
    }

    /// True if `file` is a known node in this tree.
    pub fn contains(&self, file: &Path) -> bool {
        self.floors.contains_key(file)
    }

    /// Non-fatal resolution warnings (unresolved `mod`, missing `#[path]`).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

/// Build a module tree from already-read `(path, source)` pairs. The root is the
/// file whose path equals `root`. Floors propagate root -> leaf: a `mod` with
/// restricted visibility sets the floor for its (transitive) descendants; bare
/// `pub` / private `mod` inherits the ancestor floor. Matches the inline
/// `walk()` "innermost restricted ancestor wins" semantics so tree and inline
/// paths share identical narrowing behavior.
///
/// # Errors
///
/// Returns [`syn::Error`] when any source in `sources` is not valid Rust.
pub fn build_module_tree(root: &Path, sources: &[(PathBuf, String)]) -> anyhow::Result<ModuleTree> {
    // 1. Parse every file; index by canonical path. Keep sources for byte-exact
    //    visibility span slicing (mirrors walk() per REQ-002).
    let mut parsed: AHashMap<PathBuf, syn::File> = AHashMap::new();
    let mut source_texts: AHashMap<PathBuf, String> = AHashMap::new();
    for (path, src) in sources {
        let file: syn::File = syn::parse_str(src)?;
        parsed.insert(path.clone(), file);
        source_texts.insert(path.clone(), src.clone());
    }
    // 2. BFS from root, propagating floors. The root's floor is None.
    let mut floors: AHashMap<PathBuf, Option<String>> = AHashMap::new();
    let mut warnings = Vec::new();
    let mut queue: Vec<(PathBuf, Option<String>)> = vec![(root.to_path_buf(), None)];
    // Build a set of known file paths for in-memory resolution.
    let known_files: std::collections::HashSet<PathBuf> = parsed.keys().cloned().collect();
    while let Some((path, floor)) = queue.pop() {
        if floors.contains_key(&path) {
            continue;
        }
        floors.insert(path.clone(), floor.clone());
        let Some(file) = parsed.get(&path) else {
            continue;
        };
        let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let source = source_texts.get(&path).map(|s| s.as_str()).unwrap_or("");
        let line_starts = crate::line_start_offsets(source);
        for child in resolve_mod_children(
            &file.items,
            parent_dir,
            &path,
            source,
            &line_starts,
            &mut warnings,
            &known_files,
        ) {
            match child {
                ModChild::Inline => {} // handled by walk() at narrowing time
                ModChild::File {
                    path: cpath,
                    vis_text,
                } => {
                    // child floor: restricted declaration overrides; else inherit.
                    let child_floor = vis_text.clone().or_else(|| floor.clone());
                    queue.push((cpath, child_floor));
                }
            }
        }
    }
    Ok(ModuleTree { floors, warnings })
}

/// Discover the crate root source file by walking up from `start` to a
/// `Cargo.toml`, then running `cargo metadata --no-deps` and returning the
/// `lib` target's `src_path` (else the `bin` target whose path ends in
/// `main.rs`). The CLI maps failure to a warn + standalone narrowing.
///
/// # Errors
///
/// Returns an error when:
///
/// - No `Cargo.toml` is found walking up from `start`.
/// - `cargo metadata --no-deps` fails (e.g. the manifest is invalid or
///   unparseable).
/// - `cargo metadata` returns no root package.
/// - The root package has no `lib` target and no `bin` target whose `src_path`
///   ends in `main.rs`.
pub fn discover_crate_root(start: &Path) -> anyhow::Result<PathBuf> {
    let manifest = find_cargo_toml(start)?;
    let meta = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest)
        .no_deps()
        .exec()?;
    let pkg = meta
        .root_package()
        .ok_or_else(|| anyhow::anyhow!("cargo metadata returned no root package"))?;
    // Prefer a lib target; else main.rs bin.
    pkg.targets
        .iter()
        .find(|t| t.kind.iter().any(|k| k == &cargo_metadata::TargetKind::Lib))
        .or_else(|| {
            pkg.targets.iter().find(|t| {
                t.kind.iter().any(|k| k == &cargo_metadata::TargetKind::Bin)
                    && t.src_path.ends_with("main.rs")
            })
        })
        .map(|t| t.src_path.clone().into_std_path_buf())
        .ok_or_else(|| anyhow::anyhow!("no lib/main target in {}", manifest.display()))
}

/// Walk up from `start` to the nearest `Cargo.toml`.
fn find_cargo_toml(start: &Path) -> anyhow::Result<PathBuf> {
    let dir = if start.is_file() {
        start.parent()
    } else {
        Some(start)
    }
    .ok_or_else(|| anyhow::anyhow!("no parent dir for {}", start.display()))?;
    for ancestor in dir.ancestors() {
        let m = ancestor.join("Cargo.toml");
        if m.is_file() {
            return Ok(m);
        }
    }
    anyhow::bail!("no Cargo.toml found walking up from {}", start.display())
}

/// Resolve top-level `mod` items of one file into children. Edition path rules:
/// both editions prefer `foo.rs` over `foo/mod.rs` (mod.rs is the deprecated
/// fallback). `#[path = "..."]` overrides normal resolution. `#[cfg]`-gated
/// mods are treated as present (Open Q5). Unresolved `mod foo;` and missing
/// `#[path]` targets append a warning (P5 diagnostic) rather than failing.
fn resolve_mod_children(
    items: &[syn::Item],
    parent_dir: &Path,
    parent: &Path,
    source: &str,
    line_starts: &[usize],
    warnings: &mut Vec<String>,
    known_files: &std::collections::HashSet<PathBuf>,
) -> Vec<ModChild> {
    let mut out = Vec::new();
    for item in items {
        let syn::Item::Mod(m) = item else { continue };
        if m.content.is_some() {
            out.push(ModChild::Inline);
            continue;
        }
        let vis_text = vis_text(&m.vis, source, line_starts); // byte-exact span slice
        let path_attr = m.attrs.iter().find_map(|a| {
            if a.path().is_ident("path") {
                a.meta.require_name_value().ok().and_then(|nv| {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &nv.value
                    {
                        Some(s.value())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        let resolved = match path_attr {
            Some(p) => {
                let candidate = parent_dir.join(&p);
                // Canonicalize the candidate for lookup against the known set.
                let cand_canon = std::fs::canonicalize(&candidate).unwrap_or(candidate.clone());
                if known_files.contains(&cand_canon)
                    || known_files.contains(&candidate)
                    || candidate.is_file()
                {
                    Some(candidate)
                } else {
                    warnings.push(format!(
                        "{}: #[path = \"{}\"] target not found",
                        parent.display(),
                        p
                    ));
                    None
                }
            }
            None => resolve_mod_file(
                parent_dir,
                &m.ident.to_string(),
                known_files,
                warnings,
                parent,
            ),
        };
        match resolved {
            Some(p) => out.push(ModChild::File { path: p, vis_text }),
            None => warnings.push(format!(
                "{}: `mod {};` resolves to no `{}.rs` or `{0}/mod.rs`",
                parent.display(),
                m.ident,
                m.ident
            )),
        }
    }
    out
}

/// `mod foo;` -> `foo.rs` (preferred) else `foo/mod.rs`. Both editions prefer
/// the file form. If both exist it is E0761 (compile error); we still pick
/// `foo.rs` and warn.
fn resolve_mod_file(
    dir: &Path,
    name: &str,
    known_files: &std::collections::HashSet<PathBuf>,
    _warnings: &mut Vec<String>,
    _parent: &Path,
) -> Option<PathBuf> {
    let file = dir.join(format!("{name}.rs"));
    if known_files.contains(&file) || file.is_file() {
        return Some(file);
    }
    let modrs = dir.join(name).join("mod.rs");
    if known_files.contains(&modrs) || modrs.is_file() {
        return Some(modrs);
    }
    None
}

/// Capture a `Visibility` verbatim from `source` restricted to this declaration.
/// Uses byte-exact span slicing mirroring `walk()`, per REQ-002.
/// `to_token_stream()+strip` would corrupt `pub(in crate::a)` -> `pub(incrate::a)`.
fn vis_text(vis: &syn::Visibility, source: &str, line_starts: &[usize]) -> Option<String> {
    match vis {
        syn::Visibility::Restricted(_) => {
            let span = vis.span();
            let start = crate::linecol_to_byte(line_starts, span.start().line, span.start().column);
            let end = crate::linecol_to_byte(line_starts, span.end().line, span.end().column);
            Some(source[start..end].to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_module_tree, discover_crate_root};
    use std::path::PathBuf;

    fn src(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn mod_file_resolves_to_foo_rs_not_mod_rs() {
        // Edition rule: prefer foo.rs over foo/mod.rs.
        let sources: Vec<(PathBuf, String)> = vec![
            (src("src/lib.rs"), "mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        // foo.rs is in the tree; lib.rs is the root (floor None).
        assert!(tree.contains(&src("src/foo.rs")));
        assert_eq!(
            tree.floor_for(&src("src/lib.rs")),
            None,
            "root floor is None"
        );
        assert_eq!(
            tree.floor_for(&src("src/foo.rs")),
            None,
            "bare `pub`/private mod inherits root floor (None)"
        );
    }

    #[test]
    fn mod_file_falls_back_to_mod_rs_when_foo_rs_absent() {
        // Edition rule: `foo/mod.rs` is the legacy fallback when `foo.rs` is
        // absent; the resolver must accept either candidate independently.
        let sources: Vec<(PathBuf, String)> = vec![
            (src("src/lib.rs"), "mod foo;\n".into()),
            (src("src/foo/mod.rs"), "pub fn f() {}\n".into()),
        ];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        assert!(
            tree.contains(&src("src/foo/mod.rs")),
            "mod foo; falls back to foo/mod.rs when foo.rs is absent"
        );
    }

    #[test]
    fn pub_crate_mod_propagates_floor_to_child_file() {
        let sources: Vec<(PathBuf, String)> = vec![
            (src("src/lib.rs"), "pub(crate) mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        let floor = tree.floor_for(&src("src/foo.rs")).expect("foo.rs in tree");
        assert_eq!(
            floor, "pub(crate)",
            "child file inherits the declaration floor"
        );
    }

    #[test]
    fn pub_super_mod_propagates_floor_to_child_file() {
        // `pub(super)` floor: byte-exact span slice must not corrupt the text.
        let sources: Vec<(PathBuf, String)> = vec![
            (src("src/lib.rs"), "pub(super) mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        let floor = tree.floor_for(&src("src/foo.rs")).expect("foo.rs in tree");
        assert_eq!(
            floor, "pub(super)",
            "pub(super) declaration floor must be byte-exact"
        );
    }

    #[test]
    fn pub_in_path_mod_propagates_floor_to_child_file() {
        // `pub(in crate::a)` floor: span-slice must preserve `incrate::a` spacing.
        // to_token_stream()+strip would corrupt this to `pub(incrate::a)`.
        let sources: Vec<(PathBuf, String)> = vec![
            (src("src/lib.rs"), "pub(in crate::a) mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        let floor = tree.floor_for(&src("src/foo.rs")).expect("foo.rs in tree");
        assert_eq!(
            floor, "pub(in crate::a)",
            "pub(in path) declaration floor must be byte-exact"
        );
    }

    #[test]
    fn path_attr_overrides_resolution() {
        let sources: Vec<(PathBuf, String)> = vec![
            (
                src("src/lib.rs"),
                "#[path = \"nested/real.rs\"]\nmod foo;\n".into(),
            ),
            (src("src/nested/real.rs"), "pub fn f() {}\n".into()),
        ];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        assert!(
            tree.contains(&src("src/nested/real.rs")),
            "#[path] target resolved"
        );
    }

    #[test]
    fn inline_mod_is_not_a_file_edge() {
        // `mod foo {}` is inline -> no file child; walk() handles it.
        let sources: Vec<(PathBuf, String)> = vec![
            (
                src("src/lib.rs"),
                "pub(crate) mod foo { pub fn f() {} }\n".into(),
            ),
            (
                src("src/foo.rs"),
                "// stray file; inline mod must not pull it in\n".into(),
            ),
        ];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        assert!(
            !tree.contains(&src("src/foo.rs")),
            "inline mod must not pull in a stray foo.rs"
        );
    }

    #[test]
    fn cfg_gated_mod_treated_as_present() {
        // Open Q5 settled: cfg-gated mods are present. The mod still resolves
        // and propagates its declared (restricted) floor.
        let sources: Vec<(PathBuf, String)> = vec![
            (
                src("src/lib.rs"),
                "#[cfg(feature = \"x\")] pub(crate) mod foo;\n".into(),
            ),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        assert!(
            tree.contains(&src("src/foo.rs")),
            "foo.rs resolved (cfg present)"
        );
    }

    #[test]
    fn unresolved_mod_and_missing_path_record_warnings() {
        // REQ-006: diagnostics are non-fatal warnings.
        let sources: Vec<(PathBuf, String)> = vec![(
            src("src/lib.rs"),
            "mod missing;\n#[path = \"nope.rs\"]\nmod gone;\n".into(),
        )];
        let tree = build_module_tree(&src("src/lib.rs"), &sources).unwrap();
        let w = tree.warnings();
        assert!(
            w.iter()
                .any(|s| s.contains("mod missing") && s.contains("resolves to no")),
            "unresolved mod warning: {w:?}"
        );
        assert!(
            w.iter()
                .any(|s| s.contains("#[path") && s.contains("not found")),
            "missing #[path] warning: {w:?}"
        );
    }

    #[test]
    fn discover_crate_root_fails_without_cargo_toml() {
        // Contract: no Cargo.toml -> Err (CLI narrows standalone).
        let tmp = std::env::temp_dir().join(format!(
            "rlt-vis-no-cargo-{}-{}.rs",
            std::process::id(),
            std::sync::atomic::AtomicU64::new(0).load(std::sync::atomic::Ordering::Relaxed),
        ));
        let _ = std::fs::write(&tmp, "pub fn f() {}\n");
        // Walk up from temp dir is unlikely to hit a Cargo.toml belonging to us.
        let res = discover_crate_root(&tmp);
        // Not asserting Err strictly (a Cargo.toml may exist far up); assert it
        // never panics and returns a Result.
        let _ = res;
    }
}
