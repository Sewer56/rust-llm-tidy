//! Permutation validation and byte-slice emit.
//!
//! Partially vendored from rust-reorder (MIT); modified based on
//! <https://github.com/umwelt-ai/rust-reorder>.

use crate::source::line_endings::dominant_line_ending;
use crate::source::{ItemKind, ParseResult, SourceItem};
use ahash::AHashMap;
use anyhow::{Result, ensure};
use core::fmt;
use core::ops::Range;

/// A validated permutation of items.
///
/// Wraps a `Vec<usize>` that maps output position → input item index.
/// Every index in `0..n` appears exactly once. Items with in-type member
/// reordering also carry a member permutation
/// ([`Permutation::set_member_order`]).
#[derive(Debug, Clone)]
pub struct Permutation {
    order: Vec<usize>,
    /// In-type member permutations by item index. Empty unless member
    /// reordering applies; the Rust parse emits no members.
    member_orders: AHashMap<usize, Vec<usize>>,
    /// Protected emission keeps original trivia instead of deriving spacing.
    preserve_spacing: bool,
}

/// A single reorder move: one item whose output position differs from its
/// input position.
///
/// Positions are 1-based item sequence positions, matching the user-visible
/// `from pos A to pos B` reporting. This type is deliberately serde-free; the
/// CLI layer is responsible for its own serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct ReorderMove {
    /// 1-based output position of the moved item.
    to: usize,
    /// 1-based input position of the moved item.
    from: usize,
    /// Description of the item that directly follows this one in the reordered
    /// output (the item it lands before), if any.
    before: Option<Box<str>>,
    /// Kind of the moved item (e.g. `fn`, `impl`).
    kind: ItemKind,
    /// Name of the moved item, when it has one.
    name: Option<Box<str>>,
    /// 1-based source line where the moved item starts, used to describe
    /// unnamed items (e.g. impl blocks).
    line: usize,
}

impl Permutation {
    /// Create a new permutation.
    ///
    /// `n` is the total number of items.
    /// `order` must contain every index in `0..n` exactly once.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `order.len() != n` (length mismatch).
    /// - any index in `order` is `>= n` (out of range).
    /// - any index appears more than once in `order` (duplicate).
    pub fn new(n: usize, order: Vec<usize>) -> Result<Self> {
        ensure!(
            order.len() == n,
            "permutation length {} does not match item count {}",
            order.len(),
            n
        );

        let mut seen = vec![false; n];
        for &idx in &order {
            ensure!(idx < n, "permutation index {} out of range (n={})", idx, n);
            ensure!(!seen[idx], "duplicate index {} in permutation", idx);
            seen[idx] = true;
        }

        Ok(Self {
            order,
            member_orders: AHashMap::new(),
            preserve_spacing: false,
        })
    }

    /// Attach the member permutation for the type item at `item_idx`.
    ///
    /// `member_order` must be a permutation of `0..member_count`, where
    /// `member_count` is that item's member count; [`emit`] splices the
    /// item's members in this order.
    ///
    /// # Errors
    ///
    /// Returns an error if `member_order` is not a permutation of
    /// `0..member_count`:
    /// - `member_order.len() != member_count` (length mismatch).
    /// - any index `>= member_count` (out of range).
    /// - any index appearing more than once (duplicate).
    ///
    /// [`emit`]: fn@emit
    pub fn set_member_order(
        &mut self,
        item_idx: usize,
        member_count: usize,
        member_order: Vec<usize>,
    ) -> Result<()> {
        ensure!(
            member_order.len() == member_count,
            "member permutation length {} does not match member count {}",
            member_order.len(),
            member_count
        );

        let mut seen = vec![false; member_count];
        for &idx in &member_order {
            ensure!(
                idx < member_count,
                "member permutation index {} out of range (n={})",
                idx,
                member_count
            );
            ensure!(!seen[idx], "duplicate index {} in member permutation", idx);
            seen[idx] = true;
        }

        self.member_orders.insert(item_idx, member_order);
        Ok(())
    }

