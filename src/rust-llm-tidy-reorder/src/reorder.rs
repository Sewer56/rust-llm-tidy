// Partially vendored from rust-reorder (MIT).
// Modified based on https://github.com/umwelt-ai/rust-reorder.
// Provides permutation validation and byte-slice emit.

use ahash::AHashMap;
use anyhow::{Result, ensure};
use rust_llm_tidy_model::line_endings::dominant_line_ending;
use rust_llm_tidy_model::parse::{ItemKind, ParseResult};

/// A validated permutation of items.
///
/// Wraps a `Vec<usize>` that maps output position → input item index.
/// Every index in `0..n` appears exactly once. Items with in-type member
/// reordering additionally carry a member permutation
/// ([`Permutation::set_member_order`]).
#[derive(Debug, Clone)]
pub struct Permutation {
    order: Vec<usize>,
    /// In-type member permutations by item index. Empty unless member
    /// reordering applies; the Rust parse emits no members.
    member_orders: AHashMap<usize, Vec<usize>>,
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

impl std::fmt::Display for ReorderMove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// Derive the list of moves between the input item order and `perm`.
///
/// Only items that move to an *earlier* output position (`to < from`) are
/// reported; items that merely shift to a later position to fill the gap are
/// implied by the reported moves and omitted.
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
pub fn compute_moves(
    items: &[rust_llm_tidy_model::parse::SourceItem],
    perm: &Permutation,
) -> Vec<ReorderMove> {
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
/// parsed items (e.g. an index out of range, which surfaces as a
/// slice-out-of-bounds failure).
///
/// # Line endings
///
/// Item-terminator and blank-line separators use the source's dominant
/// line ending ([`dominant_line_ending`]), so an in-place reorder never
/// flips CRLF <-> LF.
pub fn emit(parsed: &ParseResult, perm: &Permutation) -> Result<String> {
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
            // Splice the type body: the head up to the first member and the
            // tail after the last member stay fixed around the reordered
            // member spans, which tile the body back-to-back.
            //
            // Member slices are verbatim, so carried whitespace travels
            // with each member.
            let members = item.members();
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
fn describe(item: &rust_llm_tidy_model::parse::SourceItem) -> String {
    format!("{} at line {}", item.kind(), item.start_line())
}

/// The member permutation to splice for item `idx`, or `None` when the item
/// emits its plain slice (no attached member order, or an identity one,
/// which must keep the original bytes).
fn member_splice_order(perm: &Permutation, idx: usize) -> Option<&[usize]> {
    let order = perm.member_orders.get(&idx)?;
    let identity = order.iter().enumerate().all(|(pos, &member)| pos == member);
    (!identity).then_some(order.as_slice())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{MembersFirstProfile, RustProfile, compute_member_order};
    use rust_llm_tidy_model::parse::parse_source;

    /// Full reorder pipeline: parse, compute order, build permutation, emit.
    fn reorder(source: &str) -> String {
        let parsed = parse_source(source).unwrap();
        let order = crate::graph::compute_order(&parsed, &RustProfile).unwrap();
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
        let parsed = parse_source(source).unwrap();
        let order = crate::graph::compute_order(&parsed, &RustProfile).unwrap();
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

    #[test]
    fn compute_moves_describes_unnamed_item_by_kind_and_line() {
        // Reorder moves the struct's impl before the fn that precedes it, so
        // the unnamed impl block reports via kind + line.
        let src = "fn uses_impl() { let _ = Foo {}; }\nstruct Foo {}\nimpl Foo {}\n";
        let mv = moves(src);
        let unnamed = mv
            .iter()
            .find(|m| m.kind() == &rust_llm_tidy_model::parse::ItemKind::Impl);
        let unnamed = unnamed.expect("an impl block should move");
        assert_eq!(unnamed.name(), None);
        assert!(unnamed.message().contains("impl at line 3"), "{}", unnamed);
    }

    /// Member reordering round-trips: the type body splices members in the
    /// profile order, every line survives, and the spliced order is
    /// idempotent (recomputing yields the identity member permutation).
    #[test]
    fn member_reorder_round_trips_through_emit() {
        // One type item whose body holds three members: method Z (calls A),
        // field F, method A. Members tile the body back-to-back. The item is
        // built directly because the member machinery is language-agnostic;
        // the tree only accompanies the parse result.
        let method_z = "    void Z() { A(); }\n";
        let field_f = "    int F;\n";
        let method_a = "    void A() { }\n";
        let head = "class C\n{\n";
        let tail = "}\n";
        let source = format!("{head}{method_z}{field_f}{method_a}{tail}");

        let parsed = parse_source(&source).unwrap();
        let z = source.find(method_z).unwrap();
        let f = source.find(field_f).unwrap();
        let a = source.find(method_a).unwrap();
        let members = vec![
            rust_llm_tidy_model::parse::TypeMember::new(
                z,
                z + method_z.len(),
                0,
                ItemKind::Fn,
                Some("Z".into()),
            ),
            rust_llm_tidy_model::parse::TypeMember::new(
                f,
                f + field_f.len(),
                0,
                ItemKind::Const,
                Some("F".into()),
            ),
            rust_llm_tidy_model::parse::TypeMember::new(
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

        let item = rust_llm_tidy_model::parse::SourceItem::new(
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
            rust_llm_tidy_model::safety::verify_line_preservation(&source, &output).is_ok(),
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
        let parsed = parse_source(source).unwrap();
        let f = source.find("    int F;\n").unwrap();
        let members = vec![rust_llm_tidy_model::parse::TypeMember::new(
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
}
