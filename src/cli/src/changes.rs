//! Change records: the per-edit report a transformation applies (in-place) or
//! would apply (`--dry-run`).
//!
//! A [`Change`] is one edit: a 1-based anchor line, an operation code (`FIX`,
//! `REORDER`, or `VIS`), a stable human-readable message, and an item
//! kind/name. Records are label-level - they describe the affected region from
//! its first line and never embed the reconstructed source bytes.
//!
//! Reorder records come from the reorder crate's `ReorderMove`. Fence fix
//! records come from the per-entity [`rust_llm_tidy_fix::FixAnchor`]s via
//! [`fence_changes`]; tables emit one per-file record via [`table_changes`];
//! link hoists map the fix crate's before/after pairs to records via
//! [`link_changes`]. Vis records come from diffing the narrowed output against
//! the source ([`vis_changes`]).

use std::fmt;

/// A single edit applied by a transformation.
pub(crate) struct Change {
    /// 1-based line where the affected entity begins.
    pub(crate) line: usize,
    /// Operation code: `FIX`, `REORDER`, or `VIS`.
    pub(crate) code: &'static str,
    /// Stable, human-readable description (never the reconstructed source).
    pub(crate) message: String,
    /// Kind of the affected entity (e.g. `table`, `fn`).
    pub(crate) kind: String,
    /// Name of the affected item, when it has one.
    pub(crate) name: Option<String>,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Link records carry no line (0), so only prefix numbered records.
        if self.line > 0 {
            write!(f, "{}: ", self.line)?;
        }
        match &self.name {
            Some(name) => write!(
                f,
                "success[{}]: {} ({} `{}`)",
                self.code, self.message, self.kind, name
            ),
            None => write!(
                f,
                "success[{}]: {} ({})",
                self.code, self.message, self.kind
            ),
        }
    }
}

/// Map the fences fix pass's per-entity anchors to one [`Change`] per edited
/// entity.
///
/// Each anchor stands for one edit - a flipped fence delimiter - anchored at
/// the entity's first line in that pass's input. `anchors` is empty on a
/// no-op pass, so the mapping yields zero records.
pub(crate) fn fence_changes(anchors: &[rust_llm_tidy_fix::FixAnchor]) -> Vec<Change> {
    anchors
        .iter()
        .map(|a| Change {
            line: a.line,
            code: "FIX",
            message: format!("flip nested fence at line {}", a.line),
            kind: "fence".to_string(),
            name: None,
        })
        .collect()
}

/// One [`Change`] per `(before, after)` substitution pair a link hoist reports.
///
/// The pairs come straight from [`fix_links`](rust_llm_tidy_fix::fix_links), so
/// no diff or line tracking is needed here; records carry no line (0).
pub(crate) fn link_changes(pairs: &[(String, String)]) -> Vec<Change> {
    pairs
        .iter()
        .map(|(before, after)| Change {
            line: 0,
            code: "FIX",
            message: format!("`{before}` -> `{after}`"),
            kind: "link".to_string(),
            name: None,
        })
        .collect()
}

/// The one [`Change`] a file emits when its tables were realigned.
///
/// [`fix_tables`](rust_llm_tidy_fix::fix_tables) rewrites whole tables, so a
/// single per-file record suffices; no diff or line tracking is needed and the
/// record carries no line (0).
pub(crate) fn table_changes() -> Change {
    Change {
        line: 0,
        code: "FIX",
        message: "tables were aligned".to_string(),
        kind: "table".to_string(),
        name: None,
    }
}

/// Derive per-entity vis change records by diffing the narrowed `output`
/// against the `source`.
///
/// Visibility narrowing replaces a bare `pub` token on an item's own line with
/// the floor visibility, so every narrowed item lands on exactly one line where
/// `output` differs from `source`. A record is anchored at that line and names
/// the item from the rewritten line. An already-tidy input (`output == source`)
/// yields zero records.
pub(crate) fn vis_changes(source: &str, output: &str) -> Vec<Change> {
    if output == source {
        return Vec::new();
    }
    let src_lines: Vec<&str> = source.lines().collect();
    let out_lines: Vec<&str> = output.lines().collect();
    let count = out_lines.len().max(src_lines.len());
    let mut changes = Vec::new();
    for i in 0..count {
        if src_lines.get(i) == out_lines.get(i) {
            continue;
        }
        // Parsing never fails on a rewritten item line; guard defensively so a
        // surprising rewrite is skipped rather than panicking mid-report.
        let Some((kind, name)) = line_kind_name(out_lines.get(i).copied().unwrap_or("")) else {
            continue;
        };
        changes.push(Change {
            line: i + 1,
            code: "VIS",
            message: format!("narrow visibility of `{name}` at line {}", i + 1),
            kind: kind.to_string(),
            name: Some(name),
        });
    }
    changes
}