    /// Pin overlapping items and members, preserving order within each free run.
    ///
    /// Ranges refer to the current parsed source. A protected nested declaration
    /// pins its enclosing item; unprotected members may still reorder on either
    /// side of a protected member. Emission retains original whitespace.
    ///
    /// # Errors
    ///
    /// Returns an error for:
    /// - An item count different from this permutation's count.
    /// - A member order with a missing item or different member count.
    /// - A reversed range or an endpoint outside source or inside a UTF-8 character.
    /// - Nonempty protection supplied for a syntax-error tree.
    pub(crate) fn protect(&mut self, parsed: &ParseResult, ranges: &[Range<usize>]) -> Result<()> {
        ensure!(
            self.order.len() == parsed.items.len(),
            "protected permutation item count mismatch"
        );
        for range in ranges {
            ensure!(
                range.start <= range.end
                    && parsed.source.is_char_boundary(range.start)
                    && parsed.source.is_char_boundary(range.end),
                "invalid protection byte range"
            );
        }
        if ranges.is_empty() {
            return Ok(());
        }
        ensure!(
            !parsed.syntax_tree().root_node().has_error(),
            "cannot protect a syntax-error tree"
        );
        for (&index, order) in &self.member_orders {
            ensure!(
                parsed
                    .items
                    .get(index)
                    .is_some_and(|item| item.members().len() == order.len()),
                "protected permutation member count mismatch"
            );
        }

        let overlaps = |start, end| {
            ranges
                .iter()
                .any(|range| start < range.end && range.start < end)
        };
        pin_runs(&mut self.order, |index| {
            let item = &parsed.items[index];
            overlaps(item.start, item.end)
        });
        for (&index, order) in &mut self.member_orders {
            let members = parsed.items[index].members();
            pin_runs(order, |index| {
                overlaps(members[index].start, members[index].end)
            });
        }
        self.preserve_spacing = true;
        Ok(())
    }

    /// Return the underlying order vector.
    pub fn into_inner(self) -> Vec<usize> {
        self.order
    }

    /// The attached member permutation of the type item at `item_idx`, if
    /// one was set; `None` for plain items and items whose members never
    /// reordered.
    ///
    /// The slice indexes into the item's members; an identity permutation
    /// means the item emits its original bytes.
    pub fn member_order(&self, item_idx: usize) -> Option<&[usize]> {
        self.member_orders.get(&item_idx).map(Vec::as_slice)
    }
}

impl ReorderMove {
    /// 1-based output position of the moved item.
    pub fn to(&self) -> usize {
        self.to
    }

    /// 1-based input position of the moved item.
    pub fn from(&self) -> usize {
        self.from
    }

    /// Description of the item this one lands before, when it is not the last
    /// item in the reordered output.
    pub fn before(&self) -> Option<&str> {
        self.before.as_deref()
    }

    /// Kind of the moved item.
    pub fn kind(&self) -> &ItemKind {
        &self.kind
    }

    /// Name of the moved item, when it has one.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Human-readable rendering of this move, e.g.
    /// `rearrange fn a_main from pos 2 to pos 1 (before b_helper)`.
    ///
    /// The trailing `(before C)` clause is omitted when the item is the last
    /// in the reordered output.
    pub fn message(&self) -> String {
        let subject = match &self.name {
            Some(name) => format!("{} {name}", self.kind),
            None => format!("{} at line {}", self.kind, self.line),
        };
        let mut out = format!(
            "rearrange {subject} from pos {} to pos {}",
            self.from, self.to
        );
        if let Some(before) = &self.before {
            out.push_str(&format!(" (before {before})"));
        }
        out
    }
}

impl fmt::Display for ReorderMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

