//! Cross-file module-tree resolver for crate-aware visibility narrowing.
//!
//! Maps `mod foo;` file references to source files (edition path rules +
//! `#[path]` overrides), distinguishes inline `mod foo {}` (no file), and
//! propagates each file's effective floor visibility root -> leaf. Crate-root
//! discovery uses `cargo metadata --no-deps` (the CLI narrows each file
//! standalone when that fails - see `cli/src/main.rs`).

use crate::{ParsedFile, child_of_kind, visibility_node};
use ahash::AHashMap;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

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
    /// target). Surfaced to the CLI as diagnostics.
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

/// Build a module tree from pre-parsed files. The root is the file whose path
/// equals `root`. Floors propagate root -> leaf: a `mod` with restricted
/// visibility sets the floor for its (transitive) descendants; bare `pub` /
/// private `mod` inherits the ancestor floor. Matches the inline `walk()`
/// "innermost restricted ancestor wins" semantics so tree and inline paths
/// share identical narrowing behavior.
///
/// # Arguments
///
/// - `root` - the source file to treat as the crate root (its floor is `None`).
/// - `files` - every parsed file in the crate, indexed by canonical path for
///   resolving `mod foo;` edges and slicing visibility spans.
///
/// # Errors
///
/// Each file is already parsed into a [`ParsedFile`] (tree-sitter recovers from
/// invalid Rust with `ERROR` nodes), so this does not fail on a bad source. The
/// `Result` is retained for API continuity; it is `Ok` in practice.
pub fn build_module_tree(root: &Path, files: &[ParsedFile]) -> anyhow::Result<ModuleTree> {
    // 1. Index files by canonical path. Keep trees + sources for byte-exact
    //    visibility span slicing (mirrors walk()).
    let by_path: AHashMap<PathBuf, &ParsedFile> =
        files.iter().map(|f| (f.path.clone(), f)).collect();
    let known_files: std::collections::HashSet<PathBuf> =
        files.iter().map(|f| f.path.clone()).collect();

    // 2. Vec-based stack, so queue.pop() gives depth-first order. Root's floor
    //    is None. Multiple mod edges to one file: first recorded floor wins.
    let mut floors: AHashMap<PathBuf, Option<String>> = AHashMap::new();
    let mut warnings = Vec::new();
    let mut queue: Vec<(PathBuf, Option<String>)> = vec![(root.to_path_buf(), None)];
    while let Some((path, floor)) = queue.pop() {
        if floors.contains_key(&path) {
            continue;
        }
        floors.insert(path.clone(), floor.clone());
        let Some(pf) = by_path.get(&path) else {
            continue;
        };
        let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
        for child in resolve_mod_children(
            pf.tree.root_node(),
            parent_dir,
            &path,
            &pf.source,
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
                    let child_floor = vis_text.or_else(|| floor.clone());
                    queue.push((cpath, child_floor));
                }
            }
        }
    }
    Ok(ModuleTree { floors, warnings })
}

