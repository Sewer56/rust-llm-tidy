//! C# lint checks: the XML doc-comment dialect over the same codes and
//! [`Diagnostic`] shape the Rust checks emit.
//!
//! One module per rule, named by lint code.
//!
//! [`run`] walks the compilation unit and every declaration list in
//! document order, collecting one [`Declaration`] fact set per
//! declaration.
//!
//! Between collection and emission it computes the can-throw closure
//! and stamps the `<exception>` facts (`exception_scan`) of the
//! non-private throwing methods and constructors the closure flags.
//!
//! Every rule then runs over the collected facts in the same code order
//! the Rust backend emits.
//!
//! The text checks (TEXT*) follow from the same
//! parse's doc regions.
//!
//! # Semantics
//!
//! - DOC001: non-private documentable declarations (`public`, `internal`,
//!   `protected`-family modifiers) need a `///` doc comment.
//! - DOC002: a non-private method or constructor that can throw needs
//!   an `<exception>` tag (error severity). Throwing includes calls to
//!   same-file members and indexed qualified members that throw.
//! - DOC003: non-private can-throw members whose `<exception>` tags
//!   all lack a concrete `cref` type.
//!
//! Parameter and placeholder checks:
//!
//! - DOC004: non-private methods, constructors, and indexers with
//!   parameters need `<param name="...">` tags.
//! - DOC005: `<param>` tags must name every declared parameter.
//! - DOC006: placeholder markers (`TODO`/`FIXME`/`TBD`) in doc comments.
//! - DOC010: recognized doc tags must follow the canonical order
//!   (`inheritdoc` through `seealso`).
//!
//! Naming and prose checks:
//!
//! - TEST001: `TestMethod`/`Test`/`Fact`/`Theory`-marked methods with
//!   discouraged (`test_*`, `case_*`, `test` + digits) names.
//! - TEST002: test-marked methods with no comment above their attribute list.
//! - MOD003: fully-qualified dotted paths an in-scope `using` covers
//!   or that repeat past the configured threshold (hint severity).
//! - TEXT*: `///` doc-comment prose measured with the XML doc
//!   dialect; findings carry original file lines. The dialect rules live
//!   with the lint module's measuring core; see [`text_regions`]
//!   producer.
//!
//! [`text_regions`]: crate::languages::csharp::text_regions

use super::run_region_checks;
pub use crate::languages::csharp::analysis::can_throw::CanThrowIndex;
#[cfg(test)]
use crate::languages::csharp::analysis::declaration::tag_slices;
use crate::languages::csharp::analysis::declaration::{
    Declaration, THROWING, collect_children, exception_tags,
};
use crate::languages::csharp::text_regions::doc_regions;
use crate::reporting::{Diagnostic, Severity};
use crate::source::{ItemKind, ParseResult};

mod doc001_missing_docs;
mod doc002_missing_exception_tag;
mod doc003_vague_exception;
mod doc004_missing_param_tags;
mod doc005_undocumented_param;
mod doc006_placeholder;
mod doc010_tag_order;
mod mod003_qualified_path;
mod test001_test_naming;
mod test002_test_summary;

/// Kinds whose non-private declarations need doc comments.
const DOCUMENTABLE: &[ItemKind] = &[
    ItemKind::Class,
    ItemKind::Struct,
    ItemKind::Interface,
    ItemKind::Record,
    ItemKind::Enum,
    ItemKind::Delegate,
    ItemKind::Fn,
    ItemKind::Property,
    ItemKind::Event,
    ItemKind::Const,
    ItemKind::Static,
    ItemKind::Constructor,
];

impl Declaration<'_> {
    /// One diagnostic stamped with this declaration's line, kind, and
    /// name.
    fn diagnostic(
        &self,
        severity: Severity,
        code: &'static str,
        title: &str,
        message: String,
    ) -> Diagnostic {
        Diagnostic {
            title: Some(title.into()),
            severity,
            code,
            message,
            line: self.line,
            item_kind: self.kind.as_str().to_string(),
            item_name: self.name.clone(),
        }
    }
}

/// Run every C# check over `parsed`, returning all diagnostics:
/// declaration checks first, then the MOD003 hints, then the text
/// checks.
///
/// The declaration checks run first, then the whole-file MOD003 walk,
/// then the text checks (TEXT*) over the same parse's doc regions.
///
/// Returns no diagnostics when the parse tree carries error nodes: a
/// broken tree would report findings against misread declarations. The
/// whole pass therefore degrades to silence.
pub(crate) fn run(parsed: &ParseResult) -> Vec<Diagnostic> {
    run_indexed(parsed, None)
}

