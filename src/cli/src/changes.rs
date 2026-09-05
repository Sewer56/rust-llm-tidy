//! Change records: the per-edit report a transformation applies (in-place) or
//! would apply (`--dry-run`).
//!
//! A [`Change`] is one edit: a 1-based anchor line, an operation code (`FIX`,
//! `REORDER`, or `VIS`), a stable human-readable message, and an item
//! kind/name.
//!
//! Records are label-level - they describe the affected region from its first
//! line and never embed the reconstructed source bytes.
//!
//! Reorder records come from the reorder crate's `ReorderMove` producer
//! plus one record per member-reordered type ([`reorder_changes`]). Fence fix
//! records come from the per-entity [`rust_llm_tidy_fix::FixAnchor`]s via
//! [`fence_changes`]; tables emit one per-file record via [`table_changes`];
//! link hoists map the fix crate's before/after pairs to records via
//! [`link_changes`]. Vis records come from diffing the narrowed output against
//! the source ([`vis_changes`]).

use rust_llm_tidy_model::parse::ItemKind;
use std::fmt;
use std::num::NonZeroU32;

/// A single edit applied by a transformation.
///
/// Records are never mutated after construction, so owned
/// text rides in `Box<str>`, operation codes ride as `&'static str` without a
/// heap allocation, and the kind is a byte-sized enum.
///
/// Fields are declared so the 4-byte line field sits directly against the enum
/// kind, giving 56 bytes total on 64-bit, down from 96 with `usize` + `String`.
///
/// # Remarks
///
/// Lines are 1-based and non-zero, so `line` uses `Option<NonZeroU32>`
/// (`None` = the record has no specific line).
///
/// The niche makes this 4 bytes - the same size as a plain `u32`, so the
/// packed layout is unchanged - while the type guarantees a record can never
/// report line 0 and serializes to `null` when there is no line.
pub(crate) struct Change {
    /// Optional 1-based line where the affected entity begins (`None` = no line).
    pub(crate) line: Option<NonZeroU32>,
    /// Kind of the affected entity, as a typed value (see [`ChangeKind::as_str`]
    /// for the string form).
    pub(crate) kind: ChangeKind,
    /// Operation code: `FIX`, `REORDER`, or `VIS`.
    pub(crate) code: &'static str,
    /// Stable, human-readable description (never the reconstructed source).
    pub(crate) message: Box<str>,
    /// Name of the affected item, when it has one.
    pub(crate) name: Option<Box<str>>,
}

/// Typed kind of an affected entity.
///
/// Item kinds reuse the model's [`ItemKind`] rather than redeclaring their
/// variants.
///
/// The remaining variants are the fix-pass tags (`fence`, `link`, `table`)
/// and the `extern crate` phrase the vis pass synthesizes, which is not an
/// `ItemKind` (`extern` there means an `extern` block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    /// A parsed source item kind (e.g. `fn`, `struct`).
    Item(ItemKind),
    /// A nested code fence whose delimiter was flipped.
    Fence,
    /// A hoisted inline link.
    Link,
    /// A realigned table.
    Table,
    /// An `extern crate` item whose visibility was narrowed.
    ExternCrate,
}

impl ChangeKind {
    /// The stable string form used by plaintext and JSON output.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Item(kind) => kind.as_str(),
            ChangeKind::Fence => "fence",
            ChangeKind::Link => "link",
            ChangeKind::Table => "table",
            ChangeKind::ExternCrate => "extern crate",
        }
    }
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Link records carry no line, so only prefix numbered records.
        if let Some(line) = self.line {
            write!(f, "{line}: ")?;
        }
        match &self.name {
            Some(name) => write!(
                f,
                "success[{}]: {} ({} `{}`)",
                self.code,
                self.message,
                self.kind.as_str(),
                name
            ),
            None => write!(
                f,
                "success[{}]: {} ({})",
                self.code,
                self.message,
                self.kind.as_str()
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
            line: NonZeroU32::new(a.line),
            code: "FIX",
            message: format!("flip nested fence at line {}", a.line).into_boxed_str(),
            kind: ChangeKind::Fence,
            name: None,
        })
        .collect()
}