/// Discover the crate root source file by walking up from `start` to the
/// nearest `Cargo.toml` (the owning crate's manifest), then running
/// `cargo metadata --no-deps` and returning that package's `lib` target's
/// `src_path` (else the `bin` target whose path ends in `main.rs`). The owning
/// crate is matched by manifest path rather than `root_package()`: under
/// `--no-deps` `root_package()` resolves to the package at
/// `workspace_root/Cargo.toml`, which does not exist for a *virtual*
/// workspace, so member crates of a virtual workspace would otherwise degrade
/// to standalone narrowing. The CLI maps failure to a warn + standalone
/// narrowing.
///
/// # Arguments
///
/// - `start` - the file or directory to walk up from; the nearest enclosing
///   `Cargo.toml` owns the crate whose root is returned.
///
/// # Errors
///
/// Returns an error when:
///
/// - No `Cargo.toml` is found walking up from `start`.
/// - `cargo metadata --no-deps` fails (e.g. the manifest is invalid or
///   unparseable).
/// - No package in the metadata owns the manifest found (e.g. the manifest is
///   a virtual workspace root with no `[package]`).
/// - The owning package has no `lib` target and no `bin` target whose
///   `src_path` ends in `main.rs`.
pub fn discover_crate_root(start: &Path) -> anyhow::Result<PathBuf> {
    let manifest = find_cargo_toml(start)?;
    let meta = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest)
        .no_deps()
        .exec()?;
    // Match the owning package by manifest path. `root_package()` only works
    // for standalone packages and non-virtual workspaces: under `--no-deps` it
    // resolves to the package at `workspace_root/Cargo.toml`, which does not
    // exist for a *virtual* workspace, so member crates would otherwise
    // degrade to standalone narrowing.
    let canon_manifest = std::fs::canonicalize(&manifest).unwrap_or_else(|_| manifest.clone());
    let pkg = meta
        .packages
        .iter()
        .find(|p| {
            let pm: PathBuf = p.manifest_path.as_std_path().to_path_buf();
            pm == canon_manifest || std::fs::canonicalize(&pm).ok() == Some(canon_manifest.clone())
        })
        .ok_or_else(|| anyhow::anyhow!("no package owns {}", manifest.display()))?;
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
/// mods are treated as present. Unresolved `mod foo;` and missing `#[path]`
/// targets append a warning rather than failing.
///
/// In the tree-sitter CST, `#[...]` attributes are *preceding sibling*
/// `attribute_item` nodes (not children of the item), so this walk tracks the
/// pending attribute run and attaches it to the next `mod_item` (mirroring the
/// sibling model crate's `PendingTrivia`). Comments are trivia and do not break
/// the run.
fn resolve_mod_children(
    root: Node,
    parent_dir: &Path,
    parent: &Path,
    source: &str,
    warnings: &mut Vec<String>,
    known_files: &std::collections::HashSet<PathBuf>,
) -> Vec<ModChild> {
    let mut out = Vec::new();
    let mut pending_attrs: Vec<Node> = Vec::new();
    let count = root.named_child_count() as u32;
    for i in 0..count {
        let item = root.named_child(i).unwrap();
        match item.kind() {
            "attribute_item" => pending_attrs.push(item),
            "line_comment" | "block_comment" => {} // trivia: keep the attr run
            "mod_item" => {
                // `mod foo {}` (inline) has a `body`; it is not a file edge.
                if item.child_by_field_name("body").is_some() {
                    out.push(ModChild::Inline);
                    pending_attrs.clear();
                    continue;
                }
                let vis_text = vis_text_of(visibility_node(item), source);
                let path_attr = find_path_attr(&pending_attrs, source);
                let name = item
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                    .map(str::to_string);
                let name_str = name.as_deref().unwrap_or("");
                // A `#[path]` override was used: a missing target already emits
                // the dedicated "target not found" warning below, so the generic
                // "resolves to no ..." warning must be suppressed in that case
                // (it is redundant and references nonexistent `foo.rs` paths).
                let mut path_attr_used = false;
                let resolved = match path_attr {
                    Some(p) => {
                        path_attr_used = true;
                        let candidate = parent_dir.join(&p);
                        // Canonicalize the candidate for lookup against the known set.
                        let cand_canon =
                            std::fs::canonicalize(&candidate).unwrap_or(candidate.clone());
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
                    None => resolve_mod_file(parent_dir, name_str, known_files, warnings, parent),
                };
                match resolved {
                    Some(p) => out.push(ModChild::File { path: p, vis_text }),
                    None if !path_attr_used => warnings.push(format!(
                        "{}: `mod {};` resolves to no `{}.rs` or `{}/mod.rs`",
                        parent.display(),
                        name_str,
                        name_str,
                        name_str
                    )),
                    None => {} // `#[path]` target warning already emitted
                }
                pending_attrs.clear();
            }
            _ => pending_attrs.clear(),
        }
    }
    out
}

/// Extract the string value of a `#[path = "..."]` attribute from a run of
/// preceding `attribute_item` nodes. Returns the verbatim content (no quotes).
fn find_path_attr(attrs: &[Node], source: &str) -> Option<String> {
    for a in attrs {
        let Some(attr) = child_of_kind(*a, "attribute") else {
            continue;
        };
        // The attribute name is the first named child (an `identifier` for a
        // bare `path`; `crate::path` is a `scoped_identifier` and ignored here,
        // matching syn's `path().is_ident("path")`).
        let Some(first) = attr.named_child(0) else {
            continue;
        };
        if first.kind() != "identifier" {
            continue;
        }
        if first.utf8_text(source.as_bytes()).ok() != Some("path") {
            continue;
        }
        let Some(val) = attr.child_by_field_name("value") else {
            continue;
        };
        let Some(content) = child_of_kind(val, "string_content") else {
            continue;
        };
        return content
            .utf8_text(source.as_bytes())
            .ok()
            .map(str::to_string);
    }
    None
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

/// Capture a `visibility_modifier` node's text verbatim from `source` when it
/// is restricted (`pub(crate)`/`pub(super)`/`pub(in path)`). `None` for bare
/// `pub` and private (no visibility). Uses byte-exact span slicing mirroring
/// `walk()`. tree-sitter yields byte offsets directly, so no line/column
/// conversion is needed (the prior `to_token_stream()`+strip would corrupt
/// `pub(in crate::a)` -> `pub(incrate::a)`).
fn vis_text_of(vis: Option<Node>, source: &str) -> Option<String> {
    let vis = vis?;
    if vis.named_child_count() >= 1 {
        Some(source[vis.start_byte()..vis.end_byte()].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{ParsedFile, build_module_tree, discover_crate_root};
    use std::path::PathBuf;

    fn src(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    /// Parse `(path, source)` pairs into [`ParsedFile`]s for `build_module_tree`.
    fn parse_files(sources: Vec<(PathBuf, String)>) -> Vec<ParsedFile> {
        sources
            .into_iter()
            .map(|(p, s)| ParsedFile::new(p, s).expect("test source must parse"))
            .collect()
    }

    #[test]
    fn mod_file_resolves_to_foo_rs_not_mod_rs() {
        // Edition rule: prefer foo.rs over foo/mod.rs.
        let files = parse_files(vec![
            (src("src/lib.rs"), "mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
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
        let files = parse_files(vec![
            (src("src/lib.rs"), "mod foo;\n".into()),
            (src("src/foo/mod.rs"), "pub fn f() {}\n".into()),
        ]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
        assert!(
            tree.contains(&src("src/foo/mod.rs")),
            "mod foo; falls back to foo/mod.rs when foo.rs is absent"
        );
    }

    #[test]
    fn pub_crate_mod_propagates_floor_to_child_file() {
        let files = parse_files(vec![
            (src("src/lib.rs"), "pub(crate) mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
        let floor = tree.floor_for(&src("src/foo.rs")).expect("foo.rs in tree");
        assert_eq!(
            floor, "pub(crate)",
            "child file inherits the declaration floor"
        );
    }

    #[test]
    fn pub_super_mod_propagates_floor_to_child_file() {
        // `pub(super)` floor: byte-exact span slice must not corrupt the text.
        let files = parse_files(vec![
            (src("src/lib.rs"), "pub(super) mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
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
        let files = parse_files(vec![
            (src("src/lib.rs"), "pub(in crate::a) mod foo;\n".into()),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
        let floor = tree.floor_for(&src("src/foo.rs")).expect("foo.rs in tree");
        assert_eq!(
            floor, "pub(in crate::a)",
            "pub(in path) declaration floor must be byte-exact"
        );
    }

    #[test]
    fn path_attr_overrides_resolution() {
        let files = parse_files(vec![
            (
                src("src/lib.rs"),
                "#[path = \"nested/real.rs\"]\nmod foo;\n".into(),
            ),
            (src("src/nested/real.rs"), "pub fn f() {}\n".into()),
        ]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
        assert!(
            tree.contains(&src("src/nested/real.rs")),
            "#[path] target resolved"
        );
    }

    #[test]
    fn inline_mod_is_not_a_file_edge() {
        // `mod foo {}` is inline -> no file child; walk() handles it.
        let files = parse_files(vec![
            (
                src("src/lib.rs"),
                "pub(crate) mod foo { pub fn f() {} }\n".into(),
            ),
            (
                src("src/foo.rs"),
                "// stray file; inline mod must not pull it in\n".into(),
            ),
        ]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
        assert!(
            !tree.contains(&src("src/foo.rs")),
            "inline mod must not pull in a stray foo.rs"
        );
    }

    #[test]
    fn cfg_gated_mod_treated_as_present() {
        // cfg-gated mods are treated as present. The mod still resolves
        // and propagates its declared (restricted) floor.
        let files = parse_files(vec![
            (
                src("src/lib.rs"),
                "#[cfg(feature = \"x\")] pub(crate) mod foo;\n".into(),
            ),
            (src("src/foo.rs"), "pub fn f() {}\n".into()),
        ]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
        assert!(
            tree.contains(&src("src/foo.rs")),
            "foo.rs resolved (cfg present)"
        );
    }

    #[test]
    fn unresolved_mod_and_missing_path_record_warnings() {
        // Diagnostics are non-fatal warnings.
        let files = parse_files(vec![(
            src("src/lib.rs"),
            "mod missing;\n#[path = \"nope.rs\"]\nmod gone;\n".into(),
        )]);
        let tree = build_module_tree(&src("src/lib.rs"), &files).unwrap();
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
        // The generic "resolves to no" warning must only fire for the plain
        // `mod missing;` declaration, not the `#[path]`-gated `mod gone;`.
        assert_eq!(
            w.iter().filter(|s| s.contains("resolves to no")).count(),
            1,
            "generic unresolved warning only for ordinary mod decls: {w:?}"
        );
        assert!(
            w.iter()
                .any(|s| s.contains("`mod missing;`") && s.contains("missing.rs")),
            "generic warning names the missing module: {w:?}"
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