/// Run checks on `parsed` with optional shared throw answers from `shared`.
/// Returns diagnostics tiered as [`run`], or none for a tree with syntax
/// errors.
pub(crate) fn run_indexed(parsed: &ParseResult, shared: Option<&CanThrowIndex>) -> Vec<Diagnostic> {
    if parsed.syntax_tree().root_node().has_error() {
        return Vec::new();
    }

    let source = parsed.source.as_str();
    let mut declarations = Vec::with_capacity(parsed.items.len());
    collect_children(
        parsed.syntax_tree().root_node(),
        source,
        None,
        &mut declarations,
    );

    // The can-throw closure spans the whole file (a caller may sit
    // before its callee), so it runs between collection and the rules;
    // stamping from its answers keeps diagnostics in document order.
    let index = CanThrowIndex::from_declarations(&declarations);
    let shared = shared.map(|index| index.including(parsed));
    for (position, decl) in declarations.iter_mut().enumerate() {
        let throws = index.declaration_can_throw(position)
            || shared.as_ref().is_some_and(|shared| {
                decl.type_name
                    .as_deref()
                    .zip(decl.name.as_deref())
                    .is_some_and(|(owner, member)| shared.member_can_throw(owner, member))
            });
        if decl.non_private && THROWING.contains(&decl.kind) && throws {
            decl.exception_scan = Some(exception_tags(&decl.docs));
        }
    }

    let mut diagnostics = Vec::with_capacity(parsed.items.len());
    for decl in &declarations {
        check_declaration(decl, &mut diagnostics);
    }

    diagnostics.extend(mod003_qualified_path::check(parsed));

    diagnostics.extend(run_region_checks(doc_regions(parsed)));
    diagnostics
}

