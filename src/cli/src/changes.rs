//! Dry-run change records: the per-edit report a transformation would apply.
//!
//! A [`Change`] is one would-be edit under `--dry-run`: a 1-based anchor line,
//! an operation code (`FIX`, `REORDER`, or `VIS`), a stable human-readable
//! message, and an item kind/name. Records are label-level - they describe the
//! affected region from its first line and never embed the reconstructed
//! source bytes.
//!
//! Reorder records come from the reorder crate's `ReorderMove` (already
//! derived from the computed permutation). Fix and vis records are derived
//! here by diffing each transformation's before/after output, the lower-risk
//! default chosen over exposing a per-edit API from those crates.

use std::fmt;

/// A single would-be edit reported by a transformation in dry-run mode.
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
    /// 1-based input sequence position of a reorder move, when present.
    pub(crate) from: Option<usize>,
    /// 1-based output sequence position of a reorder move, when present.
    pub(crate) to: Option<usize>,
    /// Name of the item a reorder move lands before, when it is not last in the
    /// reordered output.
    pub(crate) before_name: Option<String>,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(
                f,
                "{}: success[{}]: {} ({} `{}`)",
                self.line, self.code, self.message, self.kind, name
            ),
            None => write!(
                f,
                "{}: success[{}]: {} ({})",
                self.line, self.code, self.message, self.kind
            ),
        }
    }
}

/// Derive a fix change record for a single pass, if that pass changed the text
/// it received. `pass` is one of `"tables"`, `"fences"`, or `"links"` and
/// selects the record's kind and message wording.
pub(crate) fn fix_pass_change(pass: &str, before: &str, after: &str) -> Option<Change> {
    let line = first_differing_line(before, after)?;
    let (kind, message) = match pass {
        "tables" => ("table", format!("realign table starting at line {line}")),
        "fences" => ("fence", format!("flip nested fence at line {line}")),
        "links" => ("link", format!("hoist link starting at line {line}")),
        _ => unreachable!("unknown fix pass: {pass}"),
    };
    Some(Change {
        line,
        code: "FIX",
        message,
        kind: kind.to_string(),
        name: None,
        from: None,
        to: None,
        before_name: None,
    })
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
            from: None,
            to: None,
            before_name: None,
        });
    }
    changes
}

/// First 1-based line where `a` and `b` differ, or `None` when they are equal.
///
/// Used to anchor a fix record at the first line a pass actually rewrites.
fn first_differing_line(a: &str, b: &str) -> Option<usize> {
    if a == b {
        return None;
    }
    let mut la = a.lines();
    let mut lb = b.lines();
    let mut n = 0usize;
    loop {
        n += 1;
        match (la.next(), lb.next()) {
            (Some(x), Some(y)) if x == y => continue,
            _ => return Some(n),
        }
    }
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
            from: Some(2),
            to: Some(1),
            before_name: Some("b_helper".to_string()),
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
            message: "realign table starting at line 3".to_string(),
            kind: "table".to_string(),
            name: None,
            from: None,
            to: None,
            before_name: None,
        };
        assert_eq!(
            unnamed.to_string(),
            "3: success[FIX]: realign table starting at line 3 (table)"
        );
    }

    #[test]
    fn first_differing_line_is_one_based() {
        assert_eq!(first_differing_line("a\nb", "a\nb"), None);
        assert_eq!(first_differing_line("a\nb", "a\nc"), Some(2));
        assert_eq!(first_differing_line("a\nb\nc", "a\nb"), Some(3));
        assert_eq!(first_differing_line("", "x"), Some(1));
    }

    #[test]
    fn fix_pass_change_reports_only_changed_passes() {
        let c = fix_pass_change("tables", "| a | 1 |", "| ----- |").unwrap();
        assert_eq!(c.line, 1);
        assert_eq!(c.code, "FIX");
        assert_eq!(c.kind, "table");
        assert_eq!(c.message, "realign table starting at line 1");
        assert!(fix_pass_change("tables", "same", "same").is_none());
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
