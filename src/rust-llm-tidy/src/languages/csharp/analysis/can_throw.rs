//! Owned C# can-throw graphs with type-qualified member keys.
//!
//! Simple-name groups preserve conservative same-file matches.
//! Qualified calls connect members across the supplied parses.

use super::declaration::{Declaration, THROWING, collect_children};
use crate::languages::csharp::parse::{
    call_target_name, qualified_call_target, receiver_value_names,
};
use crate::source::ParseResult;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// Shared throw facts and declaration answers for supplied C# source files.
#[derive(Clone, Default)]
pub struct CanThrowIndex {
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
    /// Parsed tree identities already represented by this graph.
    trees: HashMap<usize, tree_sitter::Tree>,
}

impl CanThrowIndex {
    /// Build a shared graph from C# `parses`, ignoring trees with syntax errors.
    ///
    /// Returns cycle-safe throw answers across the supplied files.
    ///
    /// # Arguments
    ///
    /// - `parses` - the C# parse results to index; parses with syntax errors
    ///   are skipped.
    pub fn from_parses<'a>(parses: impl IntoIterator<Item = &'a ParseResult>) -> Self {
        let mut merged = Self::default();
        for parsed in parses {
            if parsed.syntax_tree().root_node().has_error() {
                continue;
            }
            let mut declarations = Vec::with_capacity(parsed.items.len());
            collect_children(
                parsed.syntax_tree().root_node(),
                &parsed.source,
                None,
                &mut declarations,
            );
            merged.merge(Self::from_declarations(&declarations));
            merged.trees.insert(
                parsed.syntax_tree().root_node().id(),
                parsed.syntax_tree().clone(),
            );
        }

        merged.connect_qualified_calls();
        merged.propagate();
        merged
    }

    /// Compose a separately supplied `parsed` file with shared facts when absent.
    /// Existing tree identities borrow the already-computed graph without copying it.
    pub(crate) fn including<'a>(&'a self, parsed: &ParseResult) -> Cow<'a, Self> {
        if self
            .trees
            .contains_key(&parsed.syntax_tree().root_node().id())
        {
            return Cow::Borrowed(self);
        }
        let mut declarations = Vec::with_capacity(parsed.items.len());
        collect_children(
            parsed.syntax_tree().root_node(),
            &parsed.source,
            None,
            &mut declarations,
        );
        let local = Self::from_declarations(&declarations);

        let mut merged = self.clone();
        merged.merge(local);
        merged.connect_qualified_calls();
        merged.propagate();
        Cow::Owned(merged)
    }

    /// Add `local` vertices and collision edges without copying its owned facts.
    fn merge(&mut self, local: Self) {
        let merged = self;
        let offset = merged.can_throw.len();
        merged.can_throw.extend(local.can_throw);
        merged.callers.resize_with(merged.can_throw.len(), Vec::new);
        merged
            .qualified_calls
            .resize_with(merged.can_throw.len(), Vec::new);

        // Connect collisions instead of copying each overload's incoming edges.
        for (key, position) in local.members {
            let target = offset + position;
            if let Some(&existing) = merged.members.get(&key) {
                merged.callers[existing].push(target);
                merged.callers[target].push(existing);
            } else {
                merged.members.insert(key, target);
            }
        }
        for (position, callers) in local.callers.into_iter().enumerate() {
            merged.callers[offset + position].extend(callers.into_iter().map(|i| offset + i));
        }
        for (position, calls) in local.qualified_calls.into_iter().enumerate() {
            merged.qualified_calls[offset + position] = calls;
        }
    }

    /// Resolve retained qualified targets against all members currently present.
    fn connect_qualified_calls(&mut self) {
        for (caller, targets) in self.qualified_calls.iter().enumerate() {
            for target in targets {
                if let Some(&callee) = self.members.get(target) {
                    self.callers[callee].push(caller);
                }
            }
        }
    }

    /// Return whether `owner` and `member` identify a throwing indexed declaration.
    pub(crate) fn member_can_throw(&self, owner: &str, member: &str) -> bool {
        self.members
            .get(&format!("{owner}.{member}"))
            .is_some_and(|&i| self.can_throw[i])
    }

    /// Build owned facts from `declarations` using same-file edges only.
    pub(crate) fn from_declarations(declarations: &[Declaration<'_>]) -> Self {
        let count = declarations.len();
        let mut index = Self {
            members: HashMap::with_capacity(count),
            callers: Vec::with_capacity(count),
            qualified_calls: Vec::with_capacity(count),
            can_throw: Vec::with_capacity(count),
            declarations: Vec::with_capacity(count),
            trees: HashMap::new(),
        };
        let mut names: HashMap<&str, Vec<usize>> = HashMap::with_capacity(count);

        for decl in declarations {
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
                        &values,
                        &mut cursor,
                    );
                }
            }
        }

        index.propagate();
        index
    }

    /// Propagate new throw evidence through reverse edges until every caller agrees.
    fn propagate(&mut self) {
        let mut work = Vec::with_capacity(self.can_throw.len());
        work.extend(
            self.can_throw
                .iter()
                .enumerate()
                .filter_map(|(i, &throws)| throws.then_some(i)),
        );
        while let Some(callee) = work.pop() {
            for &caller in &self.callers[callee] {
                if !self.can_throw[caller] {
                    self.can_throw[caller] = true;
                    work.push(caller);
                }
            }
        }
    }

    /// Return the throw answer for `ordinal` in the original declaration order.
    pub(crate) fn declaration_can_throw(&self, ordinal: usize) -> bool {
        self.can_throw[self.declarations[ordinal]]
    }

    /// Scan `decl` with a reused cursor, excluding nested callable bodies.
    ///
    /// - `caller` addresses its graph position.
    /// - `names` bounds same-file matches.
    /// - `values` rejects declared value receivers before retaining qualified
    ///   candidates.
    fn scan_member<'a>(
        &mut self,
        decl: &Declaration<'a>,
        caller: usize,
        names: &HashMap<&str, Vec<usize>>,
        values: &HashSet<&str>,
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
                        None,
                        values,
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
            let parsed = crate::languages::csharp::parse::parse(
                "namespace N { class Outer { class Inner<T> { void Helper() { throw new E(); } } void Helper() {} public void Caller() { obj.Helper(); } } }",
            ).expect("fixture parses");
            let mut declarations = Vec::new();
            super::collect_children(
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
        assert_eq!(
            index.qualified_calls[index.members["Outer.Caller"]],
            ["obj.Helper"]
        );
        assert!(!index.members.contains_key("obj.Helper"));
        assert!(!index.members.contains_key("N.Outer.Inner.Helper"));
    }
}
