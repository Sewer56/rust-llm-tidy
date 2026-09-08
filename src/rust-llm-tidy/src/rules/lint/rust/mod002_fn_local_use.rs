//! Keep imports at module scope so dependencies are easy to find.
//!
//! Keep an import function-local only if it needs conditional compilation.
//!
//! `MOD002` ([`check`]) fires on every `use` declaration lexically inside a
//! function body (nested blocks, closures, and inner functions
//! included).
//!
//! A `use` is exempt only when the `use` item itself carries a
//! `#[cfg]` attribute; enclosing `#[cfg]` never exempts it.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::lint::CODE_MOD002;
use crate::source::ParseResult;

/// Keep dependencies easy to find by flagging local imports without `#[cfg]`.
///
/// Walks the retained tree-sitter tree once. Every `use_declaration`
/// inside a function item without a `cfg` attribute on the `use`
/// itself yields one error diagnostic. Nothing re-parses.
///
/// No exemptions: test functions and `#[cfg(test)]` functions are
/// checked like any other.
///
/// # Arguments
///
/// - `parsed` - the parsed source result whose tree is walked.
pub(super) fn check(parsed: &ParseResult) -> Vec<Diagnostic> {
    let source = parsed.source.as_str();
    let mut diags = Vec::new();
    walk_fn_local_uses(parsed.syntax_tree().root_node(), source, &mut diags);
    diags
}

/// Depth-first cursor walk over `root`, flagging fn-local `use` items.
///
/// `fn_depth` counts enclosing `function_item` nodes on one reused
/// cursor. A `use` anywhere in a function's subtree (body, nested
/// blocks, closures, inner functions) is fn-local; module-level items
/// never are.
fn walk_fn_local_uses<'a>(root: tree_sitter::Node<'a>, source: &'a str, out: &mut Vec<Diagnostic>) {
    let mut cursor = root.walk();
    let mut fn_depth = 0usize;
    'walk: loop {
        let node = cursor.node();
        match node.kind() {
            "function_item" => fn_depth += 1,
            "use_declaration" if fn_depth > 0 => check_use(node, source, out),
            _ => {}
        }
        if cursor.goto_first_child() {
            continue 'walk;
        }
        // Leave the current node exactly once: each climb drops the
        // function-depth contribution of the node being left.
        loop {
            let leaving = cursor.node();
            if leaving.kind() == "function_item" {
                fn_depth -= 1;
            }
            if cursor.goto_next_sibling() {
                continue 'walk;
            }
            if !cursor.goto_parent() || cursor.node() == root {
                return;
            }
        }
    }
}