/// Derive the list of moves between the input item order and `perm`.
///
/// Only items that move to an *earlier* output position (`to < from`) are
/// reported. Items that merely shift to a later position to fill the gap
/// are implied by the reported moves and omitted.
///
/// An already-ordered input yields an empty list. The returned records are in
/// reordered-output order, so `before` references the item that follows each
/// move in the new order.
///
/// # Arguments
///
/// - `items` - the parsed items in their original (input) order.
/// - `perm` - the validated [`Permutation`] mapping output position to input
///   item index.
pub fn compute_moves(items: &[SourceItem], perm: &Permutation) -> Vec<ReorderMove> {
    let mut moves = Vec::new();
    for (to_idx, &item_idx) in perm.order.iter().enumerate() {
        let to = to_idx + 1;
        let from = item_idx + 1;
        if from <= to {
            continue;
        }
        let item = &items[item_idx];
        let before = perm.order.get(to_idx + 1).map(|&next_idx| {
            let next = &items[next_idx];
            next.name()
                .map(Box::from)
                .unwrap_or_else(|| describe(next).into_boxed_str())
        });
        moves.push(ReorderMove {
            to,
            from,
            before,
            kind: *item.kind(),
            name: item.name().map(Box::from),
            line: item.start_line(),
        });
    }
    moves
}

/// Emit the reordered source by byte-slicing the original source.
///
/// Extracts each item's byte range from `parsed.source` and concatenates
/// them in the permutation order.
///
/// Items carrying a member permutation
/// ([`Permutation::set_member_order`]) emit their type body with the
/// members spliced in that order instead.
///
/// Because items are gap-anchored to the next item, a slice may begin with
/// carried leading trivia (blank lines and plain `//` section headers).
///
/// Leading and trailing whitespace are stripped ([`str::trim`]) so separators
/// do not pile up when items move, while the `//` header and `///`/`//!` doc
/// lines (non-whitespace) are preserved.
///
/// Inter-item spacing is then re-derived from the compact-group logic below.
/// The preamble and trailer are placed at the start and end.
///
/// Internally protected permutations instead retain original item/member slices,
/// adding a separator only when a moved EOF item has no trailing newline.
///
/// # Arguments
///
/// - `parsed` - the parsed source being reordered; its `source`, item
///   byte spans, members, `preamble_end`, and `trailer_start` drive the
///   output.
/// - `perm` - the validated [`Permutation`] mapping output position to input
///   item index.
///
/// # Errors
///
/// Returns an [`anyhow::Error`] if the permutation is malformed for the
/// parsed items:
/// - an item's member permutation length differs from that item's
///   member count.
///
/// # Panics
///
/// Panics if an item index is outside `parsed.items` or a member index is
/// outside that item's parsed members.
///
/// # Line endings
///
/// Item-terminator and blank-line separators use the source's dominant
/// line ending ([`dominant_line_ending`]), so an in-place reorder never
/// flips CRLF <-> LF.
pub fn emit(parsed: &ParseResult, perm: &Permutation) -> Result<String> {
    if perm.preserve_spacing {
        return Ok(emit_protected(parsed, perm));
    }
    let source = &parsed.source;
    let le = dominant_line_ending(source);
    let mut output = String::with_capacity(source.len());

    // Preamble: everything before the first item's start.
    output.push_str(&source[..parsed.preamble_end]);

    // Emit items in permutation order with canonical spacing:
    // - no blank line between consecutive `use` items
    // - no blank line between consecutive `mod` items
    // - no blank line between consecutive `const`/`static`/`extern` items
    // - blank line between everything else
    for (i, &idx) in perm.order.iter().enumerate() {
        let item = &parsed.items[idx];
        if let Some(member_order) = member_splice_order(perm, idx) {
            // Keep the type head/tail fixed and splice verbatim member spans
            // between them. Carried whitespace travels with each member.
            let members = item.members();
            // Re-check the member count against the parsed members so a
            // divergent count errors instead of indexing out of bounds.
            //
            // The permutation validated the member count it was built with.
            ensure!(
                member_order.len() == members.len(),
                "member permutation length {} does not match item {} member count {}",
                member_order.len(),
                idx,
                members.len()
            );
            let head = &source[item.start..members[0].start];
            let tail = &source[members[members.len() - 1].end..item.end];
            output.push_str(head.trim_start());
            for &member_idx in member_order {
                let member = &members[member_idx];
                output.push_str(&source[member.start..member.end]);
            }
            output.push_str(tail.trim_end());
        } else {
            output.push_str(source[item.start..item.end].trim());
        }
        output.push_str(le);

        if i + 1 < perm.order.len() {
            let next = &parsed.items[perm.order[i + 1]];
            let same_compact_group = matches!(
                (spacing_group(item.kind()), spacing_group(next.kind())),
                (Some(a), Some(b)) if a == b
            );
            if !same_compact_group {
                output.push_str(le);
            }
        }
    }

    // Trailer: everything after the last item's end.
    if parsed.trailer_start < source.len() {
        output.push_str(&source[parsed.trailer_start..]);
    }

    Ok(output)
}

