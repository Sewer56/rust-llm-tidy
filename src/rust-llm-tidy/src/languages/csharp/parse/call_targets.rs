//! Borrowed call-target extraction without namespace resolution or receiver typing.

use std::collections::HashSet;

/// Qualify an extracted call `target` and simple `member` in `source`.
///
/// - `caller_type`: owner of bare and `this` calls
/// - `known_types`: eligible explicit receivers; `None` keeps unresolved ones
/// - `value_names`: declared values that suppress explicit receiver matches
/// - `constructor`: whether the target names a constructed type
///
/// Declared value receivers return `None`; supplied type sets reject
/// unknown types.
pub(crate) fn qualified_call_target<'a>(
    target: tree_sitter::Node<'_>,
    member: &'a str,
    constructor: bool,
    source: &'a str,
    caller_type: Option<&'a str>,
    known_types: Option<&HashSet<&str>>,
    value_names: &HashSet<&str>,
) -> Option<(&'a str, &'a str)> {
    if constructor {
        return Some((member, member));
    }
    if member == "nameof" {
        return None;
    }

    let owner = match target.kind() {
        "identifier" | "generic_name" => caller_type?,
        "member_access_expression" => {
            let receiver = target.child_by_field_name("expression")?;
            let text = receiver.utf8_text(source.as_bytes()).ok()?;
            if text == "this" {
                caller_type?
            } else {
                if !matches!(receiver.kind(), "identifier" | "generic_name") {
                    return None;
                }
                let name = call_target_name(receiver, source)?;
                if known_types.is_some_and(|types| !types.contains(name))
                    || value_names.contains(name)
                {
                    return None;
                }
                name
            }
        }
        _ => return None,
    };

    Some((owner, member))
}

/// Collect value names under `scope` in `source` to reject ambiguous receivers.
pub(crate) fn receiver_value_names<'a>(
    scope: tree_sitter::Node<'_>,
    source: &'a str,
) -> HashSet<&'a str> {
    let mut names = HashSet::new();
    let mut cursor = scope.walk();
    loop {
        let node = cursor.node();
        if matches!(
            node.kind(),
            "parameter"
                | "variable_declarator"
                | "property_declaration"
                | "foreach_statement"
                | "catch_declaration"
        ) && let Some(name) = node.child_by_field_name("name")
            && let Ok(name) = name.utf8_text(source.as_bytes())
        {
            names.insert(name);
        }

        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return names;
            }
        }
    }
}

/// Return `target`'s rightmost simple name in `source` without generic arguments.
pub(crate) fn call_target_name<'a>(
    target: tree_sitter::Node<'_>,
    source: &'a str,
) -> Option<&'a str> {
    let mut node = target;
    loop {
        if node.kind() == "identifier" {
            return node.utf8_text(source.as_bytes()).ok();
        }
        node = node
            .child_by_field_name("name")
            .or_else(|| node.named_child(0))?;
    }
}

#[cfg(test)]
mod tests {
    use super::{call_target_name, qualified_call_target, receiver_value_names};

    /// Call syntax selects qualified keys.
    /// Unknown and shadowed receivers stay unresolved.
    #[test]
    fn qualified_targets_should_reject_unknown_receivers() {
        let cases = [
            ("bare", "Helper()", Some(("C", "Helper"))),
            ("this", "this.Helper()", Some(("C", "Helper"))),
            ("static", "T.Helper()", Some(("T", "Helper"))),
            ("unknown", "obj.Helper()", None),
            ("generic_method", "Helper<int>()", Some(("C", "Helper"))),
            ("generic_type", "T<int>.Helper()", Some(("T", "Helper"))),
            ("constructor", "new T()", Some(("T", "T"))),
            ("generic_constructor", "new T<int>()", Some(("T", "T"))),
            ("shadowed", "Shadow.Helper()", None),
            ("name_reference", "nameof(Helper)", None),
        ];
        let types = ["C", "T", "Shadow"].into_iter().collect();

        for (label, expression, expected) in cases {
            let source = format!("class C {{ void M(object Shadow) {{ {expression}; }} }}");
            let parsed = super::super::parse(&source).expect("fixture parses");
            let root = parsed.syntax_tree().root_node();
            assert!(!root.has_error(), "{label}");
            let values = receiver_value_names(root, &source);
            let mut cursor = root.walk();
            let call = loop {
                let node = cursor.node();
                if matches!(
                    node.kind(),
                    "invocation_expression" | "object_creation_expression"
                ) {
                    break node;
                }
                if cursor.goto_first_child() {
                    continue;
                }
                while !cursor.goto_next_sibling() {
                    assert!(cursor.goto_parent(), "call exists: {label}");
                }
            };

            let constructor = call.kind() == "object_creation_expression";
            let target = call
                .child_by_field_name(if constructor { "type" } else { "function" })
                .unwrap();
            let member = call_target_name(target, &source).unwrap();

            let actual = qualified_call_target(
                target,
                member,
                constructor,
                &source,
                Some("C"),
                Some(&types),
                &values,
            );

            assert_eq!(actual, expected, "{label}");
        }
    }
}
