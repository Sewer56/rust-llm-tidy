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
/// name. Returns `None` for lines that are not a narrowed item.
fn line_kind_name(line: &str) -> Option<(&'static str, String)> {
    let trimmed = line.trim();
    // The line starts with the floor visibility (`pub(crate)`, `pub(super)`,
    // ...), then the kind keyword, then the item name.
    let rest = trimmed.split_once(char::is_whitespace)?.1.trim_start();
    let mut tokens = rest.split_whitespace();
    let keyword = tokens.next()?;
    if keyword == "extern" {
        // `extern crate foo;` - kind is the two-word phrase, name follows `crate`.
        let _crate = tokens.next()?;
        let name = clean_name(tokens.next().unwrap_or(""));
        return Some(("extern crate", name));
    }
    let kind = match keyword {
        "fn" => "fn",
        "struct" => "struct",
        "enum" => "enum",
        "union" => "union",
        "type" => "type",
        "const" => "const",
        "static" => "static",
        "mod" => "mod",
        "trait" => "trait",
        _ => return None,
    };
    let name = clean_name(tokens.next().unwrap_or(""));
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
            message: "realign table starting at line 3".to_string(),
            kind: "table".to_string(),
            name: None,
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
    }
}