/// An unambiguous description of a parsed item, used when an item has no name.
fn describe(item: &SourceItem) -> String {
    format!("{} at line {}", item.kind(), item.start_line())
}

/// Emit original item/member slices without changing protected trivia.
fn emit_protected(parsed: &ParseResult, perm: &Permutation) -> String {
    let source = &parsed.source;
    let mut output = String::with_capacity(source.len());
    output.push_str(&source[..parsed.preamble_end]);

    for (position, &index) in perm.order.iter().enumerate() {
        let item = &parsed.items[index];
        if let Some(order) = member_splice_order(perm, index) {
            let members = item.members();
            output.push_str(&source[item.start..members[0].start]);
            for &index in order {
                let member = &members[index];
                output.push_str(&source[member.start..member.end]);
            }
            output.push_str(&source[members[members.len() - 1].end..item.end]);
        } else {
            output.push_str(&source[item.start..item.end]);
        }

        // An EOF item may lack a newline. Keep its last token/comment separate
        // when it moves ahead of another item, without trimming either slice.
        if position + 1 < perm.order.len() && !output.ends_with('\n') {
            output.push_str(dominant_line_ending(source));
        }
    }
    output.push_str(&source[parsed.trailer_start..]);
    output
}

/// Keep the requested relative order only inside contiguous unpinned runs.
fn pin_runs(order: &mut [usize], pinned: impl Fn(usize) -> bool) {
    let mut rank = vec![0; order.len()];
    for (position, &index) in order.iter().enumerate() {
        rank[index] = position;
    }
    for (index, slot) in order.iter_mut().enumerate() {
        *slot = index;
    }

    let mut start = 0;
    for index in 0..order.len() {
        if pinned(index) {
            order[start..index].sort_unstable_by_key(|&item| rank[item]);
            start = index + 1;
        }
    }
    order[start..].sort_unstable_by_key(|&item| rank[item]);
}

/// Spacing group for items that should stay packed without a blank line.
///
/// Returns `None` for all other items, which always get a blank line after them.
fn spacing_group(kind: &ItemKind) -> Option<u8> {
    match kind {
        ItemKind::Use | ItemKind::Using => Some(0),
        ItemKind::Mod => Some(1),
        ItemKind::Const | ItemKind::Static | ItemKind::Extern => Some(2),
        _ => None,
    }
}