/// One [`Change`] per `(before, after)` substitution pair a link hoist reports.
///
/// The pairs come straight from [`fix_links`](rust_llm_tidy_fix::fix_links), so
/// no diff or line tracking is needed here; records carry no line (`None`).
pub(crate) fn link_changes(pairs: &[(String, String)]) -> Vec<Change> {
    pairs
        .iter()
        .map(|(before, after)| Change {
            line: None,
            code: "FIX",
            message: format!("`{before}` -> `{after}`").into_boxed_str(),
            kind: ChangeKind::Link,
            name: None,
        })
        .collect()
}

/// Derive the reorder [`Change`] records for a parsed file and its
/// permutation.
///
/// Two record kinds, both anchored at the affected item's first source
/// line:
///
/// - one per top-level move, from the reorder crate's `ReorderMove`
///   producer (`from` there is the 1-based input position);
/// - one per type whose member permutation is not the identity: member
///   moves carry no top-level `ReorderMove` of their own.
///
/// An identity permutation yields zero records.
///
/// # Arguments
///
/// - `parsed`: the parsed source whose items moved.
/// - `permutation`: the validated permutation the reorder emitted.
pub(crate) fn reorder_changes(
    parsed: &rust_llm_tidy_model::parse::ParseResult,
    permutation: &rust_llm_tidy_reorder::reorder::Permutation,
) -> Vec<Change> {
    let mut change_records = Vec::new();
    for mv in rust_llm_tidy_reorder::compute_moves(&parsed.items, permutation) {
        // `mv.from()` is the 1-based input sequence position.
        let item = &parsed.items[mv.from() - 1];
        change_records.push(Change {
            line: NonZeroU32::new(item.start_line() as u32),
            code: "REORDER",
            message: mv.message().into_boxed_str(),
            kind: ChangeKind::Item(*mv.kind()),
            name: mv.name().map(Box::from),
        });
    }
    // Member moves: one record per type whose member permutation is not the
    // identity, anchored at the type's own line.
    for (idx, item) in parsed.items.iter().enumerate() {
        let moved = permutation
            .member_order(idx)
            .map(|order| order.iter().enumerate().any(|(pos, &m)| pos != m))
            .unwrap_or(false);
        if !moved {
            continue;
        }
        change_records.push(Change {
            line: NonZeroU32::new(item.start_line() as u32),
            code: "REORDER",
            message: format!("rearrange {} members to the profile order", item.kind())
                .into_boxed_str(),
            kind: ChangeKind::Item(*item.kind()),
            name: item.name().map(Box::from),
        });
    }
    change_records
}

/// The one [`Change`] a file emits when its tables were realigned.
///
/// [`fix_tables`](rust_llm_tidy_fix::fix_tables) rewrites whole tables, so a
/// single per-file record suffices; no diff or line tracking is needed and the
/// record carries no line (`None`).
pub(crate) fn table_changes() -> Change {
    Change {
        line: None,
        code: "FIX",
        message: "tables were aligned".into(),
        kind: ChangeKind::Table,
        name: None,
    }
}

/// Derive per-entity vis change records by diffing the narrowed `output`
/// against the `source`.
///
/// Visibility narrowing replaces a bare `pub` token on an item's own line with
/// the floor visibility, so every narrowed item lands on exactly one line where
/// `output` differs from `source`.
///
/// A record is anchored at that line and names the item from the rewritten
/// line. An already-tidy input (`output == source`) yields zero records.
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
        let line = (i + 1) as u32;
        changes.push(Change {
            line: NonZeroU32::new(line),
            code: "VIS",
            message: format!("narrow visibility of `{name}` at line {line}").into_boxed_str(),
            kind,
            name: Some(name.into_boxed_str()),
        });
    }
    changes
}