/// Flags `use` unless it carries a `cfg` attribute on itself.
///
/// `#[cfg_attr(...)]` and other attributes do not count; only an
/// attribute whose first path segment is exactly `cfg` guards the
/// `use`.
fn check_use<'a>(node: tree_sitter::Node<'a>, source: &str, out: &mut Vec<Diagnostic>) {
    if has_cfg_attribute(node, source) {
        return;
    }

    out.push(Diagnostic {
        severity: Severity::Error,
        code: CODE_MOD002,
        message: indoc::indoc! {"
            function-local `use` lacks its own `#[cfg]`.
            - Hoist it to module scope so dependencies are easy to find.
            - Keep it local only if it needs conditional compilation, with `#[cfg]` on the `use`."}
        .to_string(),
        line: node.start_position().row + 1,
        item_kind: "use".to_string(),
        item_name: None,
    });
}

/// True when `node`'s contiguous preceding attributes include a `#[cfg(...)]`.
///
/// The grammar attaches outer attributes as previous siblings of the
/// item, so the scan walks the named previous siblings while they are
/// `attribute_item`s. Only a plain `cfg` path guards: `cfg_attr` and
/// `foo::cfg` do not.
fn has_cfg_attribute(node: tree_sitter::Node<'_>, source: &str) -> bool {
    let mut prev = node.prev_named_sibling();
    while let Some(item) = prev {
        if item.kind() != "attribute_item" {
            return false;
        }
        let attr = (0..item.named_child_count() as u32)
            .find_map(|i| {
                let child = item.named_child(i).expect("index below count");
                (child.kind() == "attribute").then_some(child)
            })
            .expect("an attribute_item always holds one attribute");
        if attr_first_segment_is_cfg(attr, source) {
            return true;
        }
        prev = item.prev_named_sibling();
    }
    false
}

/// True when the attribute's first path segment is exactly `cfg`.
///
/// `cfg_attr` and scoped paths like `foo::cfg` do not match: only a
/// plain `#[cfg(...)]` guards the item.
fn attr_first_segment_is_cfg(attr: tree_sitter::Node<'_>, source: &str) -> bool {
    (0..attr.named_child_count() as u32).any(|i| {
        let path = attr.named_child(i).expect("index below count");
        if !matches!(path.kind(), "identifier" | "scoped_identifier") {
            return false;
        }
        let first = path.child_by_field_name("path").unwrap_or(path);
        first.utf8_text(source.as_bytes()).is_ok_and(|t| t == "cfg")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::rust::parse::parse_source;

    /// Parses `source` and runs MOD002 over it.
    fn checks(source: &str) -> Vec<Diagnostic> {
        check(&parse_source(source).unwrap())
    }

    // ── MOD002: fn-local use ──

    // Plain fn-local use -> one error at the use's line.
    #[test]
    fn check_should_explain_hoisting_when_use_is_unguarded() {
        let diags = checks("fn f() {\n    use std::io;\n}\n");

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, CODE_MOD002);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].line, 2);
        assert_eq!(
            diags[0].message,
            "function-local `use` lacks its own `#[cfg]`.\n\
             - Hoist it to module scope so dependencies are easy to find.\n\
             - Keep it local only if it needs conditional compilation, \
             with `#[cfg]` on the `use`."
        );
    }

    // Own #[cfg] on the use -> no diagnostic.
    #[test]
    fn guarded_fn_local_use_stays_quiet() {
        assert!(checks("fn f() {\n    #[cfg(unix)]\n    use std::os::unix::fs;\n}\n").is_empty());
    }

    // #[cfg] on the fn or #[cfg_attr] on the use still fires: only the
    // use's own cfg attribute guards it.
    #[test]
    fn enclosing_cfg_and_cfg_attr_do_not_exempt_the_use() {
        let cfg_fn = "#[cfg(unix)]\nfn f() {\n    use std::os::unix::fs;\n}\n";
        assert_eq!(checks(cfg_fn).len(), 1);
        let cfg_attr = "fn f() {\n    #[cfg_attr(unix, allow(unused))]\n    use std::io;\n}\n";
        assert_eq!(checks(cfg_attr).len(), 1);
    }

    // Module-level use with or without cfg -> no diagnostic.
    #[test]
    fn module_level_use_stays_quiet() {
        assert!(
            checks("use std::io;\n#[cfg(unix)]\nuse std::os::unix::fs;\nfn f() {}\n").is_empty()
        );
    }

    // Use nested in a block or closure inside the body -> diagnostic.
    #[test]
    fn nested_block_and_closure_uses_error() {
        let source = "fn f() {\n    let g = || {\n        use std::io;\n    };\n    {\n        use std::fmt;\n    }\n    let _ = g;\n}\n";
        let diags = checks(source);
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[1].line, 6);
    }

    // Test and cfg(test) functions get no exemption.
    #[test]
    fn test_functions_are_not_exempt() {
        let test_fn = "#[test]\nfn t() {\n    use std::io;\n}\n";
        assert_eq!(checks(test_fn).len(), 1);
        let cfg_test_fn = "#[cfg(test)]\nfn t() {\n    use std::io;\n}\n";
        assert_eq!(checks(cfg_test_fn).len(), 1);
    }

    // A use in an inner fn is fn-local once: it flags exactly once even
    // though two function items enclose it.
    #[test]
    fn inner_function_use_errors_once() {
        let source =
            "fn outer() {\n    fn inner() {\n        use std::io;\n    }\n    inner();\n}\n";
        assert_eq!(checks(source).len(), 1);
    }
}
