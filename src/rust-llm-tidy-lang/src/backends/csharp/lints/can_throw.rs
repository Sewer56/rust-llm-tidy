//! Owned can-throw facts for one C# file, with type-qualified member keys.
//!
//! Simple-name groups preserve conservative same-file matches.
//! Qualified call facts do not participate in propagation across files.

use super::super::parse::{call_target_name, qualified_call_target, receiver_value_names};
use super::{Declaration, THROWING};
use std::collections::{HashMap, HashSet};

/// A file's owned graph and declaration answers, independent of its syntax tree.
pub(super) struct CanThrowIndex {
    /// Qualified keys map namespace and nesting collisions to one position.
    members: HashMap<String, usize>,
    /// Reverse same-file edges, indexed by graph position.
    callers: Vec<Vec<usize>>,
    /// Qualified targets retained separately from same-file propagation edges.
    qualified_calls: Vec<Vec<String>>,
    /// Throw evidence after the cycle-safe worklist reaches its fixpoint.
    can_throw: Vec<bool>,
    /// Each declaration's graph position, including nameless declarations.
    declarations: Vec<usize>,
}

impl CanThrowIndex {
    /// Build owned facts from `declarations` using same-file edges only.
    pub(super) fn from_declarations(declarations: &[Declaration<'_>]) -> Self {
        let count = declarations.len();
        let mut index = Self {
            members: HashMap::with_capacity(count),
            callers: Vec::with_capacity(count),
            qualified_calls: Vec::with_capacity(count),
            can_throw: Vec::with_capacity(count),
            declarations: Vec::with_capacity(count),
        };
        let mut names: HashMap<&str, Vec<usize>> = HashMap::with_capacity(count);
        let mut types = HashSet::with_capacity(count);

        for decl in declarations {
            if let Some(type_name) = decl.type_name.as_deref() {
                types.insert(type_name);
            }
            let position = if THROWING.contains(&decl.kind)
                && let (Some(owner), Some(name)) = (&decl.type_name, &decl.name)
            {
                let next = index.can_throw.len();
                *index
                    .members
                    .entry(format!("{owner}.{name}"))
                    .or_insert(next)
            } else {
                index.can_throw.len()
            };
            if position == index.can_throw.len() {
                index.can_throw.push(false);
                index.callers.push(Vec::new());
                index.qualified_calls.push(Vec::new());
            }
            index.declarations.push(position);
            if THROWING.contains(&decl.kind)
                && let Some(name) = decl.name.as_deref()
            {
                names.entry(name).or_default().push(position);
            }
        }

        // A star connects equal names without expanding calls into every overload.
        for positions in names.values() {
            if let Some((&first, rest)) = positions.split_first() {
                for &other in rest {
                    index.callers[first].push(other);
                    index.callers[other].push(first);
                }
            }
        }
        if let Some(first) = declarations.first() {
            let mut root = first.node;
            while let Some(parent) = root.parent() {
                root = parent;
            }
            let values = receiver_value_names(root, first.source);
            let mut cursor = first.node.walk();
            for (ordinal, decl) in declarations.iter().enumerate() {
                if THROWING.contains(&decl.kind) {
                    index.scan_member(
                        decl,
                        index.declarations[ordinal],
                        &names,
                        (&types, &values),
                        &mut cursor,
                    );
                }
            }
        }

        let mut work = Vec::with_capacity(index.can_throw.len());
        work.extend(
            index
                .can_throw
                .iter()
                .enumerate()
                .filter_map(|(i, &throws)| throws.then_some(i)),
        );
        while let Some(callee) = work.pop() {
            for &caller in &index.callers[callee] {
                if !index.can_throw[caller] {
                    index.can_throw[caller] = true;
                    work.push(caller);
                }
            }
        }

        index
    }

    /// Return the throw answer for `ordinal` in the original declaration order.
    pub(super) fn declaration_can_throw(&self, ordinal: usize) -> bool {
        self.can_throw[self.declarations[ordinal]]
    }

    /// Scan `decl` with a reused cursor, excluding nested callable bodies.
    /// `caller` addresses its graph position; `names` bounds same-file matches.
    /// `receivers` pairs known type names with value names to reject.
    fn scan_member<'a>(
        &mut self,
        decl: &Declaration<'a>,
        caller: usize,
        names: &HashMap<&str, Vec<usize>>,
        receivers: (&HashSet<&str>, &HashSet<&str>),
        cursor: &mut tree_sitter::TreeCursor<'a>,
    ) {
        cursor.reset(decl.node);
        'walk: loop {
            let node = cursor.node();
            let nested = matches!(
                node.kind(),
                "lambda_expression" | "anonymous_method_expression" | "local_function_statement"
            );
            if !nested {
                if node.kind() == "throw_statement" {
                    self.can_throw[caller] = true;
                } else if let Some(field) = match node.kind() {
                    "invocation_expression" => Some("function"),
                    "object_creation_expression" => Some("type"),
                    _ => None,
                } && let Some(target) = node.child_by_field_name(field)
                    && let Some(name) = call_target_name(target, decl.source)
                    && name != "nameof"
                    && decl.name.is_some()
                {
                    if let Some(positions) = names.get(name) {
                        self.callers[positions[0]].push(caller);
                    }
                    if let Some((owner, member)) = qualified_call_target(
                        target,
                        name,
                        field == "type",
                        decl.source,
                        decl.type_name.as_deref(),
                        receivers.0,
                        receivers.1,
                    ) {
                        self.qualified_calls[caller].push(format!("{owner}.{member}"));
                    }
                }

                if cursor.goto_first_child() {
                    continue 'walk;
                }
            }

            loop {
                if cursor.goto_next_sibling() {
                    continue 'walk;
                }
                if !cursor.goto_parent() || cursor.node() == decl.node {
                    break 'walk;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CanThrowIndex;

    /// Owned qualified keys retain local throw propagation after the parse drops.
    #[test]
    fn index_should_preserve_simple_name_matches_and_innermost_type_keys() {
        let index = {
            let parsed = super::super::super::parse::parse(
                "namespace N { class Outer { class Inner<T> { void Helper() { throw new E(); } } void Helper() {} public void Caller() { obj.Helper(); } } }",
            ).expect("fixture parses");
            let mut declarations = Vec::new();
            super::super::collect_children(
                parsed.syntax_tree().root_node(),
                &parsed.source,
                None,
                &mut declarations,
            );

            CanThrowIndex::from_declarations(&declarations)
        };

        assert!(index.can_throw[index.members["Inner.Helper"]]);
        assert!(index.can_throw[index.members["Outer.Helper"]]);
        assert!(index.can_throw[index.members["Outer.Caller"]]);
        assert!(index.qualified_calls[index.members["Outer.Caller"]].is_empty());
        assert!(!index.members.contains_key("N.Outer.Inner.Helper"));
    }
}