/// Extract the item kind and simple name from a narrowed output line, which
/// begins with the floor visibility followed by the kind keyword and the item
/// name. Leading modifiers (`async`, `unsafe`, `default`, `extern`, and `const`
/// before a kind keyword) are skipped so modifier-carrying items still produce
/// the right record. Returns `None` for lines that are not a narrowed item.
fn line_kind_name(line: &str) -> Option<(&'static str, String)> {
    let trimmed = line.trim();
    // The line starts with the floor visibility (`pub(crate)`, `pub(super)`,
    // ...), then (possibly modifiers) the kind keyword, then the item name.
    let rest = trimmed.split_once(char::is_whitespace)?.1.trim_start();
    let toks: Vec<&str> = rest.split_whitespace().collect();

    // `extern crate foo;` - kind is the two-word phrase, name follows `crate`.
    if toks.first() == Some(&"extern") && toks.get(1) == Some(&"crate") {
        let name = clean_name(toks.get(2).copied().unwrap_or(""));
        return Some(("extern crate", name));
    }

    // Skip leading modifiers until the first kind keyword. `const` is a
    // modifier only when followed by another modifier or a kind keyword.
    let mut i = 0;
    while i < toks.len() {
        let w = toks[i];
        if is_modifier(w) {
            // `extern "C" fn` also skips the ABI string; a bare `extern` (C
            // ABI) is skipped alone.
            if w == "extern" && toks.get(i + 1).is_some_and(|t| t.starts_with('"')) {
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if w == "const" {
            match toks.get(i + 1).copied() {
                Some(next) if is_modifier(next) || is_kind(next) => {
                    i += 1; // `const fn`, `const unsafe fn`, `const trait`
                    continue;
                }
                _ => break, // `const C: u32 = ...` - the kind itself
            }
        }
        break;
    }

    let kind = kind_for(toks.get(i).copied()?)?;
    i += 1;

    // `static mut X` - skip the `mut` storage modifier before the name.
    let name_token = if kind == "static" && toks.get(i) == Some(&"mut") {
        toks.get(i + 1).copied().unwrap_or("")
    } else {
        toks.get(i).copied().unwrap_or("")
    };
    let name = clean_name(name_token);
    Some((kind, name))
}

/// Reduce a name token like `S;`, `C:`, `f()`, or `T<T>` to the item's simple
/// identifier by cutting at the first terminating character.
fn clean_name(token: &str) -> String {
    token
        .split([';', ':', '(', '<', '=', '{'])
        .next()
        .unwrap_or("")
        .to_string()
}

fn is_kind(w: &str) -> bool {
    kind_for(w).is_some()
}

fn is_modifier(w: &str) -> bool {
    matches!(w, "async" | "unsafe" | "default" | "extern")
}

fn kind_for(w: &str) -> Option<&'static str> {
    match w {
        "fn" => Some("fn"),
        "struct" => Some("struct"),
        "enum" => Some("enum"),
        "union" => Some("union"),
        "type" => Some("type"),
        "const" => Some("const"),
        "static" => Some("static"),
        "mod" => Some("mod"),
        "trait" => Some("trait"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_plaintext_matches_lint_shape() {
        let named = Change {
            line: 20,
            code: "REORDER",
            message: "rearrange fn a_main from pos 2 to pos 1 (before b_helper)".to_string(),
            kind: "fn".to_string(),
            name: Some("a_main".to_string()),
        };
        assert_eq!(
            named.to_string(),
            "20: success[REORDER]: rearrange fn a_main from pos 2 to pos 1 (before b_helper) (fn `a_main`)"
        );
    }

    #[test]
    fn change_plaintext_omits_name_when_unnamed() {
        let unnamed = Change {
            line: 3,
            code: "FIX",
            message: "flip nested fence at line 3".to_string(),
            kind: "fence".to_string(),
            name: None,
        };
        assert_eq!(
            unnamed.to_string(),
            "3: success[FIX]: flip nested fence at line 3 (fence)"
        );
    }

    #[test]
    fn change_plaintext_omits_line_when_zero() {
        let table = table_changes();
        assert_eq!(
            table.to_string(),
            "success[FIX]: tables were aligned (table)"
        );
        assert_eq!(table.line, 0);
        assert_eq!(table.kind, "table");
        assert_eq!(table.message, "tables were aligned");
    }

    #[test]
    fn fence_changes_maps_one_record_per_anchor() {
        let anchors = vec![rust_llm_tidy_fix::FixAnchor {
            line: 7,
            kind: rust_llm_tidy_fix::FixKind::Fence,
            name: None,
        }];
        let changes = fence_changes(&anchors);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].line, 7);
        assert_eq!(changes[0].code, "FIX");
        assert_eq!(changes[0].kind, "fence");
        assert_eq!(changes[0].message, "flip nested fence at line 7");
    }

    #[test]
    fn fence_changes_is_empty_without_anchors() {
        assert!(fence_changes(&[]).is_empty());
    }

    #[test]
    fn link_changes_maps_each_pair_to_a_record() {
        let pairs = vec![
            ("[A](u)".to_string(), "[A]".to_string()),
            ("[B](v)".to_string(), "[B]".to_string()),
        ];
        let changes = link_changes(&pairs);
        assert_eq!(changes.len(), 2, "one record per pair");
        assert_eq!(changes[0].line, 0, "link records carry no line");
        assert_eq!(changes[0].code, "FIX");
        assert_eq!(changes[0].kind, "link");
        assert_eq!(changes[0].message, "`[A](u)` -> `[A]`");
        assert_eq!(changes[1].message, "`[B](v)` -> `[B]`");
        assert_eq!(
            changes[1].to_string(),
            "success[FIX]: `[B](v)` -> `[B]` (link)"
        );
    }

    #[test]
    fn link_changes_is_empty_without_pairs() {
        assert!(link_changes(&[]).is_empty());
    }

    #[test]
    fn vis_changes_anchors_each_narrowed_item() {
        let source = "pub(crate) mod m {\n    pub fn f() {}\n    pub struct S;\n}\n";
        let output = "pub(crate) mod m {\n    pub(crate) fn f() {}\n    pub(crate) struct S;\n}\n";
        let changes = vis_changes(source, output);
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].line, 2);
        assert_eq!(changes[0].message, "narrow visibility of `f` at line 2");
        assert_eq!(changes[0].kind, "fn");
        assert_eq!(changes[1].line, 3);
        assert_eq!(changes[1].message, "narrow visibility of `S` at line 3");
        assert_eq!(changes[1].kind, "struct");

        // A tidy pair reports zero records for modifier-carrying items too.
        let tidy = "pub(crate) mod m {\n    pub(crate) async fn f() {}\n}\n";
        assert!(vis_changes(tidy, tidy).is_empty());
    }

    #[test]
    fn vis_changes_is_empty_when_tidy() {
        assert!(vis_changes("same", "same").is_empty());
    }

    #[test]
    fn line_kind_name_handles_const_and_generics() {
        let (kind, name) = line_kind_name("    pub(crate) const C: u32 = 0;").unwrap();
        assert_eq!(kind, "const");
        assert_eq!(name, "C");
        let (kind, name) = line_kind_name("    pub(crate) fn f<T>() {}").unwrap();
        assert_eq!(kind, "fn");
        assert_eq!(name, "f");
        assert!(line_kind_name("    let x = 1;").is_none());
        assert!(line_kind_name("    use crate::m;").is_none());
    }

    #[test]
    fn line_kind_name_skips_modifier_carrying_items() {
        for (line, kind, name) in [
            ("    pub(crate) fn f() {}", "fn", "f"),
            ("    pub(crate) async fn f() {}", "fn", "f"),
            ("    pub(crate) unsafe fn g() {}", "fn", "g"),
            ("    pub(crate) unsafe trait T {}", "trait", "T"),
            ("    pub(crate) const fn h() {}", "fn", "h"),
            ("    pub(crate) const unsafe fn j() {}", "fn", "j"),
            ("    pub(crate) extern \"C\" fn k() {}", "fn", "k"),
            ("    pub(crate) extern fn l() {}", "fn", "l"),
            ("    pub(crate) extern crate foo;", "extern crate", "foo"),
            ("    pub(crate) const C: u32 = 0;", "const", "C"),
            ("    pub(crate) static X: i32 = 0;", "static", "X"),
            ("    pub(crate) static mut X: i32 = 0;", "static", "X"),
        ] {
            let (got_kind, got_name) = line_kind_name(line).unwrap();
            assert_eq!((got_kind, got_name.as_str()), (kind, name), "for `{line}`");
        }
    }

    #[test]
    fn vis_changes_reports_modifier_carrying_items() {
        let source = "pub(crate) mod m {\n    pub async fn f() {}\n    pub unsafe fn g() {}\n    pub unsafe trait T {}\n    pub const fn h() {}\n    pub extern \"C\" fn k() {}\n    pub static mut X: i32 = 0;\n}\n";
        let output = "pub(crate) mod m {\n    pub(crate) async fn f() {}\n    pub(crate) unsafe fn g() {}\n    pub(crate) unsafe trait T {}\n    pub(crate) const fn h() {}\n    pub(crate) extern \"C\" fn k() {}\n    pub(crate) static mut X: i32 = 0;\n}\n";
        let changes = vis_changes(source, output);
        let expected = [
            (2, "fn", "f"),
            (3, "fn", "g"),
            (4, "trait", "T"),
            (5, "fn", "h"),
            (6, "fn", "k"),
            (7, "static", "X"),
        ];
        assert_eq!(changes.len(), expected.len());
        for (c, (line, kind, name)) in changes.iter().zip(expected) {
            assert_eq!(c.line, line, "line for `{name}`");
            assert_eq!(c.kind, kind, "kind for `{name}`");
            assert_eq!(c.name.as_deref(), Some(name), "name for `{name}`");
        }
    }
}