/// Run every rule over one collected declaration.
fn check_declaration(decl: &Declaration<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(doc001_missing_docs::check(decl));
    diagnostics.extend(doc002_missing_exception_tag::check(decl));
    diagnostics.extend(doc003_vague_exception::check(decl));
    diagnostics.extend(doc004_missing_param_tags::check(decl));
    diagnostics.extend(doc005_undocumented_param::check(decl));
    diagnostics.extend(doc006_placeholder::check(decl));
    diagnostics.extend(doc010_tag_order::check(decl));
    diagnostics.extend(test001_test_naming::check(decl));
    diagnostics.extend(test002_test_summary::check(decl));
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Diagnostic, run, tag_slices};
    use crate::rules::lint::CODE_MISSING_ERRORS;
    use std::collections::{HashMap, HashSet};

    /// The name-keyed and qualified-index paths emit identical complete diagnostics.
    #[test]
    fn index_should_match_name_keyed_diagnostics() {
        let sources = [
            include_str!(
                "../../../../../cli/tests/fixtures/doc/csharp/doc002_missing_exception.cs"
            ),
            include_str!(
                "../../../../../cli/tests/fixtures/doc/csharp/doc002_indirect_exception.cs"
            ),
            "class C { void A() { B(); } void B() { A(); throw new E(); } public void Caller() { A(); } }",
            "class C { C() { throw new E(); } public void Caller() { new C(); } }",
            "class C { void Helper() { throw new E(); } public void Caller() { System.Action a = () => { Helper(); }; void Local() { Helper(); } } }",
            "class C { class Nested { void Helper() { throw new E(); } } void Helper(int x) {} public void Caller() { obj.Helper(); } }",
        ];

        for source in sources {
            let parsed = crate::languages::csharp::parse::parse(source).expect("fixture parses");
            assert!(!parsed.syntax_tree().root_node().has_error());
            let mut declarations = Vec::new();
            super::collect_children(
                parsed.syntax_tree().root_node(),
                source,
                None,
                &mut declarations,
            );
            let flags = name_keyed_throw_closure(&declarations);
            let mut expected = Vec::new();
            for (decl, throws) in declarations.iter_mut().zip(flags) {
                if decl.non_private && super::THROWING.contains(&decl.kind) && throws {
                    decl.exception_scan = Some(super::exception_tags(&decl.docs));
                }
                super::check_declaration(decl, &mut expected);
            }
            expected.extend(super::mod003_qualified_path::check(&parsed));
            expected.extend(crate::languages::csharp::text_regions::text_checks(&parsed));

            let actual = run(&parsed);

            assert_eq!(format!("{actual:?}"), format!("{expected:?}"), "{source}");
        }
    }

    /// Evaluate a simple-name graph independently of qualified index positions.
    /// `declarations` supplies syntax bodies and the names available in one file.
    fn name_keyed_throw_closure(declarations: &[super::Declaration<'_>]) -> Vec<bool> {
        let names: HashSet<_> = declarations
            .iter()
            .filter(|decl| super::THROWING.contains(&decl.kind))
            .filter_map(|decl| decl.name.as_deref())
            .collect();
        let mut throwing = HashSet::new();
        let mut callers: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut direct = vec![false; declarations.len()];

        for (ordinal, decl) in declarations.iter().enumerate() {
            if !super::THROWING.contains(&decl.kind) {
                continue;
            }
            let mut cursor = decl.node.walk();
            'walk: loop {
                let node = cursor.node();
                if !matches!(
                    node.kind(),
                    "lambda_expression"
                        | "anonymous_method_expression"
                        | "local_function_statement"
                ) {
                    if node.kind() == "throw_statement" {
                        direct[ordinal] = true;
                        if let Some(name) = decl.name.as_deref() {
                            throwing.insert(name);
                        }
                    } else if let Some(field) = match node.kind() {
                        "invocation_expression" => Some("function"),
                        "object_creation_expression" => Some("type"),
                        _ => None,
                    } && let Some(caller) = decl.name.as_deref()
                        && let Some(target) = node.child_by_field_name(field)
                        && let Some(name) =
                            crate::languages::csharp::parse::call_target_name(target, decl.source)
                        && name != "nameof"
                        && names.contains(name)
                    {
                        callers.entry(name).or_default().push(caller);
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

        let mut work: Vec<_> = throwing.iter().copied().collect();
        while let Some(name) = work.pop() {
            if let Some(callers) = callers.get(name) {
                for &caller in callers {
                    if throwing.insert(caller) {
                        work.push(caller);
                    }
                }
            }
        }

        declarations
            .iter()
            .enumerate()
            .map(|(ordinal, decl)| {
                direct[ordinal]
                    || decl
                        .name
                        .as_deref()
                        .is_some_and(|name| throwing.contains(name))
            })
            .collect()
    }

    /// Full C# lint pass over `source`: the entry point every rule
    /// observes, shared by the can-throw tests.
    fn lint(source: &str) -> Vec<Diagnostic> {
        let parsed =
            crate::languages::csharp::parse::parse(source).expect("test source must parse");
        run(&parsed)
    }

    /// The DOC002 findings' member names of the pass; members without a
    /// name cannot be flagged, so they never appear.
    pub(crate) fn missing_exception_names(source: &str) -> Vec<String> {
        lint(source)
            .into_iter()
            .filter(|d| d.code == CODE_MISSING_ERRORS)
            .filter_map(|d| d.item_name)
            .collect()
    }

    // ── nested-callable scan boundaries ──

    /// A returned lambda's body runs on the caller's schedule, so a
    /// throwing call inside it must not flag the enclosing member;
    /// a direct call in the member's own body still does.
    #[test]
    fn deferred_lambda_calls_do_not_flag_enclosing_member() {
        let source = "\
class C {
    void Thrower() { throw new System.Exception(); }
    /// <summary>Returns a deferred thrower.</summary>
    public System.Func<int> Deferred() {
        return () => { Thrower(); return 0; };
    }
    /// <summary>Calls the thrower now.</summary>
    public int Direct() { Thrower(); return 0; }
}
";

        assert_eq!(missing_exception_names(source), ["Direct".to_string()]);
    }

    /// An uncalled local function's body runs only when invoked, so its
    /// throwing call must not flag the enclosing member.
    #[test]
    fn local_function_calls_do_not_flag_enclosing_member() {
        let source = "\
class C {
    void Thrower() { throw new System.Exception(); }
    /// <summary>Wraps a local function.</summary>
    public int Outer() {
        int Local() { Thrower(); return 0; }
        return 1;
    }
}
";

        assert_eq!(missing_exception_names(source), Vec::<String>::new());
    }

    // ── whole-tag-name matching ──

    /// `<paramref>` shares the `<param` prefix but is its own tag, so a
    /// `param` scan must not count it.
    #[test]
    fn tag_slices_skips_tags_sharing_only_a_prefix() {
        let docs = vec![" <paramref name=\"key\"/>".to_string()];

        assert_eq!(tag_slices(&docs, "param").count(), 0);
    }

    /// A real `<param>` tag matches exactly once, whether it carries
    /// attributes, closes, or self-closes.
    #[test]
    fn tag_slices_matches_whole_tag_names() {
        let docs = vec![
            " <param name=\"key\">The key.</param>".to_string(),
            " <param>".to_string(),
            " <param/>".to_string(),
        ];

        let slices: Vec<&str> = tag_slices(&docs, "param").collect();
        assert_eq!(slices.len(), 3, "one slice per whole-name tag: {slices:?}");
        assert!(slices[0].starts_with("<param name=\"key\""));
    }
}