/// Extract the item kind and simple name from a narrowed output line, which
/// begins with the floor visibility followed by the kind keyword and the item
/// name.
///
/// Leading modifiers (`async`, `unsafe`, `default`, `extern`, and `const`
/// before a kind keyword) are skipped so modifier-carrying items still produce
/// the right record. Returns `None` for lines that are not a narrowed item.
fn line_kind_name(line: &str) -> Option<(ChangeKind, String)> {
    let trimmed = line.trim();
    // The line starts with the floor visibility (`pub(crate)`, `pub(super)`,
    // ...), then (possibly modifiers) the kind keyword, then the item name.
    let rest = trimmed.split_once(char::is_whitespace)?.1.trim_start();
    let toks: Vec<&str> = rest.split_whitespace().collect();

    // `extern crate foo;` - kind is the two-word phrase, name follows `crate`.
    if toks.first() == Some(&"extern") && toks.get(1) == Some(&"crate") {
        let name = clean_name(toks.get(2).copied().unwrap_or(""));
        return Some((ChangeKind::ExternCrate, name));
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
    let name_token = if kind == ItemKind::Static && toks.get(i) == Some(&"mut") {
        toks.get(i + 1).copied().unwrap_or("")
    } else {
        toks.get(i).copied().unwrap_or("")
    };
    let name = clean_name(name_token);
    Some((ChangeKind::Item(kind), name))
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

fn kind_for(w: &str) -> Option<ItemKind> {
    match w {
        "fn" => Some(ItemKind::Fn),
        "struct" => Some(ItemKind::Struct),
        "enum" => Some(ItemKind::Enum),
        "union" => Some(ItemKind::Union),
        "type" => Some(ItemKind::Type),
        "const" => Some(ItemKind::Const),
        "static" => Some(ItemKind::Static),
        "mod" => Some(ItemKind::Mod),
        "trait" => Some(ItemKind::Trait),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_plaintext_matches_lint_shape() {
        let named = Change {
            line: NonZeroU32::new(20),
            code: "REORDER",
            message: "rearrange fn a_main from pos 2 to pos 1 (before b_helper)".into(),
            kind: ChangeKind::Item(ItemKind::Fn),
            name: Some("a_main".into()),
        };
        assert_eq!(
            named.to_string(),
            "20: success[REORDER]: rearrange fn a_main from pos 2 to pos 1 (before b_helper) (fn `a_main`)"
        );
    }

    #[test]
    fn change_plaintext_omits_name_when_unnamed() {
        let unnamed = Change {
            line: NonZeroU32::new(3),
            code: "FIX",
            message: "flip nested fence at line 3".into(),
            kind: ChangeKind::Fence,
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
        assert_eq!(table.line, None);
        assert_eq!(table.kind, ChangeKind::Table);
        assert_eq!(table.message.as_ref(), "tables were aligned");
    }

    #[test]
    fn fence_changes_maps_one_record_per_anchor() {
        let anchors = vec![rust_llm_tidy_fix::FixAnchor {
            line: 7,
            kind: rust_llm_tidy_fix::FixKind::Fence,
        }];
        let changes = fence_changes(&anchors);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].line.unwrap().get(), 7);
        assert_eq!(changes[0].code, "FIX");
        assert_eq!(changes[0].kind, ChangeKind::Fence);
        assert_eq!(changes[0].message.as_ref(), "flip nested fence at line 7");
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
        assert_eq!(changes[0].line, None, "link records carry no line");
        assert_eq!(changes[0].code, "FIX");
        assert_eq!(changes[0].kind, ChangeKind::Link);
        assert_eq!(changes[0].message.as_ref(), "`[A](u)` -> `[A]`");
        assert_eq!(changes[1].message.as_ref(), "`[B](v)` -> `[B]`");
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
        assert_eq!(changes[0].line.unwrap().get(), 2);
        assert_eq!(
            changes[0].message.as_ref(),
            "narrow visibility of `f` at line 2"
        );
        assert_eq!(changes[0].kind, ChangeKind::Item(ItemKind::Fn));
        assert_eq!(changes[1].line.unwrap().get(), 3);
        assert_eq!(
            changes[1].message.as_ref(),
            "narrow visibility of `S` at line 3"
        );
        assert_eq!(changes[1].kind, ChangeKind::Item(ItemKind::Struct));

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
        assert_eq!(kind, ChangeKind::Item(ItemKind::Const));
        assert_eq!(name, "C");
        let (kind, name) = line_kind_name("    pub(crate) fn f<T>() {}").unwrap();
        assert_eq!(kind, ChangeKind::Item(ItemKind::Fn));
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
            assert_eq!(
                (got_kind.as_str(), got_name.as_str()),
                (kind, name),
                "for `{line}`"
            );
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
            assert_eq!(c.line.unwrap().get(), line, "line for `{name}`");
            assert_eq!(c.kind.as_str(), kind, "kind for `{name}`");
            assert_eq!(c.name.as_deref(), Some(name), "name for `{name}`");
        }
    }
}