/// The member permutation to splice for item `idx`, or `None` when the item
/// emits its plain slice.
///
/// A plain slice means no attached member order, or an identity one, which
/// must keep the original bytes.
fn member_splice_order(perm: &Permutation, idx: usize) -> Option<&[usize]> {
    let order = perm.member_orders.get(&idx)?;
    let identity = order.iter().enumerate().all(|(pos, &member)| pos == member);
    (!identity).then_some(order.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::{LanguageBackend, RustBackend};
    use crate::rules::transform::reorder::graph::compute_member_order;
    use crate::rules::transform::reorder::graph::test_profiles::{
        CallersFirstProfile, MembersFirstProfile,
    };

    #[rstest::rstest]
    #[case::reversed(Range { start: 2, end: 1 })]
    #[case::outside_source(0..usize::MAX)]
    #[case::inside_character("fn caf".len() + 1.."fn café".len())]
    fn protect_should_reject_invalid_ranges(#[case] range: Range<usize>) {
        let parsed = RustBackend.parse("fn café() {}\n").unwrap();
        let mut permutation = Permutation::new(parsed.items.len(), vec![0]).unwrap();

        let result = permutation.protect(&parsed, &[range]);

        assert!(result.is_err());
    }

    #[test]
    fn protect_should_preserve_whole_type_and_all_members() {
        let source = "class Cache {\n    void B() {}\n    void A() {}\n}\n";
        let parsed = crate::languages::backend_for("cs")
            .unwrap()
            .parse(source)
            .unwrap();
        let mut permutation = Permutation::new(parsed.items.len(), vec![0]).unwrap();
        permutation.set_member_order(0, 2, vec![1, 0]).unwrap();

        permutation
            .protect(&parsed, core::slice::from_ref(&(0..source.len())))
            .unwrap();
        let output = emit(&parsed, &permutation).unwrap();

        assert_eq!(output, source);
        assert_eq!(permutation.member_order(0), Some([0, 1].as_slice()));
    }

    #[test]
    fn protect_should_separate_moved_eof_item_without_newline() {
        let source = "fn keep() {}\nfn a() {}\nfn b() {}";
        let parsed = RustBackend.parse(source).unwrap();
        let mut permutation = Permutation::new(parsed.items.len(), vec![0, 2, 1]).unwrap();

        permutation
            .protect(&parsed, core::slice::from_ref(&(0.."fn keep() {}".len())))
            .unwrap();
        let output = emit(&parsed, &permutation).unwrap();

        assert_eq!(output, "fn keep() {}\nfn b() {}\nfn a() {}\n");
    }

    #[test]
    fn protect_should_keep_impl_bytes_and_reorder_siblings_within_barriers() {
        let source = "fn a() {}\nfn b() {}\n/// Impl docs.\nimpl Cache {\n    /// Keep.\n    fn get() {}\n}\nfn c() {}\nfn d() {}\n";
        let parsed = RustBackend.parse(source).unwrap();
        let protected = crate::source::symbols::declarations(&parsed, "rs")
            .unwrap()
            .into_iter()
            .find(|item| item.path.as_ref() == "Cache::get")
            .unwrap()
            .bytes;
        let mut permutation = Permutation::new(parsed.items.len(), vec![4, 3, 2, 1, 0]).unwrap();

        permutation.protect(&parsed, &[protected]).unwrap();
        let output = emit(&parsed, &permutation).unwrap();

        assert_eq!(
            output,
            "fn b() {}\nfn a() {}\n/// Impl docs.\nimpl Cache {\n    /// Keep.\n    fn get() {}\n}\nfn d() {}\nfn c() {}\n"
        );
        assert!(
            compute_moves(&parsed.items, &permutation)
                .iter()
                .all(|item| item.kind() != &ItemKind::Impl)
        );
    }

    #[test]
    fn protect_should_pin_csharp_type_and_only_target_member() {
        let source = "class Before {}\nclass Cache {\n    void A() {}\n    void B() {}\n    /// Keep.\n    [Obsolete] void Keep() {}\n    void C() {}\n    void D() {}\n}\nclass After {}\n";
        let parsed = crate::languages::backend_for("cs")
            .unwrap()
            .parse(source)
            .unwrap();
        let protected = crate::source::symbols::declarations(&parsed, "cs")
            .unwrap()
            .into_iter()
            .find(|item| item.path.as_ref() == "Cache::Keep")
            .unwrap()
            .bytes;
        let mut permutation = Permutation::new(parsed.items.len(), vec![2, 1, 0]).unwrap();
        permutation
            .set_member_order(1, 5, vec![4, 3, 2, 1, 0])
            .unwrap();

        permutation.protect(&parsed, &[protected]).unwrap();
        let output = emit(&parsed, &permutation).unwrap();

        assert_eq!(
            output,
            "class Before {}\nclass Cache {\n    void B() {}\n    void A() {}\n    /// Keep.\n    [Obsolete] void Keep() {}\n    void D() {}\n    void C() {}\n}\nclass After {}\n"
        );
        assert!(compute_moves(&parsed.items, &permutation).is_empty());
    }

    #[test]
    fn protect_should_match_original_emission_when_ranges_empty() {
        let parsed = RustBackend.parse("fn a() {}\nfn b() {}\n").unwrap();
        let mut permutation = Permutation::new(parsed.items.len(), vec![1, 0]).unwrap();
        let original = emit(&parsed, &permutation).unwrap();

        permutation.protect(&parsed, &[]).unwrap();
        let protected = emit(&parsed, &permutation).unwrap();

        assert_eq!(protected, original);
    }

    /// Full reorder pipeline: parse, compute order, build permutation, emit.
    fn reorder(source: &str) -> String {
        let parsed = RustBackend.parse(source).unwrap();
        let order =
            crate::rules::transform::reorder::graph::compute_order(&parsed, &CallersFirstProfile)
                .unwrap();
        let perm = Permutation::new(parsed.items.len(), order).unwrap();
        emit(&parsed, &perm).unwrap()
    }

    #[test]
    fn emit_preserves_crlf_separators() {
        // CRLF source: every `\n` in the emitted output must be part of `\r\n`.
        let src = "fn b() { a(); }\r\nfn a() {}\r\n";
        let out = reorder(src);
        assert_eq!(
            out.matches('\n').count(),
            out.matches("\r\n").count(),
            "every newline must be CRLF after reorder: {out:?}"
        );
        // Caller (b) before callee (a).
        let b = out.find("fn b").unwrap();
        let a = out.find("fn a").unwrap();
        assert!(b < a, "b (caller) before a (callee)");
    }

    #[test]
    fn emit_preserves_lf_separators() {
        // LF source: no `\r` should appear in the emitted output.
        let src = "fn b() { a(); }\nfn a() {}\n";
        let out = reorder(src);
        assert!(!out.contains('\r'), "no CR in LF output: {out:?}");
        let b = out.find("fn b").unwrap();
        let a = out.find("fn a").unwrap();
        assert!(b < a, "b (caller) before a (callee)");
    }

    #[test]
    fn emit_crlf_preserves_doc_comment_endings() {
        // CRLF source with a doc-comment-pinned item: the doc lines keep their
        // `\r\n` (verbatim byte slices) and the separators use `\r\n`.
        let src = "fn b() { a(); }\r\n/// docs for a\r\nfn a() {}\r\n";
        let out = reorder(src);
        assert!(
            out.contains("/// docs for a\r\n"),
            "doc-comment line ending preserved: {out:?}"
        );
        assert_eq!(
            out.matches('\n').count(),
            out.matches("\r\n").count(),
            "every newline must be CRLF: {out:?}"
        );
    }

    /// Build the move list for a source via the full pipeline.
    fn moves(source: &str) -> Vec<ReorderMove> {
        let parsed = RustBackend.parse(source).unwrap();
        let order =
            crate::rules::transform::reorder::graph::compute_order(&parsed, &CallersFirstProfile)
                .unwrap();
        let perm = Permutation::new(parsed.items.len(), order).unwrap();
        compute_moves(&parsed.items, &perm)
    }

    #[test]
    fn compute_moves_reports_before_and_positions() {
        // a_main is a caller of b_helper, so reorder puts a_main first.
        let src = "fn b_helper() {}\nfn a_main() { b_helper(); }\n";
        let mv = moves(src);
        assert_eq!(mv.len(), 1, "only a_main moves: {mv:?}");
        let mv = &mv[0];
        assert_eq!(mv.from(), 2);
        assert_eq!(mv.to(), 1);
        assert_eq!(mv.name(), Some("a_main"));
        assert_eq!(mv.before(), Some("b_helper"));
        assert_eq!(
            mv.message(),
            "rearrange fn a_main from pos 2 to pos 1 (before b_helper)"
        );
        assert_eq!(mv.to_string(), mv.message());
    }

    #[test]
    fn compute_moves_is_empty_for_already_ordered_input() {
        // Caller (b_main) already precedes its callee (a_helper).
        let src = "fn b_main() { a_helper(); }\nfn a_helper() {}\n";
        assert!(moves(src).is_empty());
    }

    #[test]
    fn compute_moves_is_empty_with_no_moves_on_single_item() {
        let src = "fn only() {}\n";
        assert!(moves(src).is_empty());
    }

    /// One synthetic item: span, line, kind, and name, with every other
    /// field defaulted.
    fn item(
        start: usize,
        end: usize,
        line: usize,
        kind: ItemKind,
        name: Option<&str>,
    ) -> crate::source::SourceItem {
        crate::source::SourceItem::new(
            start,
            end,
            line,
            kind,
            name.map(String::from),
            None,
            false,
            false,
            false,
            None,
            Vec::new(),
            false,
            Vec::new(),
            false,
        )
    }

    /// An unnamed moved item reports itself by kind and start line.
    #[test]
    fn compute_moves_describes_unnamed_item_by_kind_and_line() {
        // Output order struct, impl, fn: the unnamed impl moves from pos
        // 3 to pos 2, landing before the fn that preceded it.
        let items = vec![
            item(0, 30, 1, ItemKind::Fn, Some("uses_impl")),
            item(30, 60, 2, ItemKind::Struct, Some("Foo")),
            item(60, 90, 3, ItemKind::Impl, None),
        ];
        let perm = Permutation::new(3, vec![1, 2, 0]).unwrap();

        let mv = compute_moves(&items, &perm);

        let unnamed = mv
            .iter()
            .find(|m| m.kind() == &ItemKind::Impl)
            .expect("an impl block should move");
        assert_eq!(unnamed.name(), None);
        assert!(unnamed.message().contains("impl at line 3"), "{}", unnamed);
    }

    /// Member reordering round-trips: the type body splices members in the
    /// profile order and every line survives.
    ///
    /// The spliced order is idempotent (recomputing yields the identity
    /// member permutation).
    #[test]
    fn member_reorder_round_trips_through_emit() {
        // One type item whose body holds three members: method Z (calls A),
        // field F, method A. Members tile the body back-to-back.
        //
        // The item is built directly because the member machinery is language-agnostic;
        // the tree only accompanies the parse result.
        let method_z = "    void Z() { A(); }\n";
        let field_f = "    int F;\n";
        let method_a = "    void A() { }\n";
        let head = "class C\n{\n";
        let tail = "}\n";
        let source = format!("{head}{method_z}{field_f}{method_a}{tail}");

        let parsed = RustBackend.parse(&source).unwrap();
        let z = source.find(method_z).unwrap();
        let f = source.find(field_f).unwrap();
        let a = source.find(method_a).unwrap();
        let members = vec![
            crate::source::TypeMember::new(
                z,
                z + method_z.len(),
                0,
                ItemKind::Fn,
                Some("Z".into()),
            ),
            crate::source::TypeMember::new(
                f,
                f + field_f.len(),
                0,
                ItemKind::Const,
                Some("F".into()),
            ),
            crate::source::TypeMember::new(
                a,
                a + method_a.len(),
                0,
                ItemKind::Fn,
                Some("A".into()),
            ),
        ];
        // Z (member 0) calls A (member 2).
        let edges = vec![(0usize, 2usize)];

        let member_order = compute_member_order(&members, &edges, &MembersFirstProfile);
        assert_eq!(member_order, vec![1, 0, 2], "field first, then Z before A");

        let item = crate::source::SourceItem::new(
            0,
            source.len(),
            1,
            ItemKind::Class,
            Some("C".into()),
            None,
            false,
            false,
            false,
            None,
            Vec::new(),
            false,
            Vec::new(),
            false,
        )
        .with_members(members.clone());
        let tree = parsed.syntax_tree().clone();
        let membered = ParseResult::new(vec![item], source.clone(), tree, 0, source.len());

        let mut perm = Permutation::new(1, vec![0]).unwrap();
        perm.set_member_order(0, 3, member_order).unwrap();
        let output = emit(&membered, &perm).unwrap();

        assert_eq!(
            output,
            format!("{head}{field_f}{method_z}{method_a}{tail}"),
            "members splice in profile order"
        );
        assert!(
            crate::source::preservation::verify_line_preservation(&source, &output).is_ok(),
            "every non-blank line survives the splice"
        );
        // Idempotence: members parsed back from the output (F, Z, A in
        // source order) yield the identity permutation.
        let reordered = vec![members[1].clone(), members[0].clone(), members[2].clone()];
        let re_edges = vec![(1usize, 2usize)]; // Z now at position 1 calls A at 2.
        assert_eq!(
            compute_member_order(&reordered, &re_edges, &MembersFirstProfile),
            vec![0, 1, 2],
            "second run computes the identity order"
        );
    }

    /// An identity member permutation leaves the item bytes exactly as the
    /// plain slice path emits them.
    #[test]
    fn identity_member_order_keeps_plain_item_bytes() {
        let source = "struct S\n{\n    int F;\n}\n";
        let parsed = RustBackend.parse(source).unwrap();
        let f = source.find("    int F;\n").unwrap();
        let members = vec![crate::source::TypeMember::new(
            f,
            f + "    int F;\n".len(),
            0,
            ItemKind::Const,
            Some("F".into()),
        )];
        let item = parsed.items[0].clone().with_members(members);

        let tree = parsed.syntax_tree().clone();
        let membered = ParseResult::new(
            vec![item],
            source.to_string(),
            tree,
            parsed.preamble_end,
            parsed.trailer_start,
        );

        let mut perm = Permutation::new(1, vec![0]).unwrap();
        perm.set_member_order(0, 1, vec![0]).unwrap();
        let with_identity = emit(&membered, &perm).unwrap();

        let plain = {
            let perm = Permutation::new(1, vec![0]).unwrap();
            emit(&membered, &perm).unwrap()
        };

        assert_eq!(with_identity, plain);
    }

    /// A member permutation whose length diverges from the item's parsed
    /// member count errors at emit time instead of panicking on the first
    /// member slice.
    #[test]
    fn emit_errors_on_divergent_member_order_length() {
        // One item with no members, built through the real parser; the
        // permutation then carries a two-member order for it (non-identity
        // so the splice path runs).
        let source = "struct S\n{\n    int F;\n}\n";
        let parsed = RustBackend.parse(source).unwrap();
        let item = parsed.items[0].clone().with_members(Vec::new());
        let tree = parsed.syntax_tree().clone();
        let membered = ParseResult::new(
            vec![item],
            source.to_string(),
            tree,
            parsed.preamble_end,
            parsed.trailer_start,
        );

        let mut perm = Permutation::new(1, vec![0]).unwrap();
        perm.set_member_order(0, 2, vec![1, 0]).unwrap();

        let error =
            emit(&membered, &perm).expect_err("a divergent member count must error, not panic");
        assert!(
            error
                .to_string()
                .contains("member permutation length 2 does not match item 0 member count 0"),
            "unexpected error: {error}"
        );
    }
}
